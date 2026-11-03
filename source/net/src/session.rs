//! One session over one non-blocking UDP socket.
//!
//! The host binds the game port and takes one joiner: a Hello with the same
//! protocol and build gets a Welcome, anything else a Reject with its
//! reason. Once welcomed, renet's packets ride in Data datagrams. Either
//! side ends with Bye; a peer silent for longer than the timeout is gone, unless
//! the game holds their seat: then the joiner knocks again, with the token its
//! Welcome gave it, from whatever address it has by then, and the session goes on
//! under a new nonce (KHC-027). A host is not resumed: it owns the match.
//! The host also answers Discover with an Announce, which is how a joiner
//! finds hosts on the LAN. A playback session has no socket and no peer: what
//! it receives is what `feed` put in, for a replay. A host with a direct room
//! code (KHC-051) asks public STUN servers for its address and its router to
//! open the port; a joiner with one knocks on the address in it.

use crate::direct::{self, Mapping};
use crate::link::{Conditions, Link};
use crate::portmap::Mapper;
use crate::wire::{self, Build, CODE_LENGTH, Capabilities, Datagram, PROTOCOL};
use crate::{RELIABLE, UNRELIABLE};
use renet::{ChannelConfig, ConnectionConfig, RenetClient, RenetServer, SendType};
use std::collections::VecDeque;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Mutex;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// The one joiner's id inside renet's server.
const PEER: u64 = 1;
/// Why the session ended when the other game stopped on an error.
pub const STOPPED_REASON: &str = "the other player's game stopped (error)";
/// A Hello is repeated this often until the host answers.
const HELLO_EVERY: Duration = Duration::from_millis(250);
/// What goes to the relay, and a punch, is repeated this often until it is answered.
const ASK_EVERY: Duration = Duration::from_millis(250);
const PUNCH_EVERY: Duration = Duration::from_millis(100);
/// A host keeps its room with this: the relay lets one go after a minute untouched.
const REFRESH_EVERY: Duration = Duration::from_secs(20);
/// How long the direct way gets before both go through the relay. Under the brief's
/// 3 s, so that with the relay's answer a player is in within its 3 s target
/// [duel choice, KHC-018].
const PUNCH_WAIT: Duration = Duration::from_millis(2500);
/// Something is sent at least this often so the other side knows we are here.
const KEEPALIVE: Duration = Duration::from_millis(200);
/// A STUN server is asked this often, for this long, before the code is made
/// with the answers there are; a waiting host asks again now and then, which
/// keeps its router's way in open.
const STUN_EVERY: Duration = Duration::from_millis(300);
const STUN_WAIT: Duration = Duration::from_secs(3);
const STUN_REFRESH: Duration = Duration::from_secs(15);
/// The router's answer is waited for this long from the start, and the names
/// of the STUN servers twice that.
const MAP_WAIT: Duration = Duration::from_secs(6);
/// A joiner by direct code gives up after this, with a reason for a person:
/// long enough for the host to type the joiner's reply code.
const DIRECT_JOIN_WAIT: Duration = Duration::from_secs(60);
/// A joiner not let in by then shows its reply code (KHC-053).
const REPLY_AFTER: Duration = Duration::from_secs(4);
/// A host knocks toward a reply code's address this long.
const KNOCK_FOR: Duration = Duration::from_secs(30);
/// The largest datagram read.
const DATAGRAM: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Join,
}

#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// The name the other player sees.
    pub name: String,
    /// The executable's identity; a peer with another one is refused.
    pub build: Build,
    /// The same for a person: the version and short commit.
    pub version: String,
    /// What this player can show; a joiner's goes with its Hello as it stands then.
    pub caps: Capabilities,
    /// How long a silent peer (or a host that never answers) is waited for.
    pub timeout: Duration,
    /// [KHC-044] This player's id, sent with the Hello or the Welcome.
    pub player: [u8; wire::PLAYER_ID],
}

/// The other player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub name: String,
    pub addr: SocketAddr,
    pub version: String,
    pub caps: Capabilities,
    /// Their id as they sent it (unverified).
    pub player: [u8; wire::PLAYER_ID],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The other player is in (host: a joiner was welcomed; joiner: the
    /// host welcomed us).
    Connected(Peer),
    /// The host refused us, with its reason.
    Rejected(String),
    /// The other player left, timed out, or never answered.
    Disconnected(String),
    /// Host: the joiner stopped answering and their seat is being held.
    SeatHeld,
    /// Joiner: the host stopped answering and it is knocking again.
    Reconnecting,
    /// The seat is taken again, under a new nonce: renet starts over, so what was
    /// on its way is gone.
    Resumed(Peer),
    /// Host: the relay gave this session its room code.
    RoomCode(String),
    /// Host: the relay has not answered: the LAN can still find this host.
    RelaySilent,
    /// Host: a direct code is uncertain to work, and why (after its RoomCode).
    RoomNote(String),
    /// Host: no direct code can be made, and why; the LAN can still find this host.
    NoRoomCode(String),
    /// Joiner by direct code: not let in yet; the host who types this code in
    /// knocks back, which gets through a router that filters (KHC-053).
    ReplyCode(String),
}

/// Which way the session's packets go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Route {
    #[default]
    Direct,
    Relay,
}

impl Route {
    pub fn label(self) -> &'static str {
        match self {
            Route::Direct => "Direct",
            Route::Relay => "Relay",
        }
    }
}

/// A session found through the relay.
struct Rendezvous {
    server: SocketAddr,
    /// The room: a joiner's from the start, a host's once the relay gave it.
    code: Option<[u8; CODE_LENGTH]>,
    /// What this side asks the relay under, and the key it was given.
    ask_nonce: u64,
    key: Option<u64>,
    /// The other player's public address, and when it was learned.
    peer: Option<SocketAddr>,
    met_at: Option<Duration>,
    last_ask: Option<Duration>,
    last_punch: Option<Duration>,
    /// Everything goes through the relay now.
    relayed: bool,
    /// [Test] `--relay-only`: no direct way is tried.
    relay_only: bool,
    silent_told: bool,
    first_ask: Option<Duration>,
}

impl Rendezvous {
    fn new(server: SocketAddr, code: Option<[u8; CODE_LENGTH]>) -> Self {
        Rendezvous {
            server,
            code,
            ask_nonce: random_bits() | 1,
            key: None,
            peer: None,
            met_at: None,
            last_ask: None,
            last_punch: None,
            relayed: false,
            relay_only: false,
            silent_told: false,
            first_ask: None,
        }
    }
}

/// What a direct-code host asks the world (KHC-051).
#[derive(Clone, Debug)]
pub struct DirectConfig {
    /// STUN servers, `NAME:PORT`.
    pub stun: Vec<String>,
    /// Ask the router to open the port (UPnP, NAT-PMP).
    pub map_ports: bool,
}

impl Default for DirectConfig {
    fn default() -> Self {
        DirectConfig {
            stun: direct::STUN_SERVERS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            map_ports: true,
        }
    }
}

/// One STUN server's part.
struct StunAsk {
    server: SocketAddrV4,
    transaction: [u8; 12],
    last: Option<Duration>,
    seen: Option<SocketAddrV4>,
}

/// A direct code's side: a host's way to its code, or a joiner's to its reply
/// code.
struct Direct {
    host: bool,
    /// In a lock only so the session can be shared between threads (a game resource).
    names: Option<Mutex<Receiver<Vec<SocketAddrV4>>>>,
    asks: Vec<StunAsk>,
    resolved_at: Option<Duration>,
    /// Holds the router's mapping while the session lives.
    mapper: Option<Mapper>,
    mapping: Option<Result<Mapping, String>>,
    local: Option<Ipv4Addr>,
    started: Option<Duration>,
    told: bool,
    /// Host: reply codes' addresses, knocked toward until then.
    knocks: Vec<(SocketAddr, Duration)>,
    last_knock: Option<Duration>,
}

impl Direct {
    fn new(host: bool, world: DirectConfig, port: u16) -> Direct {
        Direct {
            host,
            names: Some(Mutex::new(direct::resolve(world.stun))),
            asks: Vec::new(),
            resolved_at: None,
            mapper: (host && world.map_ports).then(|| Mapper::start(port)),
            mapping: None,
            local: crate::portmap::local_ip(),
            started: None,
            told: false,
            knocks: Vec::new(),
            last_knock: None,
        }
    }
}

/// Link statistics for the overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub rtt_ms: f64,
    /// 0..1, renet's estimate from unacked packets.
    pub packet_loss: f64,
    pub sent_bytes_per_sec: f64,
    pub received_bytes_per_sec: f64,
    /// Datagrams the link simulator dropped.
    pub simulated_drops: u64,
    /// Milliseconds since the peer was last heard.
    pub silent_ms: f64,
}

enum Side {
    Host {
        server: Box<RenetServer>,
        peer: Option<(Peer, u64)>,
        /// A dropped joiner's seat, until this time.
        held: Option<(Peer, Duration)>,
    },
    Join {
        client: Box<RenetClient>,
        host: SocketAddr,
        welcomed: Option<Peer>,
        last_hello: Option<Duration>,
        /// Knocking again for its seat, until this time.
        resuming: Option<Duration>,
    },
    /// A replay's: messages come from `feed`, by channel.
    Playback {
        peer: Peer,
        queues: [VecDeque<Vec<u8>>; 2],
    },
}

/// What a dying game needs to tell the other player: its own handle on the
/// socket, so nothing of the session's has to be reached in a panic.
pub struct StopNotice {
    socket: UdpSocket,
    to: SocketAddr,
    nonce: u64,
    /// Through the relay: its address, the key and which side this is.
    relay: Option<(SocketAddr, u64, bool)>,
}

impl StopNotice {
    /// Sends it a few times, past a lossy link and the simulator; takes 60 ms
    /// of the 100 ms a stopping game allows [duel choice, KHC-028: a datagram
    /// and not the reliable channel, because renet's state is the panicking
    /// thread's to hold].
    pub fn send(&self) {
        let bytes = Datagram::Stopped { nonce: self.nonce }.encode();
        let (bytes, to) = match self.relay {
            Some((server, key, from_joiner)) => (
                Datagram::Relayed {
                    key,
                    from_joiner,
                    inner: bytes,
                }
                .encode(),
                server,
            ),
            None => (bytes, self.to),
        };
        for pause in [30, 30, 0] {
            let _ = self.socket.send_to(&bytes, to);
            std::thread::sleep(Duration::from_millis(pause));
        }
    }
}

pub struct Session {
    /// None in a playback session.
    socket: Option<UdpSocket>,
    config: SessionConfig,
    side: Side,
    nonce: u64,
    link: Link,
    start: Instant,
    last_update: Duration,
    last_heard: Duration,
    /// When a joiner's first Hello went (its wait for the host runs from it).
    first_hello: Option<Duration>,
    last_sent: Duration,
    events: VecDeque<Event>,
    closed: bool,
    /// A copy of what the joiner's path gets, when kept for a replay: the
    /// host's sends, the joiner's receives.
    tap: Option<Vec<(u8, Vec<u8>)>>,
    /// The bytes of the messages sent and received, by their first byte (a message's
    /// kind, to the game), for the network log.
    tally: [u64; 32],
    /// How long from the last word a dropped joiner's seat is held; None: not now.
    seat_hold: Option<Duration>,
    /// What brings the joiner back: the host's to give (0: none yet).
    token: u64,
    /// [Test] Nothing goes in or out until this time.
    cut_until: Option<Duration>,
    /// Found through the relay.
    rendezvous: Option<Rendezvous>,
    /// A host's direct code being made.
    direct: Option<Direct>,
    /// A joiner gives up after this long without a Welcome, for this reason.
    give_up: Option<(Duration, String)>,
}

fn channels() -> Vec<ChannelConfig> {
    vec![
        ChannelConfig {
            channel_id: UNRELIABLE,
            max_memory_usage_bytes: 1 << 20,
            send_type: SendType::Unreliable,
        },
        ChannelConfig {
            channel_id: RELIABLE,
            max_memory_usage_bytes: 1 << 20,
            send_type: SendType::ReliableOrdered {
                resend_time: Duration::from_millis(100),
            },
        },
    ]
}

fn connection() -> ConnectionConfig {
    ConnectionConfig {
        available_bytes_per_tick: 60_000,
        server_channels_config: channels(),
        client_channels_config: channels(),
    }
}

/// 64 random bits: the standard library's hasher keys come from the system's
/// source of randomness, and each `RandomState` has new ones.
/// Where LAN discovery is answered from: a private, link-local or loopback address.
fn from_lan(from: SocketAddr) -> bool {
    match from.ip() {
        std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_loopback(),
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.segments()[0] & 0xffc0 == 0xfe80
                || ip.segments()[0] & 0xfe00 == 0xfc00
        }
    }
}

pub(crate) fn random_bits() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// A session nonce, so a restarted joiner's stale datagrams are not taken for the
/// new session's. Odd, so it is never 0.
fn fresh_nonce() -> u64 {
    random_bits() | 1
}

impl Session {
    /// Hosts on `port` of `ip` (every local address for a LAN game, the
    /// loopback one for a test on one PC).
    pub fn host(ip: Ipv4Addr, port: u16, config: SessionConfig) -> io::Result<Session> {
        let socket = UdpSocket::bind(SocketAddrV4::new(ip, port))?;
        Self::with_socket(
            socket,
            config,
            Side::Host {
                server: Box::new(RenetServer::new(connection())),
                peer: None,
                held: None,
            },
        )
    }

    /// Joins the host at `host` (a port of its own is picked).
    pub fn join(host: SocketAddr, config: SessionConfig) -> io::Result<Session> {
        // A host on this PC is reached over loopback only.
        let ip = if host.ip().is_loopback() {
            Ipv4Addr::LOCALHOST
        } else {
            Ipv4Addr::UNSPECIFIED
        };
        let socket = UdpSocket::bind(SocketAddrV4::new(ip, 0))?;
        let mut client = RenetClient::new(connection());
        client.set_connecting();
        Self::with_socket(
            socket,
            config,
            Side::Join {
                client: Box::new(client),
                host,
                welcomed: None,
                last_hello: None,
                resuming: None,
            },
        )
    }

    /// Hosts as `host` does, and also takes a room at the relay `server`.
    pub fn host_room(
        ip: Ipv4Addr,
        port: u16,
        server: SocketAddr,
        config: SessionConfig,
    ) -> io::Result<Session> {
        let mut session = Self::host(ip, port, config)?;
        session.rendezvous = Some(Rendezvous::new(server, None));
        Ok(session)
    }

    /// Joins whoever has room `code` at the relay `server`.
    pub fn join_room(
        server: SocketAddr,
        code: [u8; CODE_LENGTH],
        config: SessionConfig,
    ) -> io::Result<Session> {
        let mut session = Self::join(server, config)?;
        session.rendezvous = Some(Rendezvous::new(server, Some(code)));
        Ok(session)
    }

    /// Hosts as `host` does, and makes a direct room code: this host's public
    /// address, which needs no server of ours (KHC-051).
    pub fn host_direct(
        ip: Ipv4Addr,
        port: u16,
        world: DirectConfig,
        config: SessionConfig,
    ) -> io::Result<Session> {
        let mut session = Self::host(ip, port, config)?;
        let port = session.local_addr()?.port();
        session.direct = Some(Direct::new(true, world, port));
        Ok(session)
    }

    /// Joins the host whose direct code gave `host`.
    /// It also asks STUN for its own address, for its reply code.
    pub fn join_direct(
        host: SocketAddrV4,
        world: DirectConfig,
        config: SessionConfig,
    ) -> io::Result<Session> {
        let mut session = Self::join(SocketAddr::V4(host), config)?;
        let port = session.local_addr()?.port();
        session.direct = Some(Direct::new(false, world, port));
        session.give_up = Some((
            DIRECT_JOIN_WAIT,
            "no answer from the room code's host: check the code and that they are waiting; \
             they can type your reply code, turn on UPnP, or you can host"
                .into(),
        ));
        Ok(session)
    }

    /// Host: knocks toward the address in a joiner's reply code for a while, so
    /// that joiner's knocks get through this router (KHC-053).
    pub fn knock_toward(&mut self, joiner: SocketAddrV4) {
        let until = self.now() + KNOCK_FOR;
        if let Some(d) = self.direct.as_mut().filter(|d| d.host) {
            d.knocks.retain(|k| k.0 != SocketAddr::V4(joiner));
            d.knocks.push((SocketAddr::V4(joiner), until));
            d.last_knock = None;
        }
    }

    /// One update of a direct host's STUN asks and router answer, and its code
    /// once they are in.
    fn direct_tick(&mut self, now: Duration) {
        let waiting = self.peer().is_none();
        let Some(d) = &mut self.direct else { return };
        let started = *d.started.get_or_insert(now);
        if let Some(names) = &d.names
            && let Ok(servers) = names
                .lock()
                .map_err(|_| ())
                .and_then(|r| r.try_recv().map_err(|_| ()))
        {
            d.names = None;
            d.resolved_at = Some(now);
            d.asks = servers
                .into_iter()
                .map(|server| {
                    let mut transaction = [0u8; 12];
                    transaction[..8].copy_from_slice(&random_bits().to_le_bytes());
                    transaction[8..].copy_from_slice(&random_bits().to_le_bytes()[..4]);
                    StunAsk {
                        server,
                        transaction,
                        last: None,
                        seen: None,
                    }
                })
                .collect();
        }
        if d.mapping.is_none()
            && let Some(m) = &d.mapper
        {
            d.mapping = m.poll();
        }
        let asking = d
            .resolved_at
            .is_some_and(|t| now.saturating_sub(t) < STUN_WAIT);
        let mut out = Vec::new();
        for (k, ask) in d.asks.iter_mut().enumerate() {
            let every = if ask.seen.is_none() && asking {
                STUN_EVERY
            } else if k == 0 && waiting && d.told && d.host {
                STUN_REFRESH
            } else {
                continue;
            };
            if ask.last.is_none_or(|t| now.saturating_sub(t) >= every) {
                ask.last = Some(now);
                out.push((
                    SocketAddr::V4(ask.server),
                    direct::stun_request(ask.transaction).to_vec(),
                ));
            }
        }
        if waiting
            && d.last_knock
                .is_none_or(|t| now.saturating_sub(t) >= PUNCH_EVERY)
        {
            d.knocks.retain(|k| now < k.1);
            if !d.knocks.is_empty() {
                d.last_knock = Some(now);
            }
            for (to, _) in &d.knocks {
                out.push((*to, Datagram::Punch { nonce: self.nonce }.encode()));
            }
        }
        let answered = !d.asks.is_empty() && d.asks.iter().all(|a| a.seen.is_some());
        let stun_done = (d.resolved_at.is_some() && (answered || !asking))
            || now.saturating_sub(started) >= MAP_WAIT * 2;
        let map_done =
            d.mapper.is_none() || d.mapping.is_some() || now.saturating_sub(started) >= MAP_WAIT;
        if !d.host && !d.told && stun_done && now.saturating_sub(started) >= REPLY_AFTER {
            d.told = true;
            let seen: Vec<SocketAddrV4> = d.asks.iter().filter_map(|a| a.seen).collect();
            // A router that changes port per destination has no reply code.
            if waiting && let Ok((addr, _)) = direct::choose(&seen, None, d.local) {
                self.events
                    .push_back(Event::ReplyCode(direct::encode_code(addr)));
            }
        }
        if d.host && !d.told && stun_done && map_done {
            d.told = true;
            let seen: Vec<SocketAddrV4> = d.asks.iter().filter_map(|a| a.seen).collect();
            let mapping = d.mapping.as_ref().and_then(|m| m.as_ref().ok());
            match direct::choose(&seen, mapping, d.local) {
                Ok((addr, note)) => {
                    self.events
                        .push_back(Event::RoomCode(direct::encode_code(addr)));
                    if let Some(note) = note {
                        self.events.push_back(Event::RoomNote(note));
                    }
                }
                Err(why) => self.events.push_back(Event::NoRoomCode(why)),
            }
        }
        for (to, bytes) in out {
            self.send_plain(now, to, bytes);
        }
    }

    /// A STUN server's answer to a direct host.
    fn handle_stun(&mut self, from: SocketAddr, bytes: &[u8]) {
        let Some(d) = &mut self.direct else { return };
        let Some(transaction) = direct::stun_transaction(bytes) else {
            return;
        };
        if let Some(ask) = d
            .asks
            .iter_mut()
            .find(|a| a.transaction == transaction && SocketAddr::V4(a.server) == from)
            && let Some(seen) = direct::stun_mapped(bytes)
        {
            ask.seen.get_or_insert(seen);
        }
    }

    /// The router's answer for a direct host, once in.
    pub fn port_mapping(&self) -> Option<&Result<Mapping, String>> {
        self.direct.as_ref()?.mapping.as_ref()
    }

    /// [Test] Everything through the relay, with no direct try.
    pub fn set_relay_only(&mut self, on: bool) {
        if let Some(r) = &mut self.rendezvous {
            r.relay_only = on;
        }
    }

    /// Which way the session's packets go.
    pub fn route(&self) -> Route {
        if self.rendezvous.as_ref().is_some_and(|r| r.relayed) {
            Route::Relay
        } else {
            Route::Direct
        }
    }

    /// The room this session has at the relay.
    pub fn room_code(&self) -> Option<String> {
        let code = self.rendezvous.as_ref()?.code?;
        String::from_utf8(code.to_vec()).ok()
    }

    /// A session with no socket, already connected to `peer_name`: a replay
    /// feeds it what the joiner's path would have received.
    pub fn playback(name: &str, peer_name: &str) -> Session {
        let peer = Peer {
            name: peer_name.to_owned(),
            addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            version: String::new(),
            caps: Capabilities::default(),
            player: [0; wire::PLAYER_ID],
        };
        Session {
            socket: None,
            config: SessionConfig {
                name: name.to_owned(),
                build: [0; 32],
                version: String::new(),
                caps: Capabilities::default(),
                timeout: Duration::MAX,
                player: [0; wire::PLAYER_ID],
            },
            side: Side::Playback {
                peer: peer.clone(),
                queues: [VecDeque::new(), VecDeque::new()],
            },
            nonce: 0,
            link: Link::new(0),
            start: Instant::now(),
            last_update: Duration::ZERO,
            last_heard: Duration::ZERO,
            first_hello: None,
            last_sent: Duration::ZERO,
            events: VecDeque::from([Event::Connected(peer)]),
            closed: false,
            tap: None,
            tally: [0; 32],
            seat_hold: None,
            token: 0,
            cut_until: None,
            rendezvous: None,
            direct: None,
            give_up: None,
        }
    }

    /// A playback session's next message on `channel`; other sessions ignore
    /// it.
    pub fn feed(&mut self, channel: u8, message: Vec<u8>) {
        if let Side::Playback { queues, .. } = &mut self.side
            && let Some(queue) = queues.get_mut(usize::from(channel))
        {
            queue.push_back(message);
        }
    }

    fn with_socket(socket: UdpSocket, config: SessionConfig, side: Side) -> io::Result<Session> {
        socket.set_nonblocking(true)?;
        let nonce = fresh_nonce();
        Ok(Session {
            socket: Some(socket),
            config,
            side,
            nonce,
            link: Link::new(nonce),
            start: Instant::now(),
            last_update: Duration::ZERO,
            last_heard: Duration::ZERO,
            first_hello: None,
            last_sent: Duration::ZERO,
            events: VecDeque::new(),
            closed: false,
            tap: None,
            tally: [0; 32],
            seat_hold: None,
            token: 0,
            cut_until: None,
            rendezvous: None,
            direct: None,
            give_up: None,
        })
    }

    /// A replay's session: what it receives comes from a file.
    pub fn is_playback(&self) -> bool {
        matches!(self.side, Side::Playback { .. })
    }

    pub fn role(&self) -> Role {
        match self.side {
            Side::Host { .. } => Role::Host,
            Side::Join { .. } | Side::Playback { .. } => Role::Join,
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        match &self.socket {
            Some(socket) => socket.local_addr(),
            None => Err(io::ErrorKind::NotConnected.into()),
        }
    }

    /// Starts or stops keeping a copy of what the joiner's path gets.
    pub fn tally(&self) -> &[u64; 32] {
        &self.tally
    }

    fn count(&mut self, message: &[u8]) {
        if let Some(first) = message.first() {
            self.tally[usize::from(*first) % 32] += message.len() as u64;
        }
    }

    pub fn set_tap(&mut self, on: bool) {
        self.tap = on.then(Vec::new);
    }

    /// The copy kept since the last call: (channel, message).
    pub fn take_tapped(&mut self) -> Vec<(u8, Vec<u8>)> {
        self.tap.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// What a stopping game needs to tell the other player, once there is
    /// one.
    pub fn stop_notice(&self) -> Option<StopNotice> {
        let (to, nonce) = match &self.side {
            Side::Host {
                peer: Some((p, nonce)),
                ..
            } => (p.addr, *nonce),
            Side::Join {
                host,
                welcomed: Some(_),
                ..
            } => (*host, self.nonce),
            _ => return None,
        };
        let socket = self.socket.as_ref()?.try_clone().ok()?;
        Some(StopNotice {
            socket,
            to,
            nonce,
            relay: self.relay_path(),
        })
    }

    /// The other player once the session runs.
    pub fn peer(&self) -> Option<&Peer> {
        match &self.side {
            Side::Host { peer, .. } => peer.as_ref().map(|p| &p.0),
            Side::Join { welcomed, .. } => welcomed.as_ref(),
            Side::Playback { peer, .. } => Some(peer),
        }
    }

    pub fn is_connected(&self) -> bool {
        !self.closed && self.peer().is_some()
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn conditions(&self) -> Conditions {
        self.link.conditions
    }

    /// How long silence means the other player is gone, from now on.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.config.timeout = timeout;
    }

    /// While a match runs: a joiner dropped keeps their seat for this long from
    /// the last word, and knocks again as long. None: a drop ends the session.
    pub fn set_seat_hold(&mut self, hold: Option<Duration>) {
        self.seat_hold = hold;
    }

    /// The seat is empty, or being knocked for.
    pub fn seat_waiting(&self) -> bool {
        match &self.side {
            Side::Host { held, .. } => held.is_some(),
            Side::Join { resuming, .. } => resuming.is_some(),
            Side::Playback { .. } => false,
        }
    }

    /// How long the seat has left, once it is waiting.
    pub fn seat_time_left(&self) -> Option<Duration> {
        let until = match &self.side {
            Side::Host { held, .. } => held.as_ref().map(|h| h.1),
            Side::Join { resuming, .. } => *resuming,
            Side::Playback { .. } => None,
        }?;
        Some(until.saturating_sub(self.now()))
    }

    /// [Test] Nothing goes in or out for this long, as if the cable were pulled.
    pub fn cut_for(&mut self, time: Duration) {
        self.cut_until = Some(self.now() + time);
    }

    fn cut(&self, now: Duration) -> bool {
        self.cut_until.is_some_and(|until| now < until)
    }

    /// What this player can show, from now on.
    pub fn set_caps(&mut self, caps: Capabilities) {
        self.config.caps = caps;
    }

    pub fn set_conditions(&mut self, conditions: Conditions) {
        self.link.conditions = conditions;
    }

    pub fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// The relay, the key and this side's part, while packets go that way.
    fn relay_path(&self) -> Option<(SocketAddr, u64, bool)> {
        let r = self.rendezvous.as_ref().filter(|r| r.relayed)?;
        Some((r.server, r.key?, matches!(self.side, Side::Join { .. })))
    }

    /// One update of what the relay is asked, and of the way to the other player.
    fn rendezvous_tick(&mut self, now: Duration) {
        let timeout = self.config.timeout;
        let waiting = self.peer().is_none();
        let host_side = matches!(self.side, Side::Host { .. });
        let Some(r) = &mut self.rendezvous else {
            return;
        };
        let mut out: Vec<(SocketAddr, Vec<u8>)> = Vec::new();
        let due =
            |last: Option<Duration>, every| last.is_none_or(|t| now.saturating_sub(t) >= every);
        let first = *r.first_ask.get_or_insert(now);
        if host_side {
            let every = if r.code.is_some() {
                REFRESH_EVERY
            } else {
                ASK_EVERY
            };
            if due(r.last_ask, every) {
                r.last_ask = Some(now);
                out.push((
                    r.server,
                    Datagram::Register {
                        code: r.code.unwrap_or([0; CODE_LENGTH]),
                        nonce: r.ask_nonce,
                    }
                    .encode(),
                ));
            }
            if r.code.is_none() && !r.silent_told && now.saturating_sub(first) > timeout {
                r.silent_told = true;
                self.events.push_back(Event::RelaySilent);
            }
        } else if r.peer.is_none()
            && let Some(code) = r.code
            && due(r.last_ask, ASK_EVERY)
        {
            r.last_ask = Some(now);
            out.push((
                r.server,
                Datagram::Lookup {
                    code,
                    nonce: r.ask_nonce,
                }
                .encode(),
            ));
        }
        // Both knock on the other's public address until a player is in; a joiner
        // gives the direct way a moment and then goes through the relay.
        if waiting && !r.relayed {
            if let Some(met) = r.met_at
                && !host_side
                && (r.relay_only || now.saturating_sub(met) >= PUNCH_WAIT)
            {
                r.relayed = true;
            } else if let Some(peer) = r.peer
                && !r.relay_only
                && due(r.last_punch, PUNCH_EVERY)
            {
                r.last_punch = Some(now);
                out.push((peer, Datagram::Punch { nonce: r.ask_nonce }.encode()));
            }
        }
        for (to, bytes) in out {
            self.send_plain(now, to, bytes);
        }
    }

    /// What comes from the relay itself: true when it was that.
    fn handle_relay(&mut self, now: Duration, from: SocketAddr, d: &Datagram) -> bool {
        let Some(r) = &mut self.rendezvous else {
            return false;
        };
        if from != r.server {
            return false;
        }
        match d {
            Datagram::Registered { nonce, code, key } if *nonce == r.ask_nonce => {
                r.key = Some(*key);
                if r.code != Some(*code) {
                    r.code = Some(*code);
                    self.events
                        .push_back(Event::RoomCode(String::from_utf8_lossy(code).into_owned()));
                }
            }
            Datagram::PeerAt { nonce, key, addr } if *nonce == r.ask_nonce => {
                r.key = Some(*key);
                let peer = SocketAddr::V4(*addr);
                if r.peer != Some(peer) {
                    r.peer = Some(peer);
                    r.met_at = Some(now);
                    r.last_punch = None;
                    r.relayed = false;
                }
                if let Side::Join { host, .. } = &mut self.side {
                    *host = peer;
                }
            }
            Datagram::NoRoom { nonce } if *nonce == r.ask_nonce => {
                self.closed = true;
                self.events
                    .push_back(Event::Rejected("no room has that code".into()));
            }
            _ => return false,
        }
        true
    }

    /// What reaches this socket by way of the relay: the session's, as if it had
    /// come from the other player, and from now on the answers go back that way.
    fn handle_relayed(&mut self, now: Duration, from: SocketAddr, key: u64, inner: &[u8]) {
        let Some(r) = &mut self.rendezvous else {
            return;
        };
        if from != r.server || r.key != Some(key) {
            return;
        }
        r.relayed = true;
        let peer = r.peer.unwrap_or(from);
        if let Some(d) = Datagram::decode(inner) {
            self.handle(now, peer, d);
        }
    }

    /// To the relay, or a punch: never through it.
    fn send_plain(&mut self, now: Duration, to: SocketAddr, bytes: Vec<u8>) {
        if self.cut(now) {
            return;
        }
        if let Some((to, bytes)) = self.link.send(now, to, bytes)
            && let Some(socket) = &self.socket
        {
            let _ = socket.send_to(&bytes, to);
        }
    }

    /// Reads what arrived, advances renet, handles timeouts and sends what
    /// is due. Call once per simulation tick.
    pub fn update(&mut self) {
        if self.closed || self.socket.is_none() {
            return;
        }
        let now = self.now();
        self.receive_all(now);
        self.rendezvous_tick(now);
        self.direct_tick(now);
        let delta = now.saturating_sub(self.last_update);
        self.last_update = now;
        let timeout = self.config.timeout;
        let silent = now.saturating_sub(self.last_heard);
        let first_hello = &mut self.first_hello;
        let give_up = self.give_up.clone();
        let hold_until = self.seat_hold.map(|hold| self.last_heard + hold);
        match &mut self.side {
            Side::Host { server, peer, held } => {
                server.update(delta);
                if peer.is_some() && silent > timeout {
                    let gone = peer.take();
                    server.remove_connection(PEER);
                    match (gone, hold_until) {
                        (Some((p, _)), Some(until)) if self.token != 0 => {
                            *held = Some((p, until));
                            self.events.push_back(Event::SeatHeld);
                        }
                        _ => self
                            .events
                            .push_back(Event::Disconnected("the other player timed out".into())),
                    }
                }
                if held.as_ref().is_some_and(|h| now >= h.1) {
                    *held = None;
                    self.token = 0;
                    self.events
                        .push_back(Event::Disconnected("the other player timed out".into()));
                }
            }
            Side::Join {
                client,
                host,
                welcomed,
                last_hello,
                resuming,
            } => {
                client.update(delta);
                if welcomed.is_none() {
                    // The wait starts with the first Hello: a joiner's own
                    // load, before its first update, does not count.
                    let waited = now.saturating_sub(*first_hello.get_or_insert(now));
                    if let Some(until) = *resuming {
                        if now >= until {
                            self.closed = true;
                            self.events
                                .push_back(Event::Disconnected("the host timed out".into()));
                            return;
                        }
                    } else if let Some((wait, why)) = &give_up {
                        // A direct code's wait is its own, not the timeout's.
                        if waited > *wait {
                            self.closed = true;
                            self.events.push_back(Event::Disconnected(why.clone()));
                            return;
                        }
                    } else if waited > timeout {
                        self.closed = true;
                        self.events.push_back(Event::Disconnected(format!(
                            "no answer from {host}: check the address, that the host is waiting, and the firewall"
                        )));
                        return;
                    }
                    // Through the relay, a Hello waits until the room has a host.
                    let met = self.rendezvous.as_ref().is_none_or(|r| r.peer.is_some());
                    if met && last_hello.is_none_or(|t| now.saturating_sub(t) >= HELLO_EVERY) {
                        *last_hello = Some(now);
                        let hello = Datagram::Hello {
                            protocol: PROTOCOL,
                            build: self.config.build,
                            nonce: self.nonce,
                            version: self.config.version.clone(),
                            name: self.config.name.clone(),
                            caps: self.config.caps,
                            resume: if resuming.is_some() { self.token } else { 0 },
                            player: self.config.player,
                        };
                        let to = *host;
                        self.transmit(now, to, hello.encode());
                    }
                } else if silent > timeout {
                    match hold_until {
                        Some(until) if self.token != 0 => {
                            // A new session under the old seat: nothing of the
                            // old one's packets is good for it.
                            *welcomed = None;
                            **client = RenetClient::new(connection());
                            client.set_connecting();
                            *last_hello = None;
                            *resuming = Some(until);
                            self.nonce = fresh_nonce();
                            // The old way may be gone with the old address.
                            if let Some(r) = &mut self.rendezvous {
                                r.met_at = Some(now);
                                r.relayed = false;
                            }
                            self.events.push_back(Event::Reconnecting);
                        }
                        _ => {
                            self.closed = true;
                            client.disconnect_due_to_transport();
                            self.events
                                .push_back(Event::Disconnected("the host timed out".into()));
                            return;
                        }
                    }
                }
            }
            Side::Playback { .. } => {}
        }
        self.flush(now);
    }

    fn receive_all(&mut self, now: Duration) {
        let mut buffer = [0u8; DATAGRAM];
        let cut = self.cut(now);
        loop {
            let Some(socket) = &self.socket else { return };
            match socket.recv_from(&mut buffer) {
                Ok((n, from)) => {
                    if !cut && direct::stun_transaction(&buffer[..n]).is_some() {
                        let bytes = buffer[..n].to_vec();
                        self.handle_stun(from, &bytes);
                    } else if !cut && let Some(d) = Datagram::decode(&buffer[..n]) {
                        self.handle(now, from, d);
                    }
                    // A Bye or Reject ended the session: the rest is stale.
                    if self.closed {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                // Windows reports an earlier datagram's ICMP "port
                // unreachable" on the next read; it is not this socket's end.
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionRefused
                    ) => {}
                Err(_) => break,
            }
        }
    }

    fn handle(&mut self, now: Duration, from: SocketAddr, d: Datagram) {
        if self.handle_relay(now, from, &d) {
            return;
        }
        if let Datagram::Relayed { key, inner, .. } = &d {
            self.handle_relayed(now, from, *key, inner);
            return;
        }
        let build = self.config.build;
        let name = self.config.name.clone();
        let version = self.config.version.clone();
        let caps = self.config.caps;
        let token = self.token;
        match (&mut self.side, d) {
            // Any protocol is answered: a list that shows a host of another
            // version beats one that hides it.
            // [KHC-062] Only from the LAN: a host with an open port would
            // otherwise answer a spoofed 7-byte Discover with a far larger Announce.
            (Side::Host { .. }, Datagram::Discover { .. }) if !from_lan(from) => {}
            (Side::Host { .. }, Datagram::Discover { .. }) => {
                let open = self.peer().is_none();
                let a = Datagram::Announce {
                    protocol: PROTOCOL,
                    build,
                    version,
                    name,
                    open,
                };
                self.transmit(now, from, a.encode());
            }
            (
                Side::Host { server, peer, held },
                Datagram::Hello {
                    protocol,
                    build: theirs,
                    nonce,
                    version: their_version,
                    name: their_name,
                    caps: their_caps,
                    resume,
                    player: their_player,
                },
            ) => {
                let refused = wire::refusal(
                    &wire::Identity {
                        protocol: PROTOCOL,
                        build: &build,
                        version: &version,
                        caps,
                    },
                    &wire::Identity {
                        protocol,
                        build: &theirs,
                        version: &their_version,
                        caps: their_caps,
                    },
                );
                let welcome = |token| Datagram::Welcome {
                    nonce,
                    name: name.clone(),
                    version: version.clone(),
                    caps,
                    token,
                    player: self.config.player,
                };
                let knocks = resume != 0 && resume == token;
                let them = Peer {
                    name: their_name,
                    addr: from,
                    version: their_version,
                    caps: their_caps,
                    player: their_player,
                };
                let reply = match peer {
                    // A repeated Hello of the session already welcomed.
                    Some((p, n)) if p.addr == from && *n == nonce => welcome(token),
                    // Back, at a new address or nonce, before or after this side
                    // noticed they had gone. Stale datagrams keep their old nonce,
                    // and the token is not guessed.
                    Some(_) if knocks => {
                        server.remove_connection(PEER);
                        server.add_connection(PEER);
                        *peer = Some((them.clone(), nonce));
                        self.last_heard = now;
                        self.events.push_back(Event::Resumed(them));
                        welcome(token)
                    }
                    Some(_) => Datagram::Reject {
                        nonce,
                        reason: "this match already has two players".into(),
                    },
                    None if knocks && held.is_some() => {
                        *held = None;
                        server.add_connection(PEER);
                        *peer = Some((them.clone(), nonce));
                        self.last_heard = now;
                        self.events.push_back(Event::Resumed(them));
                        welcome(token)
                    }
                    None if held.is_some() => Datagram::Reject {
                        nonce,
                        reason: "this match is waiting for its other player".into(),
                    },
                    None if resume != 0 => Datagram::Reject {
                        nonce,
                        reason: "that seat is no longer held".into(),
                    },
                    None if refused.is_some() => Datagram::Reject {
                        nonce,
                        reason: refused.unwrap_or_default(),
                    },
                    None => {
                        *peer = Some((them.clone(), nonce));
                        server.add_connection(PEER);
                        self.last_heard = now;
                        self.events.push_back(Event::Connected(them));
                        self.token = random_bits() | 1;
                        welcome(self.token)
                    }
                };
                self.transmit(now, from, reply.encode());
            }
            (Side::Host { server, peer, .. }, Datagram::Data { nonce, payload }) => {
                if let Some((p, n)) = peer
                    && p.addr == from
                    && *n == nonce
                {
                    self.last_heard = now;
                    if !payload.is_empty() {
                        let _ = server.process_packet_from(&payload, PEER);
                    }
                }
            }
            (Side::Host { server, peer, .. }, Datagram::Bye { nonce }) => {
                if let Some((p, n)) = peer
                    && p.addr == from
                    && *n == nonce
                {
                    *peer = None;
                    server.remove_connection(PEER);
                    self.events
                        .push_back(Event::Disconnected("the other player left".into()));
                }
            }
            (Side::Host { server, peer, .. }, Datagram::Stopped { nonce }) => {
                if let Some((p, n)) = peer
                    && p.addr == from
                    && *n == nonce
                {
                    *peer = None;
                    server.remove_connection(PEER);
                    self.events
                        .push_back(Event::Disconnected(STOPPED_REASON.into()));
                }
            }
            (
                Side::Join {
                    client,
                    host,
                    welcomed,
                    resuming,
                    ..
                },
                Datagram::Welcome {
                    nonce,
                    name,
                    version,
                    caps,
                    token,
                    player,
                },
            ) if from == *host && nonce == self.nonce && welcomed.is_none() => {
                let p = Peer {
                    name,
                    addr: from,
                    version,
                    caps,
                    player,
                };
                *welcomed = Some(p.clone());
                client.set_connected();
                self.last_heard = now;
                self.token = token;
                self.events.push_back(if resuming.take().is_some() {
                    Event::Resumed(p)
                } else {
                    Event::Connected(p)
                });
            }
            (Side::Join { host, .. }, Datagram::Reject { nonce, reason })
                if from == *host && nonce == self.nonce =>
            {
                self.closed = true;
                self.events.push_back(Event::Rejected(reason));
            }
            (
                Side::Join {
                    client,
                    host,
                    welcomed,
                    ..
                },
                Datagram::Data { nonce, payload },
            ) if from == *host && nonce == self.nonce && welcomed.is_some() => {
                self.last_heard = now;
                if !payload.is_empty() {
                    client.process_packet(&payload);
                }
            }
            (Side::Join { host, welcomed, .. }, Datagram::Bye { nonce })
                if from == *host && nonce == self.nonce && welcomed.is_some() =>
            {
                self.closed = true;
                self.events
                    .push_back(Event::Disconnected("the host left".into()));
            }
            (Side::Join { host, welcomed, .. }, Datagram::Stopped { nonce })
                if from == *host && nonce == self.nonce && welcomed.is_some() =>
            {
                self.closed = true;
                self.events
                    .push_back(Event::Disconnected(STOPPED_REASON.into()));
            }
            _ => {}
        }
    }

    /// renet's packets, then a keep-alive if nothing went out for a while,
    /// then what the link simulator releases now.
    fn flush(&mut self, now: Duration) {
        let mut out: Vec<(SocketAddr, Datagram)> = Vec::new();
        let nonce_join = self.nonce;
        match &mut self.side {
            Side::Host {
                server,
                peer: Some((p, nonce)),
                ..
            } => {
                for payload in server.get_packets_to_send(PEER).unwrap_or_default() {
                    out.push((
                        p.addr,
                        Datagram::Data {
                            nonce: *nonce,
                            payload,
                        },
                    ));
                }
                if out.is_empty() && now.saturating_sub(self.last_sent) >= KEEPALIVE {
                    out.push((
                        p.addr,
                        Datagram::Data {
                            nonce: *nonce,
                            payload: Vec::new(),
                        },
                    ));
                }
            }
            Side::Join {
                client,
                host,
                welcomed: Some(_),
                ..
            } => {
                for payload in client.get_packets_to_send() {
                    out.push((
                        *host,
                        Datagram::Data {
                            nonce: nonce_join,
                            payload,
                        },
                    ));
                }
                if out.is_empty() && now.saturating_sub(self.last_sent) >= KEEPALIVE {
                    out.push((
                        *host,
                        Datagram::Data {
                            nonce: nonce_join,
                            payload: Vec::new(),
                        },
                    ));
                }
            }
            _ => {}
        }
        for (to, d) in out {
            self.transmit(now, to, d.encode());
        }
        let cut = self.cut(now);
        for (to, bytes) in self.link.due(now) {
            if !cut && let Some(socket) = &self.socket {
                let _ = socket.send_to(&bytes, to);
            }
        }
    }

    fn transmit(&mut self, now: Duration, to: SocketAddr, bytes: Vec<u8>) {
        self.last_sent = now;
        let (to, bytes) = self.by_way(to, bytes);
        self.send_plain(now, to, bytes);
    }

    /// A datagram of the session, as it goes on the route in use.
    fn by_way(&self, to: SocketAddr, bytes: Vec<u8>) -> (SocketAddr, Vec<u8>) {
        match self.relay_path() {
            Some((server, key, from_joiner)) => (
                server,
                Datagram::Relayed {
                    key,
                    from_joiner,
                    inner: bytes,
                }
                .encode(),
            ),
            None => (to, bytes),
        }
    }

    /// Queues a message for the other player on `channel`.
    pub fn send(&mut self, channel: u8, message: Vec<u8>) {
        if !self.is_connected() {
            return;
        }
        self.count(&message);
        match &mut self.side {
            Side::Host { server, .. } => {
                if let Some(tap) = &mut self.tap {
                    tap.push((channel, message.clone()));
                }
                server.send_message(PEER, channel, message);
            }
            Side::Join { client, .. } => client.send_message(channel, message),
            // Nobody to send to.
            Side::Playback { .. } => {}
        }
    }

    /// The next message the other player sent on `channel`.
    pub fn receive(&mut self, channel: u8) -> Option<Vec<u8>> {
        if !self.is_connected() {
            return None;
        }
        let message = match &mut self.side {
            Side::Host { server, .. } => server.receive_message(PEER, channel).map(|b| b.to_vec()),
            Side::Join { client, .. } => {
                let message = client.receive_message(channel).map(|b| b.to_vec());
                if let (Some(tap), Some(m)) = (&mut self.tap, &message) {
                    tap.push((channel, m.clone()));
                }
                message
            }
            Side::Playback { queues, .. } => queues
                .get_mut(usize::from(channel))
                .and_then(VecDeque::pop_front),
        };
        if let Some(m) = &message {
            self.count(m);
        }
        message
    }

    pub fn stats(&self) -> Stats {
        let info = match &self.side {
            Side::Host { server, peer, .. } if peer.is_some() => server.network_info(PEER).ok(),
            Side::Join {
                client,
                welcomed: Some(_),
                ..
            } => Some(client.network_info()),
            _ => None,
        };
        let silent = self.now().saturating_sub(self.last_heard);
        Stats {
            rtt_ms: info.as_ref().map_or(0., |i| i.rtt * 1000.),
            packet_loss: info.as_ref().map_or(0., |i| i.packet_loss),
            sent_bytes_per_sec: info.as_ref().map_or(0., |i| i.bytes_sent_per_second),
            received_bytes_per_sec: info.as_ref().map_or(0., |i| i.bytes_received_per_second),
            simulated_drops: self.link.dropped,
            silent_ms: silent.as_secs_f64() * 1000.,
        }
    }

    /// Ends the session: Bye goes out (a few times, past a lossy link) and
    /// the socket closes when the session is dropped.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        let bye = match &self.side {
            Side::Host {
                peer: Some((p, nonce)),
                ..
            } => Some((p.addr, *nonce)),
            Side::Join {
                host,
                welcomed: Some(_),
                ..
            } => Some((*host, self.nonce)),
            _ => None,
        };
        if let Some((to, nonce)) = bye {
            let (to, bytes) = self.by_way(to, Datagram::Bye { nonce }.encode());
            // Past the simulator: the other side should hear it at once.
            for _ in 0..3 {
                if let Some(socket) = &self.socket {
                    let _ = socket.send_to(&bytes, to);
                }
            }
        }
        self.closed = true;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

/// A host found on the LAN.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub addr: SocketAddr,
    /// Same protocol and build as ours.
    pub compatible: bool,
    /// Its version, for a person to read.
    pub version: String,
    /// The host still waits for a player.
    pub open: bool,
    /// How long its answer to our last ask took (ms, smoothed).
    pub ping_ms: Option<u32>,
    last_seen: Duration,
}

/// LAN discovery for the join screen: Discover goes to the game port as a
/// broadcast (and to this PC, for two instances on one machine); hosts
/// answer with Announce.
pub struct Browser {
    socket: UdpSocket,
    port: u16,
    build: Build,
    start: Instant,
    last_probe: Option<Duration>,
    found: Vec<Found>,
}

/// How often the browser asks, and how long an unanswered host stays listed.
const PROBE_EVERY: Duration = Duration::from_millis(1000);
const FORGET_AFTER: Duration = Duration::from_millis(3500);

impl Browser {
    pub fn new(port: u16, build: Build) -> io::Result<Browser> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_nonblocking(true)?;
        socket.set_broadcast(true)?;
        Ok(Browser {
            socket,
            port,
            build,
            start: Instant::now(),
            last_probe: None,
            found: Vec::new(),
        })
    }

    /// Asks again when due, reads the answers and forgets silent hosts.
    pub fn update(&mut self) {
        let now = self.start.elapsed();
        if self
            .last_probe
            .is_none_or(|t| now.saturating_sub(t) >= PROBE_EVERY)
        {
            self.last_probe = Some(now);
            let probe = Datagram::Discover { protocol: PROTOCOL }.encode();
            for ip in [Ipv4Addr::BROADCAST, Ipv4Addr::LOCALHOST] {
                let _ = self
                    .socket
                    .send_to(&probe, SocketAddrV4::new(ip, self.port));
            }
        }
        let mut buffer = [0u8; DATAGRAM];
        loop {
            match self.socket.recv_from(&mut buffer) {
                Ok((n, from)) => {
                    if let Some(Datagram::Announce {
                        protocol,
                        build,
                        version,
                        name,
                        open,
                    }) = Datagram::decode(&buffer[..n])
                    {
                        // The answer's time since the last ask (asks go out a
                        // second apart, so a later answer is no measure).
                        let ping = self
                            .last_probe
                            .map(|t| now.saturating_sub(t))
                            .filter(|d| *d < PROBE_EVERY)
                            .map(|d| d.as_millis() as u32);
                        self.note(
                            now,
                            from,
                            name,
                            version,
                            protocol == PROTOCOL && build == self.build,
                            open,
                            ping,
                        );
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionRefused
                    ) => {}
                Err(_) => break,
            }
        }
        self.found
            .retain(|f| now.saturating_sub(f.last_seen) < FORGET_AFTER);
    }

    #[allow(clippy::too_many_arguments)]
    fn note(
        &mut self,
        now: Duration,
        addr: SocketAddr,
        name: String,
        version: String,
        compatible: bool,
        open: bool,
        ping: Option<u32>,
    ) {
        // One host answers both the broadcast and the loopback probe: keep
        // one row per name and port, preferring the LAN address.
        let mut row = Found {
            name,
            addr,
            compatible,
            version,
            open,
            ping_ms: ping,
            last_seen: now,
        };
        match self
            .found
            .iter_mut()
            .find(|f| f.addr == addr || (f.name == row.name && f.addr.port() == addr.port()))
        {
            Some(f) => {
                // A steadier number: three parts the last, one the new.
                row.ping_ms = match (f.ping_ms, ping) {
                    (Some(old), Some(new)) => Some((old * 3 + new).div_ceil(4)),
                    (old, new) => new.or(old),
                };
                let keep_lan = f.addr.ip().is_loopback() && !addr.ip().is_loopback();
                let addr = if keep_lan || !f.addr.ip().is_loopback() {
                    if keep_lan { addr } else { f.addr }
                } else {
                    addr
                };
                *f = Found { addr, ..row };
            }
            None => self.found.push(row),
        }
        self.found
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.addr.cmp(&b.addr)));
    }

    pub fn found(&self) -> &[Found] {
        &self.found
    }
}

/// A session lives in a game resource, which is shared between threads.
const _: fn() = || {
    fn shared<T: Send + Sync>() {}
    shared::<Session>();
};

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str, build: u8) -> SessionConfig {
        SessionConfig {
            name: name.into(),
            build: [build; 32],
            version: format!("0.1.0 commit{build}"),
            caps: Capabilities::default(),
            timeout: Duration::from_secs(2),
            player: [build; wire::PLAYER_ID],
        }
    }

    /// Steps both sessions until `done` or about two seconds pass.
    fn pump(
        a: &mut Session,
        b: &mut Session,
        mut done: impl FnMut(&mut Session, &mut Session) -> bool,
    ) -> bool {
        for _ in 0..400 {
            a.update();
            b.update();
            if done(a, b) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    #[test]
    fn join_exchange_and_leave_over_loopback() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), config("Joiner", 1)).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        assert_eq!(host.peer().map(|p| p.name.as_str()), Some("Joiner"));
        assert_eq!(join.peer().map(|p| p.name.as_str()), Some("Host"));
        assert!(matches!(host.next_event(), Some(Event::Connected(_))));
        assert!(matches!(join.next_event(), Some(Event::Connected(_))));

        join.send(RELIABLE, b"ready".to_vec());
        host.send(UNRELIABLE, b"state".to_vec());
        let mut got = (None, None);
        assert!(pump(&mut host, &mut join, |h, j| {
            if got.0.is_none() {
                got.0 = h.receive(RELIABLE);
            }
            if got.1.is_none() {
                got.1 = j.receive(UNRELIABLE);
            }
            got.0.is_some() && got.1.is_some()
        }));
        assert_eq!(got, (Some(b"ready".to_vec()), Some(b"state".to_vec())));

        join.close();
        assert!(pump(&mut host, &mut join, |h, _| !h.is_connected()));
        assert_eq!(
            host.next_event(),
            Some(Event::Disconnected("the other player left".into()))
        );
    }

    #[test]
    fn another_build_is_refused_with_a_reason() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), config("Joiner", 2)).unwrap();
        assert!(pump(&mut host, &mut join, |_, j| j.is_closed()));
        match join.next_event() {
            Some(Event::Rejected(reason)) => {
                assert!(reason.contains("different build"));
                assert!(
                    reason.contains("Host: 0.1.0 commit1 id 01010101"),
                    "{reason}"
                );
                assert!(
                    reason.contains("You: 0.1.0 commit2 id 02020202"),
                    "{reason}"
                );
            }
            other => panic!("expected a rejection, got {other:?}"),
        }
        assert!(!host.is_connected());
    }

    #[test]
    fn a_third_player_is_refused() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut first = Session::join(loopback(port), config("One", 1)).unwrap();
        assert!(pump(&mut host, &mut first, |h, j| h.is_connected()
            && j.is_connected()));
        let mut second = Session::join(loopback(port), config("Two", 1)).unwrap();
        assert!(pump(&mut host, &mut second, |_, j| j.is_closed()));
        assert!(
            matches!(second.next_event(), Some(Event::Rejected(r)) if r.contains("two players"))
        );
        first.update();
        assert!(first.is_connected());
    }

    #[test]
    fn a_leaving_host_is_heard_once() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut cfg = config("Joiner", 1);
        cfg.timeout = Duration::from_millis(300);
        let mut join = Session::join(loopback(port), cfg).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        drop(host);
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut ended = Vec::new();
        while Instant::now() < deadline {
            join.update();
            while let Some(e) = join.next_event() {
                if let Event::Disconnected(r) = e {
                    ended.push(r);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Dropping the host sends Bye (three times, past a lossy link); the
        // joiner hears it leave once.
        assert_eq!(ended, vec!["the host left".to_owned()]);
    }

    #[test]
    fn a_stopped_game_is_told_apart_from_a_timeout() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), config("Joiner", 1)).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        while host.next_event().is_some() {}
        while join.next_event().is_some() {}
        // The host's notice needs nothing of the session once it is made.
        let notice = host.stop_notice().expect("connected");
        notice.send();
        assert!(pump(&mut host, &mut join, |_, j| j.is_closed()));
        assert_eq!(
            join.next_event(),
            Some(Event::Disconnected(STOPPED_REASON.into()))
        );
        // And from the joiner's side.
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), config("Joiner", 1)).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        while host.next_event().is_some() {}
        join.stop_notice().expect("connected").send();
        assert!(pump(&mut host, &mut join, |h, _| !h.is_connected()));
        assert_eq!(
            host.next_event(),
            Some(Event::Disconnected(STOPPED_REASON.into()))
        );
    }

    #[test]
    fn a_tap_copies_what_the_joiner_gets() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), config("Joiner", 1)).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        host.set_tap(true);
        join.set_tap(true);
        host.send(RELIABLE, b"rules".to_vec());
        join.send(RELIABLE, b"not the joiner's path".to_vec());
        let mut got = None;
        assert!(pump(&mut host, &mut join, |_, j| {
            got = j.receive(RELIABLE);
            got.is_some()
        }));
        assert_eq!(host.take_tapped(), vec![(RELIABLE, b"rules".to_vec())]);
        assert_eq!(join.take_tapped(), vec![(RELIABLE, b"rules".to_vec())]);
        assert!(host.take_tapped().is_empty());
    }

    #[test]
    fn a_playback_session_gives_back_what_it_is_fed() {
        let mut s = Session::playback("Me", "Them");
        assert!(s.local_addr().is_err());
        assert!(s.stop_notice().is_none());
        assert!(matches!(s.next_event(), Some(Event::Connected(p)) if p.name == "Them"));
        assert!(s.is_connected());
        s.feed(RELIABLE, b"a".to_vec());
        s.feed(UNRELIABLE, b"b".to_vec());
        s.send(RELIABLE, b"ignored".to_vec());
        s.update();
        assert_eq!(s.receive(UNRELIABLE), Some(b"b".to_vec()));
        assert_eq!(s.receive(RELIABLE), Some(b"a".to_vec()));
        assert_eq!(s.receive(RELIABLE), None);
        s.close();
    }

    #[test]
    fn a_silent_host_times_out() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut cfg = config("Joiner", 1);
        cfg.timeout = Duration::from_millis(300);
        let mut join = Session::join(loopback(port), cfg).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        // The host stops answering without a Bye (it is no longer updated).
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !join.is_closed() {
            join.update();
            std::thread::sleep(Duration::from_millis(10));
        }
        let events: Vec<_> = std::iter::from_fn(|| join.next_event()).collect();
        assert_eq!(
            events.last(),
            Some(&Event::Disconnected("the host timed out".into()))
        );
        drop(host);
    }

    /// Steps both for up to `time`, until `done`.
    fn pump_for(
        a: &mut Session,
        b: &mut Session,
        time: Duration,
        mut done: impl FnMut(&mut Session, &mut Session) -> bool,
    ) -> bool {
        let deadline = Instant::now() + time;
        while Instant::now() < deadline {
            a.update();
            b.update();
            if done(a, b) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn pair(timeout: Duration) -> (Session, Session) {
        let mut h = config("Host", 1);
        h.timeout = timeout;
        let mut j = config("Joiner", 1);
        j.timeout = timeout;
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, h).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), j).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        while host.next_event().is_some() {}
        while join.next_event().is_some() {}
        (host, join)
    }

    #[test]
    fn a_ten_second_cut_comes_back_to_the_same_seat() {
        let (mut host, mut join) = pair(Duration::from_secs(1));
        let hold = Duration::from_secs(30);
        host.set_seat_hold(Some(hold));
        join.set_seat_hold(Some(hold));
        let old_nonce = join.nonce;
        // The cable is pulled at the joiner, both ways, for ten seconds.
        join.cut_for(Duration::from_secs(10));
        let from = Instant::now();
        let mut events = (Vec::new(), Vec::new());
        assert!(pump_for(
            &mut host,
            &mut join,
            Duration::from_secs(15),
            |h, j| {
                events.0.extend(std::iter::from_fn(|| h.next_event()));
                events.1.extend(std::iter::from_fn(|| j.next_event()));
                // Dropped, and back.
                events.0.len() >= 2 && h.is_connected() && j.is_connected()
            }
        ));
        let took = from.elapsed();
        assert!(took >= Duration::from_secs(10), "{took:?}");
        // Within five seconds of the link returning.
        assert!(took < Duration::from_secs(15), "{took:?}");
        assert!(
            matches!(events.0.first(), Some(Event::SeatHeld)),
            "{events:?}"
        );
        assert!(matches!(events.0.last(), Some(Event::Resumed(p)) if p.name == "Joiner"));
        assert!(
            matches!(events.1.first(), Some(Event::Reconnecting)),
            "{events:?}"
        );
        assert!(matches!(events.1.last(), Some(Event::Resumed(p)) if p.name == "Host"));
        assert_ne!(join.nonce, old_nonce, "the resumed session has a new nonce");
        assert!(!host.seat_waiting() && !join.seat_waiting());
        // It works, and both channels started over.
        join.send(RELIABLE, b"back".to_vec());
        host.send(RELIABLE, b"welcome back".to_vec());
        let mut got = (None, None);
        assert!(pump(&mut host, &mut join, |h, j| {
            got.0 = got.0.take().or_else(|| h.receive(RELIABLE));
            got.1 = got.1.take().or_else(|| j.receive(RELIABLE));
            got.0.is_some() && got.1.is_some()
        }));
    }

    #[test]
    fn a_wrong_token_is_refused_while_the_seat_is_held() {
        let (mut host, join) = pair(Duration::from_millis(200));
        host.set_seat_hold(Some(Duration::from_secs(5)));
        drop(join);
        // Dropping says Bye; a seat is held only for one who just went silent.
        assert!(
            pump_for(
                &mut host,
                &mut Session::playback("a", "b"),
                Duration::from_millis(100),
                |h, _| !h.is_connected()
            ),
            "Bye heard"
        );
        let (mut host, mut join) = pair(Duration::from_millis(200));
        host.set_seat_hold(Some(Duration::from_secs(5)));
        join.set_seat_hold(Some(Duration::from_secs(5)));
        join.cut_for(Duration::from_secs(5));
        assert!(pump_for(
            &mut host,
            &mut join,
            Duration::from_secs(2),
            |h, _| h.seat_waiting()
        ));
        // Somebody else, with no token and then with a wrong one.
        let port = host.local_addr().unwrap().port();
        let mut other = Session::join(loopback(port), config("Other", 1)).unwrap();
        assert!(pump(&mut host, &mut other, |_, o| o.is_closed()));
        assert!(matches!(other.next_event(), Some(Event::Rejected(r)) if r.contains("waiting")),);
        let mut other = Session::join(loopback(port), config("Other", 1)).unwrap();
        if let Side::Join { resuming, .. } = &mut other.side {
            *resuming = Some(Duration::from_secs(60));
        }
        other.token = host.token.wrapping_add(2);
        assert!(pump(&mut host, &mut other, |_, o| o.is_closed()));
        assert!(matches!(other.next_event(), Some(Event::Rejected(_))));
        assert!(host.seat_waiting(), "the seat is still held");
    }

    #[test]
    fn an_unheld_seat_ends_as_before_and_a_held_one_runs_out() {
        // No hold: a timeout, at once.
        let (mut host, mut join) = pair(Duration::from_millis(200));
        join.cut_for(Duration::from_secs(5));
        assert!(pump_for(
            &mut host,
            &mut join,
            Duration::from_secs(2),
            |h, _| !h.is_connected()
        ));
        assert_eq!(
            host.next_event(),
            Some(Event::Disconnected("the other player timed out".into()))
        );
        // A hold that runs out: the same, later, on both sides.
        let (mut host, mut join) = pair(Duration::from_millis(100));
        let hold = Some(Duration::from_millis(400));
        host.set_seat_hold(hold);
        join.set_seat_hold(hold);
        join.cut_for(Duration::from_secs(5));
        let mut events = (Vec::new(), Vec::new());
        assert!(pump_for(
            &mut host,
            &mut join,
            Duration::from_secs(3),
            |h, j| {
                events.0.extend(std::iter::from_fn(|| h.next_event()));
                events.1.extend(std::iter::from_fn(|| j.next_event()));
                j.is_closed() && events.0.len() >= 2
            }
        ));
        assert_eq!(
            events.0,
            [
                Event::SeatHeld,
                Event::Disconnected("the other player timed out".into())
            ]
        );
        assert_eq!(
            events.1,
            [
                Event::Reconnecting,
                Event::Disconnected("the host timed out".into())
            ]
        );
    }

    #[test]
    fn stale_datagrams_of_the_old_nonce_are_ignored_after_a_resume() {
        let (mut host, mut join) = pair(Duration::from_millis(200));
        let hold = Some(Duration::from_secs(5));
        host.set_seat_hold(hold);
        join.set_seat_hold(hold);
        let (old_nonce, old_addr) = (join.nonce, join.local_addr().unwrap());
        let host_addr = host.local_addr().unwrap();
        join.cut_for(Duration::from_millis(500));
        assert!(pump_for(
            &mut host,
            &mut join,
            Duration::from_secs(3),
            |h, j| h.is_connected() && j.is_connected() && j.nonce != old_nonce
        ));
        while host.next_event().is_some() {}
        // The old session's Bye turns up late, from the same socket's address.
        let stray = UdpSocket::bind("127.0.0.1:0").unwrap();
        for d in [
            Datagram::Bye { nonce: old_nonce },
            Datagram::Stopped { nonce: old_nonce },
            Datagram::Data {
                nonce: old_nonce,
                payload: vec![1, 2, 3],
            },
        ] {
            stray.send_to(&d.encode(), host_addr).unwrap();
        }
        let _ = old_addr;
        assert!(!pump_for(
            &mut host,
            &mut join,
            Duration::from_millis(100),
            |h, _| !h.is_connected()
        ));
        assert_eq!(host.next_event(), None);
    }

    #[test]
    fn nobody_answering_ends_the_join() {
        // A socket that never answers (kept bound so no other test takes
        // its port).
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let free = silent.local_addr().unwrap();
        let mut cfg = config("Joiner", 1);
        cfg.timeout = Duration::from_millis(300);
        let mut join = Session::join(free, cfg).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !join.is_closed() {
            join.update();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            matches!(join.next_event(), Some(Event::Disconnected(r)) if r.contains("no answer"))
        );
    }

    #[test]
    fn a_slow_load_does_not_use_up_the_join_s_wait() {
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let free = silent.local_addr().unwrap();
        let mut cfg = config("Joiner", 1);
        cfg.timeout = Duration::from_millis(100);
        let mut join = Session::join(free, cfg).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        join.update();
        assert!(!join.is_closed());
        std::thread::sleep(Duration::from_millis(150));
        join.update();
        assert!(join.is_closed());
    }

    #[test]
    fn the_browser_finds_a_local_host() {
        let host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut host = host;
        let mut browser = Browser::new(port, [1; 32]).unwrap();
        let mut found = Vec::new();
        for _ in 0..200 {
            browser.update();
            host.update();
            browser.update();
            found = browser.found().to_vec();
            if !found.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "Host");
        assert_eq!(found[0].version, "0.1.0 commit1");
        assert!(found[0].compatible && found[0].open);
        // Answered within the ask's second: its round trip is known.
        assert!(found[0].ping_ms.is_some_and(|ms| ms < 1000), "{found:?}");
    }

    #[test]
    fn lossy_link_still_delivers_reliable_messages() {
        let mut host = Session::host(Ipv4Addr::LOCALHOST, 0, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), config("Joiner", 1)).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        let lossy = Conditions {
            latency_ms: 20,
            jitter_ms: 5,
            loss_percent: 30.,
        };
        host.set_conditions(lossy);
        join.set_conditions(lossy);
        for i in 0..20u8 {
            join.send(RELIABLE, vec![i]);
        }
        let mut got = Vec::new();
        assert!(pump(&mut host, &mut join, |h, _| {
            while let Some(m) = h.receive(RELIABLE) {
                got.push(m[0]);
            }
            got.len() == 20
        }));
        assert_eq!(got, (0..20).collect::<Vec<_>>());
    }

    /// A STUN server of our own on loopback: it answers each Binding Request
    /// with where it came from, its port moved by `shift` (a router that gives
    /// each destination its own port, when two servers differ).
    fn stun_server(shift: u16) -> String {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = socket.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut b = [0u8; 512];
            while let Ok((n, SocketAddr::V4(from))) = socket.recv_from(&mut b) {
                let Some(tx) = direct::stun_transaction(&b[..n]) else {
                    continue;
                };
                let port = from.port().wrapping_add(shift) ^ 0x2112;
                let ip = u32::from(*from.ip()) ^ 0x2112_a442;
                let mut answer = vec![0x01, 0x01, 0, 12, 0x21, 0x12, 0xa4, 0x42];
                answer.extend(tx);
                answer.extend([0x00, 0x20, 0, 8, 0, 1]);
                answer.extend(port.to_be_bytes());
                answer.extend(ip.to_be_bytes());
                let _ = socket.send_to(&answer, from);
            }
        });
        addr.to_string()
    }

    fn next_room_event(host: &mut Session) -> Option<Event> {
        for _ in 0..400 {
            host.update();
            while let Some(e) = host.next_event() {
                if matches!(e, Event::RoomCode(_) | Event::NoRoomCode(_)) {
                    return Some(e);
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    #[test]
    fn a_direct_code_carries_the_host_to_its_joiner() {
        let world = DirectConfig {
            stun: vec![stun_server(0), stun_server(0)],
            map_ports: false,
        };
        let mut host =
            Session::host_direct(Ipv4Addr::LOCALHOST, 0, world, config("Host", 1)).unwrap();
        let port = host.local_addr().unwrap().port();
        let Some(Event::RoomCode(code)) = next_room_event(&mut host) else {
            panic!("no code");
        };
        let addr = direct::parse_code(&code).unwrap();
        assert_eq!(addr, SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
        let world = DirectConfig {
            stun: Vec::new(),
            map_ports: false,
        };
        let mut join = Session::join_direct(addr, world, config("Joiner", 1)).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
        assert_eq!(join.route(), Route::Direct);
        join.send(RELIABLE, vec![5]);
        let mut got = None;
        assert!(pump(&mut host, &mut join, |h, _| {
            got = h.receive(RELIABLE);
            got.is_some()
        }));
        assert_eq!(got, Some(vec![5]));
    }

    #[test]
    fn a_joiner_not_let_in_gets_a_reply_code_and_the_host_knocks_back() {
        // A host that never answers: its router drops what comes from strangers.
        let quiet = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let SocketAddr::V4(quiet_at) = quiet.local_addr().unwrap() else {
            panic!()
        };
        let world = DirectConfig {
            stun: vec![stun_server(0)],
            map_ports: false,
        };
        // Its 2 s timeout is not its wait: a direct code's is longer.
        let mut join = Session::join_direct(quiet_at, world, config("Joiner", 1)).unwrap();
        let mut reply = None;
        for _ in 0..140 {
            join.update();
            while let Some(e) = join.next_event() {
                if let Event::ReplyCode(code) = e {
                    reply = Some(code);
                }
            }
            if reply.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let reply = reply.expect("no reply code");
        let joiner = direct::parse_code(&reply).unwrap();
        assert_eq!(SocketAddr::V4(joiner), join.local_addr().unwrap());
        // The host types it in and knocks toward the joiner.
        let world = DirectConfig {
            stun: Vec::new(),
            map_ports: false,
        };
        let mut host =
            Session::host_direct(Ipv4Addr::LOCALHOST, 0, world, config("Host", 1)).unwrap();
        // A plain socket stands in for the joiner's router, to read the knocks.
        let door = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        door.set_nonblocking(true).unwrap();
        let SocketAddr::V4(door_at) = door.local_addr().unwrap() else {
            panic!()
        };
        host.knock_toward(door_at);
        let mut knocked = None;
        let mut b = [0u8; 256];
        for _ in 0..100 {
            host.update();
            if let Ok((n, _)) = door.recv_from(&mut b) {
                knocked = Datagram::decode(&b[..n]);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            matches!(knocked, Some(Datagram::Punch { .. })),
            "{knocked:?}"
        );
    }

    #[test]
    fn a_port_per_destination_gives_no_code_and_says_why() {
        let world = DirectConfig {
            stun: vec![stun_server(0), stun_server(7)],
            map_ports: false,
        };
        let mut host =
            Session::host_direct(Ipv4Addr::LOCALHOST, 0, world, config("Host", 1)).unwrap();
        match next_room_event(&mut host) {
            Some(Event::NoRoomCode(why)) => assert!(why.contains("its own port"), "{why}"),
            other => panic!("{other:?}"),
        }
        // Still a host for the LAN.
        let port = host.local_addr().unwrap().port();
        let mut join = Session::join(loopback(port), config("Joiner", 1)).unwrap();
        assert!(pump(&mut host, &mut join, |h, j| h.is_connected()
            && j.is_connected()));
    }

    #[test]
    fn no_stun_answer_gives_no_code() {
        // A port nothing listens on.
        let quiet = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let world = DirectConfig {
            stun: vec![quiet.local_addr().unwrap().to_string()],
            map_ports: false,
        };
        let mut host =
            Session::host_direct(Ipv4Addr::LOCALHOST, 0, world, config("Host", 1)).unwrap();
        let mut event = None;
        for _ in 0..80 {
            host.update();
            if let Some(e) = host.next_event() {
                event = Some(e);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        match event {
            Some(Event::NoRoomCode(why)) => assert!(why.contains("STUN"), "{why}"),
            other => panic!("{other:?}"),
        }
    }
}
