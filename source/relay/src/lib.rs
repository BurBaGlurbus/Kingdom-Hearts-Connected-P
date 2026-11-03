//! The rendezvous and relay server, as a state machine: a datagram and the time
//! go in, datagrams come out, and it never touches a socket (`main.rs` does).
//!
//! A host registers and is given a room code. A joiner looks the code up, and each
//! is told the other's public address, so they can try each other directly. If that
//! does not work inside a few seconds they switch to sending everything here, and it
//! is forwarded between the two by the room's key. Nothing here reads what it forwards.
//!
//! [duel choice, KHC-018] Limits: one room per code; a room nobody has touched for 60
//! seconds goes, so a waiting host refreshes it; a rate limit per source address, for
//! control messages and for forwarding apart; and no reply larger than its request, so
//! this server cannot be used to multiply someone's traffic.

use net::wire::{CODE_ALPHABET, CODE_LENGTH, Datagram, RELAYED_LIMIT};
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// How long a room lives untouched.
pub const ROOM_LIFE: Duration = Duration::from_secs(60);
/// Rooms held in all, and per source address.
const MAX_ROOMS: usize = 4096;
const MAX_ROOMS_PER_SOURCE: usize = 8;
/// Control messages a source may send: a burst, and how many a second go on.
const CONTROL_BURST: f32 = 20.;
const CONTROL_RATE: f32 = 10.;
/// Datagrams it may have forwarded: a duel is a few hundred a second at its busiest.
const FORWARD_BURST: f32 = 2000.;
const FORWARD_RATE: f32 = 1000.;
/// Sources remembered before the idle ones are dropped.
const MAX_SOURCES: usize = 16_384;

#[derive(Clone, Debug)]
struct Room {
    code: [u8; CODE_LENGTH],
    key: u64,
    host: SocketAddr,
    host_nonce: u64,
    joiner: Option<(SocketAddr, u64)>,
    touched: Duration,
}

/// A token bucket.
#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: f32,
    at: Duration,
}

impl Bucket {
    fn take(&mut self, now: Duration, burst: f32, rate: f32) -> bool {
        let passed = now.saturating_sub(self.at).as_secs_f32();
        self.at = now;
        self.tokens = (self.tokens + passed * rate).min(burst);
        if self.tokens >= 1. {
            self.tokens -= 1.;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
pub struct Relay {
    /// Rooms by code.
    rooms: HashMap<[u8; CODE_LENGTH], Room>,
    /// Codes by key.
    keys: HashMap<u64, [u8; CODE_LENGTH]>,
    control: HashMap<IpAddr, Bucket>,
    forward: HashMap<IpAddr, Bucket>,
    /// [KHC-062] Codes and keys are SipHash of a counter under a key drawn from
    /// the OS: one answer does not tell anyone the next (a plain generator's did).
    secret: RandomState,
    seed: u64,
    drawn: u64,
    swept: Duration,
    /// Datagrams refused for each reason, for the operator.
    pub rate_limited: u64,
}

impl Relay {
    /// `seed` is mixed into codes and keys; their secrecy comes from the OS's
    /// randomness behind `RandomState`, which the relay draws itself.
    pub fn new(seed: u64) -> Relay {
        Relay {
            rooms: HashMap::new(),
            keys: HashMap::new(),
            control: HashMap::new(),
            forward: HashMap::new(),
            secret: RandomState::new(),
            seed,
            drawn: 0,
            swept: Duration::ZERO,
            rate_limited: 0,
        }
    }

    pub fn rooms(&self) -> usize {
        self.rooms.len()
    }

    /// The next 64 unpredictable bits: SipHash of a counter under the secret key.
    fn next(&mut self) -> u64 {
        let mut h = self.secret.build_hasher();
        h.write_u64(self.seed);
        h.write_u64(self.drawn);
        self.drawn = self.drawn.wrapping_add(1);
        h.finish()
    }

    fn fresh_code(&mut self) -> [u8; CODE_LENGTH] {
        loop {
            let mut bits = self.next();
            let mut code = [0; CODE_LENGTH];
            for c in &mut code {
                *c = CODE_ALPHABET[(bits % CODE_ALPHABET.len() as u64) as usize];
                bits /= CODE_ALPHABET.len() as u64;
            }
            if !self.rooms.contains_key(&code) {
                return code;
            }
        }
    }

    fn fresh_key(&mut self) -> u64 {
        loop {
            let key = self.next() | 1;
            if !self.keys.contains_key(&key) {
                return key;
            }
        }
    }

    fn sweep(&mut self, now: Duration) {
        if now.saturating_sub(self.swept) < Duration::from_secs(1) {
            return;
        }
        self.swept = now;
        let keys = &mut self.keys;
        self.rooms.retain(|_, room| {
            let live = now.saturating_sub(room.touched) < ROOM_LIFE;
            if !live {
                keys.remove(&room.key);
            }
            live
        });
        for table in [&mut self.control, &mut self.forward] {
            if table.len() > MAX_SOURCES {
                table.retain(|_, b| now.saturating_sub(b.at) < ROOM_LIFE);
            }
        }
    }

    fn allow_control(&mut self, now: Duration, from: IpAddr) -> bool {
        let bucket = self.control.entry(from).or_insert(Bucket {
            tokens: CONTROL_BURST,
            at: now,
        });
        let ok = bucket.take(now, CONTROL_BURST, CONTROL_RATE);
        self.rate_limited += u64::from(!ok);
        ok
    }

    /// One datagram from `from` at `now`: what goes out for it.
    pub fn handle(
        &mut self,
        now: Duration,
        from: SocketAddr,
        bytes: &[u8],
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        self.sweep(now);
        let Some(datagram) = Datagram::decode(bytes) else {
            return Vec::new();
        };
        match datagram {
            Datagram::Register { code, nonce } => {
                if !self.allow_control(now, from.ip()) {
                    return Vec::new();
                }
                self.register(now, from, code, nonce, bytes.len())
            }
            Datagram::Lookup { code, nonce } => {
                if !self.allow_control(now, from.ip()) {
                    return Vec::new();
                }
                self.lookup(now, from, code, nonce, bytes.len())
            }
            Datagram::Relayed {
                key,
                from_joiner,
                inner,
            } => self.forward(now, from, key, from_joiner, inner, bytes),
            _ => Vec::new(),
        }
    }

    fn register(
        &mut self,
        now: Duration,
        from: SocketAddr,
        wanted: [u8; CODE_LENGTH],
        nonce: u64,
        asked: usize,
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        // A refresh, or a repeat of a Register whose answer was lost: the same
        // room for the same host's same nonce, even from a new address.
        let mine = self
            .rooms
            .values()
            .find(|r| r.host_nonce == nonce && (r.host == from || r.code == wanted))
            .map(|r| r.code);
        let code = match mine {
            Some(code) => code,
            None => {
                let held = self
                    .rooms
                    .values()
                    .filter(|r| r.host.ip() == from.ip())
                    .count();
                if self.rooms.len() >= MAX_ROOMS || held >= MAX_ROOMS_PER_SOURCE {
                    self.rate_limited += 1;
                    return Vec::new();
                }
                let code = self.fresh_code();
                let key = self.fresh_key();
                self.keys.insert(key, code);
                self.rooms.insert(
                    code,
                    Room {
                        code,
                        key,
                        host: from,
                        host_nonce: nonce,
                        joiner: None,
                        touched: now,
                    },
                );
                code
            }
        };
        let Some(room) = self.rooms.get_mut(&code) else {
            return Vec::new();
        };
        room.host = from;
        room.touched = now;
        let reply = Datagram::Registered {
            nonce,
            code,
            key: room.key,
        }
        .encode();
        out_if_smaller(from, reply, asked)
    }

    fn lookup(
        &mut self,
        now: Duration,
        from: SocketAddr,
        code: [u8; CODE_LENGTH],
        nonce: u64,
        asked: usize,
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        let room = self.rooms.get_mut(&code);
        // One joiner to a room: another with the same code hears there is none, as
        // a stranger guessing does.
        let Some(room) = room.filter(|r| r.joiner.is_none_or(|j| j.1 == nonce)) else {
            return out_if_smaller(from, Datagram::NoRoom { nonce }.encode(), asked);
        };
        room.joiner = Some((from, nonce));
        room.touched = now;
        let (Ok(host_at), Ok(joiner_at)) = (ipv4(room.host), ipv4(from)) else {
            return Vec::new();
        };
        let to_joiner = Datagram::PeerAt {
            nonce,
            key: room.key,
            addr: host_at,
        }
        .encode();
        let to_host = Datagram::PeerAt {
            nonce: room.host_nonce,
            key: room.key,
            addr: joiner_at,
        }
        .encode();
        let mut out = out_if_smaller(from, to_joiner, asked);
        out.extend(out_if_smaller(room.host, to_host, asked));
        out
    }

    fn forward(
        &mut self,
        now: Duration,
        from: SocketAddr,
        key: u64,
        from_joiner: bool,
        inner: Vec<u8>,
        bytes: &[u8],
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        let bucket = self.forward.entry(from.ip()).or_insert(Bucket {
            tokens: FORWARD_BURST,
            at: now,
        });
        if !bucket.take(now, FORWARD_BURST, FORWARD_RATE) {
            self.rate_limited += 1;
            return Vec::new();
        }
        if inner.len() > RELAYED_LIMIT {
            return Vec::new();
        }
        let Some(room) = self
            .keys
            .get(&key)
            .and_then(|code| self.rooms.get_mut(code))
        else {
            return Vec::new();
        };
        room.touched = now;
        // The key is what proves a sender, so an address can change on the way
        // (a rejoin from another network).
        let to = if from_joiner {
            match room.joiner.as_mut() {
                Some(joiner) => joiner.0 = from,
                None => room.joiner = Some((from, 0)),
            }
            Some(room.host)
        } else {
            room.host = from;
            room.joiner.map(|j| j.0)
        };
        to.map(|to| vec![(to, bytes.to_vec())]).unwrap_or_default()
    }
}

fn ipv4(addr: SocketAddr) -> Result<std::net::SocketAddrV4, ()> {
    match addr {
        SocketAddr::V4(v4) => Ok(v4),
        SocketAddr::V6(_) => Err(()),
    }
}

/// A reply, unless it is larger than what asked for it.
fn out_if_smaller(to: SocketAddr, reply: Vec<u8>, asked: usize) -> Vec<(SocketAddr, Vec<u8>)> {
    if reply.len() > asked {
        return Vec::new();
    }
    vec![(to, reply)]
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_CODE: [u8; CODE_LENGTH] = [0; CODE_LENGTH];

    fn at(n: u8, port: u16) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, n], port))
    }

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn register(r: &mut Relay, now: Duration, from: SocketAddr, nonce: u64) -> ([u8; 6], u64) {
        let asked = Datagram::Register {
            code: NO_CODE,
            nonce,
        }
        .encode();
        let out = r.handle(now, from, &asked);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].0, from);
        match Datagram::decode(&out[0].1) {
            Some(Datagram::Registered {
                nonce: n,
                code,
                key,
            }) => {
                assert_eq!(n, nonce);
                (code, key)
            }
            other => panic!("{other:?}"),
        }
    }

    fn lookup(code: [u8; 6], nonce: u64) -> Vec<u8> {
        Datagram::Lookup { code, nonce }.encode()
    }

    #[test]
    fn a_code_has_no_look_alikes_and_a_repeat_keeps_its_room() {
        let mut r = Relay::new(99);
        for n in 0..8u8 {
            let (code, _) = register(&mut r, secs(0), at(n + 1, 1000), u64::from(n));
            assert!(code.iter().all(|c| CODE_ALPHABET.contains(c)), "{code:?}");
            for bad in b"01OIL" {
                assert!(!code.contains(bad), "{code:?}");
            }
        }
        let (a, k) = register(&mut r, secs(0), at(1, 1000), 0);
        let (b, l) = register(&mut r, secs(1), at(1, 1000), 0);
        assert_eq!((a, k), (b, l));
        assert_eq!(r.rooms(), 8);
    }

    #[test]
    fn a_lookup_tells_each_where_the_other_is() {
        let mut r = Relay::new(1);
        let (host, joiner) = (at(1, 5000), at(2, 6000));
        let (code, key) = register(&mut r, secs(0), host, 7);
        let out = r.handle(secs(1), joiner, &lookup(code, 8));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, joiner);
        assert_eq!(
            Datagram::decode(&out[0].1),
            Some(Datagram::PeerAt {
                nonce: 8,
                key,
                addr: "10.0.0.1:5000".parse().unwrap()
            })
        );
        assert_eq!(out[1].0, host);
        assert_eq!(
            Datagram::decode(&out[1].1),
            Some(Datagram::PeerAt {
                nonce: 7,
                key,
                addr: "10.0.0.2:6000".parse().unwrap()
            })
        );
    }

    #[test]
    fn one_joiner_to_a_room_and_a_wrong_code_finds_nothing() {
        let mut r = Relay::new(2);
        let (code, _) = register(&mut r, secs(0), at(1, 5000), 7);
        let refused = |out: Vec<(SocketAddr, Vec<u8>)>| {
            assert_eq!(out.len(), 1);
            assert!(matches!(
                Datagram::decode(&out[0].1),
                Some(Datagram::NoRoom { .. })
            ));
        };
        refused(r.handle(secs(1), at(9, 1), &lookup(*b"ZZZZZZ", 1)));
        assert_eq!(r.handle(secs(1), at(2, 6000), &lookup(code, 8)).len(), 2);
        // Asked again by the same joiner: the same answer.
        assert_eq!(r.handle(secs(1), at(2, 6000), &lookup(code, 8)).len(), 2);
        refused(r.handle(secs(1), at(3, 6000), &lookup(code, 9)));
    }

    #[test]
    fn a_room_nobody_touches_goes_after_a_minute_and_a_refresh_keeps_it() {
        let mut r = Relay::new(3);
        let host = at(1, 5000);
        let (code, _) = register(&mut r, secs(0), host, 7);
        // Another source's traffic keeps the sweep going.
        r.handle(secs(59), at(9, 1), b"junk");
        assert_eq!(r.rooms(), 1);
        r.handle(secs(61), at(9, 1), b"junk");
        assert_eq!(r.rooms(), 0);
        let out = r.handle(secs(62), at(2, 6000), &lookup(code, 8));
        assert!(matches!(
            Datagram::decode(&out[0].1),
            Some(Datagram::NoRoom { .. })
        ));
        // Refreshed every 20 seconds it stays, for as long as it takes.
        let (code, key) = register(&mut r, secs(100), host, 11);
        for t in 1..20 {
            let (c, k) = register(&mut r, secs(100 + t * 20), host, 11);
            assert_eq!((c, k), (code, key));
        }
        assert_eq!(r.rooms(), 1);
    }

    #[test]
    fn control_messages_are_rate_limited_per_source() {
        let mut r = Relay::new(4);
        let noisy = at(9, 1);
        let mut answered = 0;
        for _ in 0..100 {
            answered += r.handle(secs(0), noisy, &lookup(*b"ZZZZZZ", 1)).len();
        }
        assert_eq!(answered, CONTROL_BURST as usize);
        assert_eq!(r.rate_limited, 100 - CONTROL_BURST as u64);
        // Another address is not held back by it.
        assert_eq!(r.handle(secs(0), at(8, 1), &lookup(*b"ZZZZZZ", 1)).len(), 1);
        // And it comes back with time.
        assert_eq!(r.handle(secs(1), noisy, &lookup(*b"ZZZZZZ", 1)).len(), 1);
    }

    #[test]
    fn one_address_cannot_hold_many_rooms() {
        let mut r = Relay::new(5);
        for n in 0..MAX_ROOMS_PER_SOURCE as u64 {
            register(&mut r, secs(n * 2), at(1, 5000), n);
        }
        let asked = Datagram::Register {
            code: NO_CODE,
            nonce: 99,
        }
        .encode();
        assert!(r.handle(secs(20), at(1, 5000), &asked).is_empty());
    }

    #[test]
    fn no_reply_is_larger_than_its_request() {
        let mut r = Relay::new(6);
        let (host, joiner) = (at(1, 5000), at(2, 6000));
        let asked = Datagram::Register {
            code: NO_CODE,
            nonce: 7,
        }
        .encode();
        let out = r.handle(secs(0), host, &asked);
        assert!(out[0].1.len() <= asked.len());
        let Some(Datagram::Registered { code, .. }) = Datagram::decode(&out[0].1) else {
            panic!("registered");
        };
        let asked = lookup(code, 8);
        for (_, reply) in r.handle(secs(0), joiner, &asked) {
            assert!(reply.len() <= asked.len());
        }
        let asked = lookup(*b"ZZZZZZ", 8);
        for (_, reply) in r.handle(secs(0), joiner, &asked) {
            assert!(reply.len() <= asked.len());
        }
        // A short request gets nothing at all.
        assert!(
            r.handle(secs(0), joiner, &asked[..asked.len() - 1])
                .is_empty()
        );
    }

    #[test]
    fn data_is_forwarded_between_the_two_by_the_key() {
        let mut r = Relay::new(7);
        let (host, joiner) = (at(1, 5000), at(2, 6000));
        let (code, key) = register(&mut r, secs(0), host, 7);
        r.handle(secs(0), joiner, &lookup(code, 8));
        let relayed = |from_joiner, inner: &[u8]| {
            Datagram::Relayed {
                key,
                from_joiner,
                inner: inner.to_vec(),
            }
            .encode()
        };
        let up = relayed(true, b"up");
        assert_eq!(r.handle(secs(1), joiner, &up), [(host, up.clone())]);
        let down = relayed(false, b"down");
        assert_eq!(r.handle(secs(1), host, &down), [(joiner, down.clone())]);
        // A wrong key reaches nobody.
        let stray = Datagram::Relayed {
            key: key ^ 2,
            from_joiner: true,
            inner: vec![1],
        }
        .encode();
        assert!(r.handle(secs(1), at(9, 1), &stray).is_empty());
        // A joiner back from another address is found there.
        let moved = at(3, 7000);
        assert_eq!(r.handle(secs(2), moved, &up), [(host, up.clone())]);
        assert_eq!(r.handle(secs(2), host, &down), [(moved, down)]);
        // Forwarding keeps the room alive past a minute.
        for t in 1..10 {
            r.handle(secs(2 + t * 30), moved, &up);
        }
        assert_eq!(r.rooms(), 1);
    }

    #[test]
    fn forwarding_is_rate_limited_and_too_large_a_datagram_goes_nowhere() {
        let mut r = Relay::new(8);
        let (host, joiner) = (at(1, 5000), at(2, 6000));
        let (code, key) = register(&mut r, secs(0), host, 7);
        r.handle(secs(0), joiner, &lookup(code, 8));
        let big = Datagram::Relayed {
            key,
            from_joiner: true,
            inner: vec![0; RELAYED_LIMIT + 1],
        }
        .encode();
        assert!(r.handle(secs(0), joiner, &big).is_empty());
        let up = Datagram::Relayed {
            key,
            from_joiner: true,
            inner: vec![1],
        }
        .encode();
        let went: usize = (0..3000)
            .map(|_| r.handle(secs(0), joiner, &up).len())
            .sum();
        assert_eq!(went, FORWARD_BURST as usize - 1);
    }
}
