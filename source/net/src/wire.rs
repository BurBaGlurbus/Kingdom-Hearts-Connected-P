//! Datagram framing and a small byte writer/reader.
//!
//! Every datagram starts with [`MAGIC`] and a kind byte. Hello, Welcome,
//! Reject, Bye and Stopped run the session; Data carries renet's packets; Discover
//! and Announce find hosts on the LAN. Register, Registered, Lookup, PeerAt and
//! NoRoom find a host by room code through the relay, Punch opens a path between
//! two players, and Relayed carries any of the session's through the relay. All
//! numbers are little-endian; a datagram that does not parse is ignored.

use std::net::SocketAddrV4;

/// Datagram prefix: "KH2D".
pub const MAGIC: [u8; 4] = *b"KH2D";
/// Bumped whenever a datagram or game message changes shape.
/// 14: Hello and Welcome carry the player's id (KHC-044).
pub const PROTOCOL: u16 = 14;
/// The first protocol whose Hello and Announce carry a readable version.
const VERSIONED: u16 = 8;
/// A player id's bytes (the hub's `PlayerId`; all zeros: none).
pub const PLAYER_ID: usize = 16;
/// Longest player name kept, in bytes of UTF-8.
pub const NAME_LIMIT: usize = 32;
/// Longest version text kept, in bytes of UTF-8.
pub const VERSION_LIMIT: usize = 40;
/// Longest reason text kept, in bytes of UTF-8.
pub const REASON_LIMIT: usize = 255;

/// The most a Relayed datagram carries: a renet packet with its Data framing.
pub const RELAYED_LIMIT: usize = 1500;
/// A room code's length, and what it is made of: no letter or digit that can be
/// taken for another (no 0, O, 1, I or L).
pub const CODE_LENGTH: usize = 6;
pub const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// Padding in what asks the relay for something: its answers are no larger than
/// what asked, so nobody can aim them at someone else for profit.
const ASK_PADDING: [u8; 8] = [0; 8];

/// A room code as a player types it: any case, surrounding spaces ignored.
pub fn parse_code(text: &str) -> Option<[u8; CODE_LENGTH]> {
    let text = text.trim().to_ascii_uppercase();
    let code: [u8; CODE_LENGTH] = text.as_bytes().try_into().ok()?;
    code.iter()
        .all(|c| CODE_ALPHABET.contains(c))
        .then_some(code)
}

/// The build identity both sides must share (a hash of the executable).
pub type Build = [u8; 32];

/// Where a player stands with another game's files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cap {
    /// Not on this PC, or it cannot be read.
    #[default]
    Absent,
    /// There, and not read yet.
    Unread,
    /// Read: the hash of the bytes the duel reads.
    Hash(Build),
}

/// What a player can show and fight with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub kh3: Cap,
    pub bbs: Cap,
}

impl Capabilities {
    pub fn put(&self, w: &mut Writer) {
        for cap in [self.kh3, self.bbs] {
            match cap {
                Cap::Absent => w.u8(0),
                Cap::Unread => w.u8(1),
                Cap::Hash(hash) => {
                    w.u8(2);
                    w.bytes(&hash);
                }
            }
        }
    }

    pub fn get(r: &mut Reader) -> Option<Capabilities> {
        fn one(r: &mut Reader) -> Option<Cap> {
            Some(match r.u8()? {
                0 => Cap::Absent,
                1 => Cap::Unread,
                2 => Cap::Hash(r.array()?),
                _ => return None,
            })
        }
        Some(Capabilities {
            kh3: one(r)?,
            bbs: one(r)?,
        })
    }
}

/// A player as a refusal names them.
#[derive(Clone, Copy, Debug)]
pub struct Identity<'a> {
    pub protocol: u16,
    pub build: &'a Build,
    pub version: &'a str,
    pub caps: Capabilities,
}

/// The first bytes of a hash, for a person to compare.
pub fn short_hash(hash: &Build) -> String {
    hash[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// Why a host refuses a joiner, if it does: another protocol or executable, each
/// mismatch on its own line. KH3's and BBS's files do not refuse a player (the lobby
/// keeps a duel off what they differ on), but a refusal says so while it is at it.
/// The game's font has no parentheses, so a line holds none.
pub fn refusal(host: &Identity, joiner: &Identity) -> Option<String> {
    if host.protocol == joiner.protocol && host.build == joiner.build {
        return None;
    }
    let mut lines = vec!["a different build, and both players need the same one".to_owned()];
    if host.protocol != joiner.protocol {
        lines.push(format!(
            "Protocol: host {}, you {}",
            host.protocol, joiner.protocol
        ));
    }
    if host.build != joiner.build {
        lines.push("Executable differs".to_owned());
        lines.push(format!(
            "Host: {} id {}",
            host.version,
            short_hash(host.build)
        ));
        lines.push(format!(
            "You: {} id {}",
            joiner.version,
            short_hash(joiner.build)
        ));
    }
    let side = |cap: Cap| match cap {
        Cap::Absent => "none".to_owned(),
        Cap::Unread => "not read yet".to_owned(),
        Cap::Hash(hash) => short_hash(&hash),
    };
    for (name, h, j) in [
        ("KH3 files", host.caps.kh3, joiner.caps.kh3),
        ("BBS arena files", host.caps.bbs, joiner.caps.bbs),
    ] {
        if h != j {
            lines.push(format!("{name}: host {}, you {}", side(h), side(j)));
        }
    }
    Some(lines.join("\n"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Datagram {
    /// Joiner → host: ask to join with a session nonce.
    Hello {
        protocol: u16,
        build: Build,
        nonce: u64,
        /// The workspace version and short commit, for a person to read.
        version: String,
        name: String,
        caps: Capabilities,
        /// The token of a seat being held for this player (0: none).
        resume: u64,
        /// [KHC-044] The player's id, unverified until ids are signed.
        player: [u8; PLAYER_ID],
    },
    /// Host → joiner: accepted.
    Welcome {
        nonce: u64,
        name: String,
        version: String,
        caps: Capabilities,
        /// Brings this player back to their seat after a drop.
        token: u64,
        /// [KHC-044] The host's player id.
        player: [u8; PLAYER_ID],
    },
    /// Host → joiner: refused, with the reason to show.
    Reject { nonce: u64, reason: String },
    /// A renet packet of the session `nonce`.
    Data { nonce: u64, payload: Vec<u8> },
    /// Either side: the session ends.
    Bye { nonce: u64 },
    /// Either side: this game stopped on an error (KHC-028).
    Stopped { nonce: u64 },
    /// Joiner → anyone on the game port: who is hosting?
    Discover { protocol: u16 },
    /// Host → the asker: a host, its name, and whether it can take a player.
    Announce {
        protocol: u16,
        build: Build,
        version: String,
        name: String,
        open: bool,
    },
    /// Host → relay: give me a room, or keep this one (`nonce` is the host's).
    Register { code: [u8; CODE_LENGTH], nonce: u64 },
    /// Relay → host: your room's code, and the key that forwards data in it.
    Registered {
        nonce: u64,
        code: [u8; CODE_LENGTH],
        key: u64,
    },
    /// Joiner → relay: who has this room?
    Lookup { code: [u8; CODE_LENGTH], nonce: u64 },
    /// Relay → each of the two: the other's public address, and the room's key.
    PeerAt {
        nonce: u64,
        key: u64,
        addr: SocketAddrV4,
    },
    /// Relay → joiner: no such room, or it has its player.
    NoRoom { nonce: u64 },
    /// Either player → the other's public address: opens the way through a router.
    Punch { nonce: u64 },
    /// Through the relay, by the room's key: any datagram of the session whole.
    Relayed {
        key: u64,
        from_joiner: bool,
        inner: Vec<u8>,
    },
}

const HELLO: u8 = 1;
const WELCOME: u8 = 2;
const REJECT: u8 = 3;
const DATA: u8 = 4;
const BYE: u8 = 5;
const DISCOVER: u8 = 6;
const ANNOUNCE: u8 = 7;
const STOPPED: u8 = 8;
const REGISTER: u8 = 9;
const REGISTERED: u8 = 10;
const LOOKUP: u8 = 11;
const PEER_AT: u8 = 12;
const NO_ROOM: u8 = 13;
const PUNCH: u8 = 14;
const RELAYED: u8 = 15;

impl Datagram {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(&MAGIC);
        match self {
            Datagram::Hello {
                protocol,
                build,
                nonce,
                version,
                name,
                caps,
                resume,
                player,
            } => {
                w.u8(HELLO);
                w.u16(*protocol);
                w.bytes(build);
                w.u64(*nonce);
                w.text(version, VERSION_LIMIT);
                w.text(name, NAME_LIMIT);
                caps.put(&mut w);
                w.u64(*resume);
                w.bytes(player);
            }
            Datagram::Welcome {
                nonce,
                name,
                version,
                caps,
                token,
                player,
            } => {
                w.u8(WELCOME);
                w.u64(*nonce);
                w.text(name, NAME_LIMIT);
                w.text(version, VERSION_LIMIT);
                caps.put(&mut w);
                w.u64(*token);
                w.bytes(player);
            }
            Datagram::Reject { nonce, reason } => {
                w.u8(REJECT);
                w.u64(*nonce);
                w.text(reason, REASON_LIMIT);
            }
            Datagram::Data { nonce, payload } => {
                w.u8(DATA);
                w.u64(*nonce);
                w.bytes(payload);
            }
            Datagram::Bye { nonce } => {
                w.u8(BYE);
                w.u64(*nonce);
            }
            Datagram::Stopped { nonce } => {
                w.u8(STOPPED);
                w.u64(*nonce);
            }
            Datagram::Discover { protocol } => {
                w.u8(DISCOVER);
                w.u16(*protocol);
            }
            Datagram::Announce {
                protocol,
                build,
                version,
                name,
                open,
            } => {
                w.u8(ANNOUNCE);
                w.u16(*protocol);
                w.bytes(build);
                w.text(version, VERSION_LIMIT);
                w.text(name, NAME_LIMIT);
                w.u8(u8::from(*open));
            }
            Datagram::Register { code, nonce } => {
                w.u8(REGISTER);
                w.bytes(code);
                w.u64(*nonce);
                w.bytes(&ASK_PADDING);
            }
            Datagram::Registered { nonce, code, key } => {
                w.u8(REGISTERED);
                w.u64(*nonce);
                w.bytes(code);
                w.u64(*key);
            }
            Datagram::Lookup { code, nonce } => {
                w.u8(LOOKUP);
                w.bytes(code);
                w.u64(*nonce);
                w.bytes(&ASK_PADDING);
            }
            Datagram::PeerAt { nonce, key, addr } => {
                w.u8(PEER_AT);
                w.u64(*nonce);
                w.u64(*key);
                w.bytes(&addr.ip().octets());
                w.u16(addr.port());
            }
            Datagram::NoRoom { nonce } => {
                w.u8(NO_ROOM);
                w.u64(*nonce);
            }
            Datagram::Punch { nonce } => {
                w.u8(PUNCH);
                w.u64(*nonce);
            }
            Datagram::Relayed {
                key,
                from_joiner,
                inner,
            } => {
                w.u8(RELAYED);
                w.u64(*key);
                w.u8(u8::from(*from_joiner));
                w.bytes(inner);
            }
        }
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Datagram> {
        let mut r = Reader::new(bytes);
        if r.take(4)? != MAGIC {
            return None;
        }
        let datagram = match r.u8()? {
            HELLO => {
                // Another protocol's Hello is still read as far as its version
                // and name, so its player can be told why they are refused.
                let protocol = r.u16()?;
                let build = r.array()?;
                let nonce = r.u64()?;
                let version = if protocol < VERSIONED {
                    format!("protocol {protocol}")
                } else {
                    r.text(VERSION_LIMIT)?
                };
                let name = r.text(NAME_LIMIT)?;
                let (caps, resume, player) = if protocol == PROTOCOL {
                    (Capabilities::get(&mut r)?, r.u64()?, r.array()?)
                } else {
                    r.rest();
                    (Capabilities::default(), 0, [0; PLAYER_ID])
                };
                Datagram::Hello {
                    protocol,
                    build,
                    nonce,
                    version,
                    name,
                    caps,
                    resume,
                    player,
                }
            }
            WELCOME => Datagram::Welcome {
                nonce: r.u64()?,
                name: r.text(NAME_LIMIT)?,
                version: r.text(VERSION_LIMIT)?,
                caps: Capabilities::get(&mut r)?,
                token: r.u64()?,
                player: r.array()?,
            },
            REJECT => Datagram::Reject {
                nonce: r.u64()?,
                reason: r.text(REASON_LIMIT)?,
            },
            DATA => {
                let nonce = r.u64()?;
                return Some(Datagram::Data {
                    nonce,
                    payload: r.rest().to_vec(),
                });
            }
            BYE => Datagram::Bye { nonce: r.u64()? },
            STOPPED => Datagram::Stopped { nonce: r.u64()? },
            DISCOVER => Datagram::Discover { protocol: r.u16()? },
            ANNOUNCE => Datagram::Announce {
                protocol: r.u16()?,
                build: r.array()?,
                version: r.text(VERSION_LIMIT)?,
                name: r.text(NAME_LIMIT)?,
                open: r.u8()? != 0,
            },
            REGISTER => {
                let (code, nonce) = (r.array()?, r.u64()?);
                let _: [u8; 8] = r.array()?;
                Datagram::Register { code, nonce }
            }
            REGISTERED => Datagram::Registered {
                nonce: r.u64()?,
                code: r.array()?,
                key: r.u64()?,
            },
            LOOKUP => {
                let (code, nonce) = (r.array()?, r.u64()?);
                let _: [u8; 8] = r.array()?;
                Datagram::Lookup { code, nonce }
            }
            PEER_AT => Datagram::PeerAt {
                nonce: r.u64()?,
                key: r.u64()?,
                addr: {
                    let ip: [u8; 4] = r.array()?;
                    SocketAddrV4::new(ip.into(), r.u16()?)
                },
            },
            NO_ROOM => Datagram::NoRoom { nonce: r.u64()? },
            PUNCH => Datagram::Punch { nonce: r.u64()? },
            RELAYED => {
                let key = r.u64()?;
                let from_joiner = r.u8()? != 0;
                return Some(Datagram::Relayed {
                    key,
                    from_joiner,
                    inner: r.rest().to_vec(),
                });
            }
            _ => return None,
        };
        r.is_empty().then_some(datagram)
    }
}

/// Cuts `text` to at most `limit` bytes on a character boundary.
pub fn clip(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Little-endian byte writer.
#[derive(Default)]
pub struct Writer(Vec<u8>);

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    /// The exact bits, so both sides hold the same value.
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    pub fn bytes(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }
    /// A length byte, then at most `limit` (≤ 255) bytes of UTF-8.
    pub fn text(&mut self, v: &str, limit: usize) {
        let v = clip(v, limit.min(255));
        self.u8(v.len() as u8);
        self.bytes(v.as_bytes());
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// Bounds-checked little-endian reader: every getter returns None past the
/// end.
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let s = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(s)
    }
    pub fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.array()?))
    }
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.array()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.array()?))
    }
    pub fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.array()?))
    }
    pub fn f32(&mut self) -> Option<f32> {
        Some(f32::from_bits(self.u32()?))
    }
    pub fn text(&mut self, limit: usize) -> Option<String> {
        let n = self.u8()? as usize;
        if n > limit {
            return None;
        }
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.bytes[self.at..];
        self.at = self.bytes.len();
        s
    }
    pub fn is_empty(&self) -> bool {
        self.at >= self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Datagram> {
        vec![
            Datagram::Hello {
                protocol: PROTOCOL,
                build: [7; 32],
                nonce: 0x0102_0304_0506_0708,
                version: "0.1.0 abc1234".into(),
                name: "Riku".into(),
                caps: Capabilities {
                    kh3: Cap::Hash([3; 32]),
                    bbs: Cap::Unread,
                },
                resume: 0x1122_3344_5566_7788,
                player: [0x42; PLAYER_ID],
            },
            Datagram::Welcome {
                nonce: 9,
                name: "Kairi".into(),
                version: "0.1.0 def5678".into(),
                caps: Capabilities {
                    kh3: Cap::Absent,
                    bbs: Cap::Hash([4; 32]),
                },
                token: 77,
                player: [0x24; PLAYER_ID],
            },
            Datagram::Reject {
                nonce: 9,
                reason: "match full".into(),
            },
            Datagram::Data {
                nonce: 3,
                payload: vec![1, 2, 3, 250],
            },
            Datagram::Data {
                nonce: 3,
                payload: Vec::new(),
            },
            Datagram::Bye { nonce: 11 },
            Datagram::Stopped { nonce: 12 },
            Datagram::Discover { protocol: PROTOCOL },
            Datagram::Announce {
                protocol: PROTOCOL,
                build: [1; 32],
                version: "0.1.0 abc1234".into(),
                name: "Host".into(),
                open: true,
            },
            Datagram::Register {
                code: *b"AB23CD",
                nonce: 5,
            },
            Datagram::Registered {
                nonce: 5,
                code: *b"AB23CD",
                key: 6,
            },
            Datagram::Lookup {
                code: *b"AB23CD",
                nonce: 7,
            },
            Datagram::PeerAt {
                nonce: 7,
                key: 6,
                addr: "203.0.113.9:47620".parse().unwrap(),
            },
            Datagram::NoRoom { nonce: 7 },
            Datagram::Punch { nonce: 7 },
            Datagram::Relayed {
                key: 6,
                from_joiner: true,
                inner: Datagram::Bye { nonce: 1 }.encode(),
            },
        ]
    }

    #[test]
    fn datagrams_round_trip() {
        for d in all() {
            assert_eq!(Datagram::decode(&d.encode()), Some(d));
        }
    }

    #[test]
    fn truncated_or_foreign_datagrams_are_ignored() {
        for d in all() {
            let bytes = d.encode();
            // Data and Relayed take the rest of the datagram, so only their
            // headers can be cut.
            let shortest = match d {
                Datagram::Data { .. } => 13,
                Datagram::Relayed { .. } => 14,
                _ => bytes.len(),
            };
            for n in 0..shortest {
                assert_eq!(Datagram::decode(&bytes[..n]), None, "{d:?} cut at {n}");
            }
        }
        assert_eq!(Datagram::decode(b"XXXX\x05\0\0\0\0\0\0\0\0"), None);
        let mut bytes = Datagram::Bye { nonce: 1 }.encode();
        bytes.push(0);
        assert_eq!(Datagram::decode(&bytes), None, "trailing bytes");
        let mut bytes = MAGIC.to_vec();
        bytes.push(99);
        assert_eq!(Datagram::decode(&bytes), None, "unknown kind");
    }

    #[test]
    fn names_are_clipped_on_a_character_boundary() {
        let long = "é".repeat(40);
        let d = Datagram::Welcome {
            nonce: 1,
            name: long,
            version: String::new(),
            caps: Capabilities::default(),
            token: 0,
            player: [0; PLAYER_ID],
        };
        let Some(Datagram::Welcome { name, .. }) = Datagram::decode(&d.encode()) else {
            panic!("welcome decodes");
        };
        assert_eq!(name, "é".repeat(16));
        assert_eq!(clip("abc", 2), "ab");
        assert_eq!(clip("aé", 2), "a");
    }

    #[test]
    fn another_protocol_s_hello_is_read_as_far_as_its_name() {
        // Protocol 7's Hello: its build, nonce and name, nothing more.
        let mut w = Writer::default();
        w.bytes(&MAGIC);
        w.u8(HELLO);
        w.u16(7);
        w.bytes(&[9; 32]);
        w.u64(5);
        w.text("Sora", NAME_LIMIT);
        let Some(Datagram::Hello { version, name, .. }) = Datagram::decode(&w.finish()) else {
            panic!("an older Hello decodes");
        };
        assert_eq!((version.as_str(), name.as_str()), ("protocol 7", "Sora"));
        // A later protocol's: whatever follows its name is not this one's to read.
        let mut w = Writer::default();
        w.bytes(&MAGIC);
        w.u8(HELLO);
        w.u16(PROTOCOL + 1);
        w.bytes(&[9; 32]);
        w.u64(5);
        w.text("9.9.9", VERSION_LIMIT);
        w.text("Sora", NAME_LIMIT);
        w.bytes(&[1, 2, 3]);
        let Some(Datagram::Hello { version, caps, .. }) = Datagram::decode(&w.finish()) else {
            panic!("a later Hello decodes");
        };
        assert_eq!(version, "9.9.9");
        assert_eq!(caps, Capabilities::default());
    }

    #[test]
    fn a_refusal_names_both_versions_and_each_mismatch_on_its_line() {
        let host = Identity {
            protocol: PROTOCOL,
            build: &[0xab; 32],
            version: "0.1.0 abc1234",
            caps: Capabilities {
                kh3: Cap::Hash([1; 32]),
                bbs: Cap::Hash([2; 32]),
            },
        };
        let same = host;
        assert_eq!(refusal(&host, &same), None);
        let joiner = Identity {
            protocol: PROTOCOL,
            build: &[0xcd; 32],
            version: "0.1.0 def5678",
            caps: Capabilities {
                kh3: Cap::Hash([9; 32]),
                bbs: Cap::Absent,
            },
        };
        let text = refusal(&host, &joiner).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(
            lines,
            [
                "a different build, and both players need the same one",
                "Executable differs",
                "Host: 0.1.0 abc1234 id abababab",
                "You: 0.1.0 def5678 id cdcdcdcd",
                "KH3 files: host 01010101, you 09090909",
                "BBS arena files: host 02020202, you none",
            ]
        );
        assert!(!text.contains('('), "the game's font has no parentheses");
        assert!(text.len() <= REASON_LIMIT);
        // Different files alone refuse nobody.
        let files_only = Identity {
            build: host.build,
            version: host.version,
            ..joiner
        };
        assert_eq!(refusal(&host, &files_only), None);
        let older = Identity {
            protocol: 7,
            ..files_only
        };
        assert!(
            refusal(&host, &older)
                .unwrap()
                .contains(&format!("Protocol: host {PROTOCOL}, you 7"))
        );
    }

    #[test]
    fn what_asks_the_relay_is_as_large_as_what_it_answers() {
        let ask = Datagram::Register {
            code: [0; CODE_LENGTH],
            nonce: 1,
        }
        .encode();
        let answer = Datagram::Registered {
            nonce: 1,
            code: [0; CODE_LENGTH],
            key: 2,
        }
        .encode();
        assert!(answer.len() <= ask.len());
        let ask = Datagram::Lookup {
            code: [0; CODE_LENGTH],
            nonce: 1,
        }
        .encode();
        let answer = Datagram::PeerAt {
            nonce: 1,
            key: 2,
            addr: "1.2.3.4:5".parse().unwrap(),
        }
        .encode();
        assert!(answer.len() <= ask.len());
    }

    #[test]
    fn a_room_code_is_read_as_a_player_types_it() {
        assert_eq!(parse_code(" ab23cd "), Some(*b"AB23CD"));
        assert_eq!(parse_code("AB23C"), None);
        assert_eq!(parse_code("AB23CDE"), None);
        assert_eq!(parse_code("AB23C0"), None, "no look-alikes");
        assert_eq!(parse_code("ABCDEO"), None);
    }

    #[test]
    fn reader_stops_at_the_end() {
        let mut w = Writer::default();
        w.u16(0xbeef);
        w.f32(-1.5);
        w.i32(-7);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u16(), Some(0xbeef));
        assert_eq!(r.f32(), Some(-1.5));
        assert_eq!(r.i32(), Some(-7));
        assert!(r.is_empty());
        assert_eq!(r.u8(), None);
    }
}
