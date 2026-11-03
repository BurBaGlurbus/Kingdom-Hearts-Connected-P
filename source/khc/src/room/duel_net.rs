//! The duel's LAN session in the room: hosting or joining (`--host`,
//! `--join`), the session's own tick at the start and end of every fixed
//! step, the other player arriving and leaving, and the network overlay
//! (F3 > Sora > Display, or `--net-overlay`).
//!
//! The host simulates both Soras; the joiner sends its buttons and draws
//! the host's snapshots (duel_sync.rs).

use super::*;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use net::link::Conditions;
use net::wire::{Cap, Capabilities};
use net::{Event as NetEvent, Role, Session, SessionConfig};
use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::time::Duration;

/// What two players compare: the workspace's version and the commit it was built from.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " ", env!("KHC_COMMIT"));
/// How long a dropped joiner's seat is held, from the last word [the brief's
/// default until the owner says otherwise, KHC-027].
pub const SEAT_HOLD: Duration = Duration::from_secs(30);
/// How long a match goes on with its other player silent: it pauses well before
/// the timeout, so a drop costs nobody five seconds of free hits [duel choice,
/// KHC-027].
const PAUSE_AFTER_MS: f64 = 1000.;
/// How long a silent peer is waited for.
const TIMEOUT: Duration = Duration::from_secs(5);
/// A PC that is just starting or loading (a dev build's first frame, KH3's files) can stay silent longer than a lost
/// peer would: this long, for the first moments of the process and of a connection.
const START_TIMEOUT: Duration = Duration::from_secs(45);
const START_WINDOW: Duration = Duration::from_secs(60);
/// Overlay text: size, line height, the top-right corner (below the duel's
/// HP gauge) and colours.
const SIZE: f32 = 10.;
const LINE: f32 = 12.;
const RIGHT: f32 = 590.;
const TOP: f32 = 46.;
const TEXT: [f32; 4] = [1.6, 1.6, 1.6, 1.];
const WARN: [f32; 4] = [1.8, 1.3, 0.5, 1.];
/// The vote's list: size, line height, and the cursor row's colour.
const LOBBY_SIZE: f32 = 12.;
const LOBBY_LINE: f32 = 15.;
const CURSOR: [f32; 4] = [1.9, 1.6, 0.7, 1.];
/// The vote's panel top.
const VOTE_TOP: f32 = 290.;
const BACKING: [f32; 4] = [0., 0., 0., 0.55];
/// The status balloon's top (WINDOW_BALLOON at y 0x30) and its frame
/// beyond the text.
const BALLOON_TOP: i32 = 0x30;
const BALLOON_PAD: i32 = 28;
/// The match's text over the field: its shadow's opacity.
const SHADOW: f32 = 0.7;
/// The banner ("Waiting for a player", "Opponent left"): its size and row.
const BANNER_SIZE: f32 = 16.;
const BANNER_Y: f32 = 120.;
/// The middle of the 512-wide layout.
const BANNER_CENTRE: f32 = 256.;

/// Where the session stands, for the banner.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Host: waiting for a player on this port.
    Waiting(u16),
    /// Joiner: knocking at the host.
    Joining(SocketAddr),
    /// Joiner: looking a room code up.
    JoiningRoom(String),
    Playing,
    /// The session ended at this tick; the reason to show.
    Ended(String, u64),
}

/// What `--host` / `--join` asked for.
#[derive(Clone, Debug)]
pub struct Setup {
    pub role: Role,
    /// The host's address to listen on (every local one by default).
    pub bind: std::net::Ipv4Addr,
    pub port: u16,
    pub host: Option<SocketAddr>,
    pub name: String,
    pub conditions: Conditions,
    pub overlay: bool,
    /// `--net-log`: where the network log goes.
    pub net_log: Option<std::path::PathBuf>,
    /// Where each match's replay goes: `--replay-out`, or a new file in the
    /// replays folder, or (an unattended run) nowhere.
    pub replay_out: Option<Option<std::path::PathBuf>>,
    /// What this install is remembered to hold, for a refusal before it is read.
    pub remembered: Capabilities,
    /// Host: one snapshot in this many ticks (`--snapshot-rate`).
    pub snapshot_every: u32,
    /// [Test] `--net-cut`.
    pub cut: Option<(u64, Duration)>,
    /// `--relay`: the rendezvous and relay server; `--room`: the code to join
    /// through it, and [Test] `--relay-only`.
    pub relay: Option<SocketAddr>,
    pub room: Option<[u8; net::wire::CODE_LENGTH]>,
    pub relay_only: bool,
    /// [KHC-051] A direct room code: a host makes one from its own address, a
    /// joiner's `host` came from one.
    pub direct: bool,
}

#[derive(Resource)]
pub struct DuelNet {
    /// Until when silence is judged by `START_TIMEOUT`.
    slow_until: std::time::Instant,
    pub session: Session,
    /// The running executable's hash.
    pub build: [u8; 32],
    /// What this install was remembered to hold, until it is read again; and
    /// what the other player was last told of it.
    remembered: Capabilities,
    sent_caps: Option<Capabilities>,
    /// The network log's file, and when its last line went out (duel_log.rs).
    pub net_log: Option<std::io::LineWriter<std::fs::File>>,
    pub log_at: std::time::Instant,
    /// Where a session's replay goes, and the one being written.
    replay_out: Option<Option<std::path::PathBuf>>,
    pub replay: Option<duel_replay::Recorder>,
    /// The other player's game stopped on an error: the result says so.
    pub peer_stopped: bool,
    /// A match is paused for the other player's link (KHC-027).
    pub paused: bool,
    /// The session came back under a new nonce: the match's screens send again.
    pub resumed: bool,
    /// [Test] `--net-cut`: this far into the script the link goes for this long.
    pub cut: Option<(u64, Duration)>,
    /// Host: the room code the relay gave, and that it has not answered.
    pub room_code: Option<String>,
    pub relay_silent: bool,
    /// [KHC-051] Host: a line on whether a direct code will let a friend in, or
    /// why there is no code.
    pub room_note: Option<String>,
    pub no_room_code: Option<String>,
    /// [KHC-053] Joiner: its reply code. Host: the reply code being typed, and
    /// what came of the last one.
    pub reply_code: Option<String>,
    pub reply_typed: String,
    pub reply_said: Option<String>,
    pub role: Role,
    pub name: String,
    pub peer: Option<String>,
    pub status: Status,
    pub overlay: bool,
    /// Joiner: its newest ticks of buttons (redundancy).
    pub recent: VecDeque<(u32, rival::RivalPad)>,
    /// Host: snapshots going out, one in `snapshot_every` of them (`--snapshot-rate`),
    /// counted, what the joiner has of them, and what their flying things took.
    pub encoder: duel_wire::Encoder,
    pub snapshot_every: u32,
    pub send_count: u32,
    pub flight_bytes: u64,
    /// Host: the joiner reads his Sora's motion from the snapshots.
    pub peer_predicts: bool,
    /// Joiner: snapshots coming in.
    pub decoder: duel_wire::Decoder,
    /// Bytes by kind at the last log line (duel_log.rs).
    pub tallied: duel_log::ByKind,
    /// Host: the joiner's buttons by joiner tick, not yet used.
    pub inputs: BTreeMap<u32, rival::RivalPad>,
    /// Host: the newest joiner tick used.
    pub used_input: Option<u32>,
    /// Host: the snapshot on the joiner's screen when those buttons were made,
    /// by joiner tick.
    pub input_shown: BTreeMap<u32, u64>,
    /// Joiner: snapshots by host tick, waiting to be shown.
    pub snapshots: BTreeMap<u64, duel_wire::Snapshot>,
    /// Joiner: the host tick on screen.
    pub shown_snapshot: Option<u64>,
    /// Joiner: when each snapshot is shown (duel_playout.rs), and the two being
    /// shown between.
    pub playout: duel_playout::Playout,
    pub glide: duel_sync::Glide,
    /// Joiner: the longest wait with nothing to show since the last log line, the
    /// jumps ahead, and the wait now running.
    pub froze_longest: u32,
    pub jumps: u64,
    /// This tick's reliable messages, each taken by the system it is for
    /// (the rest are dropped at the end of the tick).
    pub inbox: Vec<Vec<u8>>,
    /// Frames since the link's numbers were last logged.
    stats_frames: u32,
    /// Joiner: the host's events by host tick, waiting for that tick's
    /// snapshot.
    pub events: VecDeque<(u64, Vec<duel_wire::DuelEvent>)>,
    /// Ticks the host had no new buttons for (late input) and ticks the
    /// joiner had no new snapshot for; reset each overlay refresh.
    pub starved: u32,
    pub stale: u32,
    /// Ticks in a row without them (the overlay warns at half a second).
    pub starve_run: u32,
    pub stale_run: u32,
    /// The link simulator's preset (`SIMULATED`), or None for the one
    /// `--net-sim` gave.
    pub preset: Option<usize>,
    /// Snapshots skipped to catch up, inputs dropped as too old.
    pub skipped: u64,
    /// Host: the rules last sent to the joiner.
    pub sent_rules: Option<duel_rules::MatchRules>,
    /// The smoothed round trip, and the wait it asks of the host's buttons
    /// (duel_delay.rs): the host's in force once its Soras are next held, the
    /// joiner's only for showing.
    pub delay_meter: duel_delay::DelayMeter,
    pub host_delay_ticks: u8,
    pub warnings: VecDeque<String>,
    since_refresh: u32,
    lines: Vec<(String, bool)>,
    sheets: Option<(usize, usize)>,
    /// The status in the game's message balloon: its art (read once, on
    /// its own thread), the text wanted this frame, the window showing it
    /// and those still closing, and the art's 2D sheets.
    notice_job: Option<std::thread::JoinHandle<Result<NoticeArt, String>>>,
    notice_art: Option<Result<NoticeArt, String>>,
    pub notice_want: Option<String>,
    notice: Option<(String, visual::dialogue::TalkWindow)>,
    notices_closing: Vec<visual::dialogue::TalkWindow>,
    notice_sheets: Option<(usize, usize)>,
    /// The match's art on the 2D layer (duel_match_room::MatchArt).
    match_sheets: Option<[usize; 3]>,
}

/// YS::WINDOW_BALLOON's art as the world map reads it (msg/us/fontinfo.bar
/// and wm.bar's window, the event font, 03system 'ftst' colour 5 of the
/// world map) [duel choice: the session's status in the game's own message
/// window].
pub struct NoticeArt {
    sheet: Sheet,
    font: SystemFont,
    colour: u32,
}

fn load_notice_art(source: &ProbeSource) -> Result<NoticeArt, String> {
    let mut library = data::path_lookup::Library::open(source).map_err(|e| e.to_string())?;
    let at = |name: &str| {
        library
            .find(name)
            .map(|f| (f.index, f.ordinal, f.name_hash))
            .ok_or(format!("{name} is not installed"))
    };
    let info = at("msg/us/fontinfo.bar")?;
    let sheet =
        hud::load_window_sheet(source, info, at("msg/us/wm.bar")?).map_err(|e| e.to_string())?;
    let font = hud::load_font(source, at("msg/us/fontimage.bar")?, info, b"evt\0")
        .map_err(|e| e.to_string())?;
    let colour = library
        .read("03system.bin")
        .ok()
        .and_then(|s| hud::ftst_colour(&s, 5, 15).ok())
        .unwrap_or(0x8005_0520);
    Ok(NoticeArt {
        sheet,
        font,
        colour,
    })
}

/// A notice's text in game text: its lines (`\n`) end in 0x02.
fn notice_text(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push(0x02);
        }
        out.extend(pause_menu::encode_text(line));
    }
    out.push(0);
    out
}

/// SHA-256 of the running executable: two instances play together only on
/// the same build.
pub fn build_identity() -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    Ok(Sha256::digest(&bytes).into())
}

/// [KHC-044] The profile's name when `--name` is not given: never the
/// Windows user name, which would show other players who is at this PC.
pub fn default_name() -> String {
    crate::profile::load_or_create(&crate::profile::dir(), None)
        .map(|p| p.name)
        .unwrap_or_else(|_| "Player".into())
}

/// The F3 menu's link simulator presets: name and conditions (what this
/// instance sends; choose the same on both for a round trip).
pub const SIMULATED: [(&str, Conditions); 4] = [
    (
        "Off",
        Conditions {
            latency_ms: 0,
            jitter_ms: 0,
            loss_percent: 0.,
        },
    ),
    (
        "Wi-Fi",
        Conditions {
            latency_ms: 15,
            jitter_ms: 5,
            loss_percent: 0.5,
        },
    ),
    (
        "Busy Wi-Fi",
        Conditions {
            latency_ms: 50,
            jitter_ms: 20,
            loss_percent: 3.,
        },
    ),
    (
        "Bad link",
        Conditions {
            latency_ms: 120,
            jitter_ms: 40,
            loss_percent: 10.,
        },
    ),
];

/// `HOST[:PORT]` (the default port when none is given).
pub fn parse_address(text: &str) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    let text = text.trim();
    let with_port = if text.contains(':') {
        text.to_owned()
    } else {
        format!("{text}:{}", net::DEFAULT_PORT)
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("{text}: {e}"))?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("{text}: no IPv4 address"))
}

/// `LATENCY,JITTER,LOSS` (milliseconds, milliseconds, percent).
pub fn parse_conditions(text: &str) -> Result<Conditions, String> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    let [latency, jitter, loss] = parts.as_slice() else {
        return Err("expected LATENCY,JITTER,LOSS (ms, ms, percent)".into());
    };
    let ms = |s: &str| s.parse::<u32>().map_err(|e| format!("{s}: {e}"));
    let loss = loss.parse::<f32>().map_err(|e| format!("{loss}: {e}"))?;
    if !(0. ..=100.).contains(&loss) {
        return Err("loss must be 0..100".into());
    }
    Ok(Conditions {
        latency_ms: ms(latency)?,
        jitter_ms: ms(jitter)?,
        loss_percent: loss,
    })
}

impl DuelNet {
    pub fn start(setup: &Setup) -> Result<DuelNet, String> {
        let build = build_identity()?;
        let config = SessionConfig {
            name: setup.name.clone(),
            build,
            version: VERSION.to_owned(),
            caps: setup.remembered,
            timeout: TIMEOUT,
            // [KHC-044] The profile's id: unverified until ids are signed.
            player: crate::profile::load_or_create(&crate::profile::dir(), None)
                .map(|p| p.player_id().0)
                .unwrap_or_default(),
        };
        let (session, status) = match (setup.role, setup.host) {
            (Role::Host, _) => (
                match setup.relay {
                    Some(relay) => Session::host_room(setup.bind, setup.port, relay, config),
                    None if setup.direct => Session::host_direct(
                        setup.bind,
                        setup.port,
                        net::DirectConfig::default(),
                        config,
                    ),
                    None => Session::host(setup.bind, setup.port, config),
                }
                .map_err(|e| format!("could not host on UDP port {}: {e}", setup.port))?,
                Status::Waiting(setup.port),
            ),
            (Role::Join, None) if setup.room.is_some() && setup.relay.is_some() => {
                let (Some(relay), Some(code)) = (setup.relay, setup.room) else {
                    return Err("a room needs its relay".into());
                };
                (
                    Session::join_room(relay, code, config)
                        .map_err(|e| format!("could not reach {relay}: {e}"))?,
                    Status::JoiningRoom(String::from_utf8_lossy(&code).into_owned()),
                )
            }
            (Role::Join, Some(SocketAddr::V4(host))) if setup.direct => (
                Session::join_direct(host, net::DirectConfig::default(), config)
                    .map_err(|e| format!("could not join {host}: {e}"))?,
                Status::JoiningRoom(net::direct::encode_code(host)),
            ),
            (Role::Join, Some(host)) => (
                Session::join(host, config).map_err(|e| format!("could not join {host}: {e}"))?,
                Status::Joining(host),
            ),
            (Role::Join, None) => return Err("join needs the host's address".into()),
        };
        let mut session = session;
        session.set_conditions(setup.conditions);
        session.set_relay_only(setup.relay_only);
        Self::with_session(setup, session, status, build)
    }

    /// A replay's session: the joiner's code, fed from a file.
    pub fn playback(setup: &Setup, host_name: &str) -> Result<DuelNet, String> {
        let session = Session::playback(&setup.name, host_name);
        let status = Status::Joining(SocketAddr::from(([127, 0, 0, 1], 0)));
        Self::with_session(setup, session, status, [0; 32])
    }

    fn with_session(
        setup: &Setup,
        session: Session,
        status: Status,
        build: [u8; 32],
    ) -> Result<DuelNet, String> {
        let net_log = match &setup.net_log {
            Some(path) => Some(duel_log::open(path)?),
            None => None,
        };
        Ok(DuelNet {
            slow_until: std::time::Instant::now() + START_WINDOW,
            session,
            build,
            remembered: setup.remembered,
            sent_caps: None,
            net_log,
            log_at: std::time::Instant::now(),
            replay_out: setup.replay_out.clone(),
            replay: None,
            peer_stopped: false,
            paused: false,
            resumed: false,
            cut: setup.cut,
            room_code: None,
            relay_silent: false,
            room_note: None,
            no_room_code: None,
            reply_code: None,
            reply_typed: String::new(),
            reply_said: None,
            role: setup.role,
            name: setup.name.clone(),
            peer: None,
            status,
            overlay: setup.overlay,
            recent: VecDeque::new(),
            encoder: duel_wire::Encoder::default(),
            snapshot_every: setup.snapshot_every,
            send_count: 0,
            flight_bytes: 0,
            peer_predicts: false,
            decoder: duel_wire::Decoder::default(),
            tallied: duel_log::ByKind::default(),
            inputs: BTreeMap::new(),
            used_input: None,
            input_shown: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            shown_snapshot: None,
            playout: duel_playout::Playout::default(),
            glide: duel_sync::Glide::default(),
            froze_longest: 0,
            jumps: 0,
            starved: 0,
            stale: 0,
            starve_run: 0,
            stale_run: 0,
            preset: setup.conditions.is_clear().then_some(0),
            skipped: 0,
            sent_rules: None,
            delay_meter: duel_delay::DelayMeter::default(),
            host_delay_ticks: 0,
            events: VecDeque::new(),
            inbox: Vec::new(),
            stats_frames: 0,
            warnings: VecDeque::new(),
            since_refresh: 0,
            lines: Vec::new(),
            sheets: None,
            notice_job: None,
            notice_art: None,
            notice_want: None,
            notice: None,
            notices_closing: Vec::new(),
            notice_sheets: None,
            match_sheets: None,
        })
    }

    pub fn playing(&self) -> bool {
        self.status == Status::Playing
    }

    fn warn(&mut self, text: String) {
        if self.warnings.len() >= 4 {
            self.warnings.pop_front();
        }
        self.warnings.push_back(text);
    }
}

/// What this install holds now, as read this session.
pub fn capabilities(game: &Game) -> Capabilities {
    let o = game.ext.online();
    Capabilities {
        kh3: match (&o.kh3_install, o.kh3_failed, o.kh3_digest) {
            (None, ..) | (_, true, _) => Cap::Absent,
            (_, _, Some(hash)) => Cap::Hash(hash),
            _ => Cap::Unread,
        },
        bbs: match (&o.mirage, o.bbs_digest) {
            (mirage_arena::Avail::Missing(_), _) => Cap::Absent,
            (mirage_arena::Avail::Ready, Some(hash)) => Cap::Hash(hash),
            _ => Cap::Unread,
        },
    }
}

/// Tells the other player whenever this install's standing changes, and keeps it
/// for a Hello or a refusal; until a file is read the remembered hash stands in
/// there, never in what the lobby decides on.
fn tell_capabilities(net: &mut DuelNet, game: &mut Game) {
    let now = capabilities(game);
    let or_remembered = |cap: Cap, kept: Cap| if cap == Cap::Unread { kept } else { cap };
    net.session.set_caps(Capabilities {
        kh3: or_remembered(now.kh3, net.remembered.kh3),
        bbs: or_remembered(now.bbs, net.remembered.bbs),
    });
    if net.session.is_connected() && net.sent_caps != Some(now) {
        net.sent_caps = Some(now);
        net.session
            .send(net::RELIABLE, duel_wire::encode_caps(&now));
        let tick = game.tick;
        let text = |cap: Cap| match cap {
            Cap::Absent => "absent".to_owned(),
            Cap::Unread => "unread".to_owned(),
            Cap::Hash(hash) => net::wire::short_hash(&hash),
        };
        game.event(serde_json::json!({"event":"duel_caps","tick":tick,
            "kh3":text(now.kh3),"bbs":text(now.bbs)}));
    }
}

/// The slot this instance's own Sora takes.
pub fn local_slot(role: Role) -> u8 {
    match role {
        Role::Host => fighter_slots::HOST,
        Role::Join => fighter_slots::JOIN,
    }
}

/// The slot the other player's Sora takes.
pub fn remote_slot(role: Role) -> u8 {
    match role {
        Role::Host => fighter_slots::JOIN,
        Role::Join => fighter_slots::HOST,
    }
}

pub mod fighter_slots {
    pub const HOST: u8 = 0;
    pub const JOIN: u8 = 1;
}

/// Start of every fixed step: what arrived, the other player coming and
/// going.
/// Whether an address is this PC's own (loopback, or one of its
/// interfaces: binding to it works).
pub fn is_local(ip: std::net::IpAddr) -> bool {
    ip.is_loopback() || std::net::UdpSocket::bind((ip, 0)).is_ok()
}

/// `--input auto` (the default): two players on one PC each get their own
/// devices, the host the keyboard and mouse and the joiner a controller
/// [choice, T256]; otherwise each reads all of them.
fn assign_devices(controls: &mut visual::input::Controls, role: Role, peer: SocketAddr) -> bool {
    use visual::input::{Assign, Devices};
    if controls.wanted != Assign::Auto {
        return false;
    }
    let devices = match (is_local(peer.ip()), role) {
        (true, Role::Host) => Devices::KeyboardMouse,
        (true, Role::Join) => Devices::Pad(None),
        (false, _) => Devices::Both,
    };
    let changed = controls.devices != devices;
    controls.devices = devices;
    changed
}

pub fn begin(
    net: Option<ResMut<DuelNet>>,
    mut controls: ResMut<visual::input::Controls>,
    mut game: ResMut<Game>,
    mut loadout: ResMut<duel_loadout::DuelLoadout>,
    gallery: Res<enemy_gallery::EnemyGallery>,
) {
    let Some(mut net) = net else { return };
    duel_predict::hide(&mut game);
    notice_tick(&mut net, &gallery, &mut game);
    let wait = if std::time::Instant::now() < net.slow_until {
        START_TIMEOUT
    } else {
        TIMEOUT
    };
    net.session.set_timeout(wait);
    net.session.update();
    net.inbox.clear();
    while let Some(m) = net.session.receive(net::RELIABLE) {
        net.inbox.push(m);
    }
    while let Some(event) = net.session.next_event() {
        let tick = game.tick;
        match event {
            NetEvent::Connected(peer) => {
                net.slow_until = std::time::Instant::now() + START_WINDOW;
                if assign_devices(&mut controls, net.role, peer.addr) {
                    let devices = controls.devices.label();
                    game.event(serde_json::json!({"event":"input_devices","tick":tick,
                        "devices":devices}));
                }
                net.peer = Some(peer.name.clone());
                net.peer_stopped = false;
                net.status = Status::Playing;
                crate::crash::note_session(
                    net.build,
                    vec![net.name.clone(), peer.name.clone()],
                    net.session.stop_notice(),
                );
                let out = net.replay_out.clone();
                net.replay = out.map(|path| {
                    duel_replay::Recorder::new(path.unwrap_or_else(duel_replay::auto_path))
                });
                let keep = net.replay.is_some();
                net.session.set_tap(keep);
                net.sent_caps = None;
                net.encoder = duel_wire::Encoder::default();
                net.decoder = duel_wire::Decoder::default();
                net.peer_predicts = false;
                net.inputs.clear();
                net.used_input = None;
                net.input_shown.clear();
                game.ext.online_mut().rewind.shown = None;
                net.snapshots.clear();
                net.shown_snapshot = None;
                net.playout = duel_playout::Playout::default();
                net.glide = duel_sync::Glide::default();
                net.events.clear();
                net.sent_rules = None;
                net.delay_meter = duel_delay::DelayMeter::default();
                let slot = remote_slot(net.role);
                rival_play::spawn_remote(&mut game, slot);
                // The host's Drive rolls (Antiform) start from the clock, a
                // different sequence each session; each Sora draws his own.
                if net.role == Role::Host {
                    let clock = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(1, |d| d.subsec_nanos() ^ d.as_secs() as u32);
                    // [Test] `--duel-seed`: the same rolls each run.
                    let clock = game.ext.online().duel_seed.unwrap_or(clock);
                    let g = &mut *game;
                    let drives = g
                        .drive
                        .as_mut()
                        .into_iter()
                        .chain(g.rivals.iter_mut().filter_map(|r| r.own.drive.as_mut()));
                    for (k, d) in drives.enumerate() {
                        d.seed = clock.wrapping_add(k as u32 * 0x9e37_79b9) | 1;
                    }
                }
                game.event(serde_json::json!({"event":"net_connected","tick":tick,
                    "peer":peer.name,"address":peer.addr.to_string(),
                    "version":peer.version,"ours":VERSION,
                    "route":net.session.route().label()}));
            }
            NetEvent::RoomCode(code) => {
                game.event(serde_json::json!({"event":"duel_room_code","tick":tick,"code":code}));
                // For the launcher, which shows the room it started.
                println!("KHC room code: {code}");
                net.room_code = Some(code);
                net.relay_silent = false;
            }
            NetEvent::RelaySilent => {
                game.event(serde_json::json!({"event":"duel_relay_silent","tick":tick}));
                net.relay_silent = true;
            }
            NetEvent::RoomNote(note) => {
                game.event(serde_json::json!({"event":"duel_room_note","tick":tick,"note":note}));
                net.room_note = Some(note);
            }
            NetEvent::ReplyCode(code) => {
                game.event(serde_json::json!({"event":"duel_reply_code","tick":tick,"code":code}));
                net.reply_code = Some(code);
            }
            NetEvent::NoRoomCode(why) => {
                game.event(serde_json::json!({"event":"duel_no_room_code","tick":tick,"why":why}));
                println!("KHC no room code: {why}");
                net.no_room_code = Some(why);
            }
            NetEvent::SeatHeld | NetEvent::Reconnecting => {
                net.paused = true;
                let unix_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64);
                game.event(serde_json::json!({"event":"net_seat_waiting","tick":tick,
                    "unix_ms":unix_ms}));
            }
            NetEvent::Resumed(peer) => {
                net.paused = false;
                net.resumed = true;
                net.sent_caps = None;
                net.sent_rules = None;
                net.encoder = duel_wire::Encoder::default();
                net.decoder = duel_wire::Decoder::default();
                net.inputs.clear();
                net.input_shown.clear();
                net.snapshots.clear();
                net.playout = duel_playout::Playout::default();
                crate::crash::note_session(
                    net.build,
                    vec![net.name.clone(), peer.name.clone()],
                    net.session.stop_notice(),
                );
                let unix_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64);
                game.event(serde_json::json!({"event":"net_resumed","tick":tick,
                    "address":peer.addr.to_string(),"unix_ms":unix_ms}));
            }
            NetEvent::Rejected(reason) => {
                game.event(serde_json::json!({"event":"net_rejected","tick":tick,"reason":reason}));
                net.status = Status::Ended(format!("The host refused: {reason}."), tick);
                leave_session(&mut game, &mut loadout);
            }
            NetEvent::Disconnected(reason) => {
                net.paused = false;
                crate::crash::session_over();
                net.peer_stopped = reason == net::session::STOPPED_REASON;
                let unix_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64);
                game.event(serde_json::json!({"event":"net_disconnected","tick":tick,
                    "reason":reason,"unix_ms":unix_ms}));
                game.rivals.clear();
                // The name stays for the result screen ("X left the duel").
                let text = match (net.role, net.peer.clone()) {
                    // The host keeps listening for the next player.
                    (Role::Host, who) => format!(
                        "{}\nWaiting for the next player.",
                        gone(who.as_deref().unwrap_or("The other player"), &reason)
                    ),
                    (Role::Join, Some(who)) => format!("{}\nEsc to quit.", gone(&who, &reason)),
                    (Role::Join, None) => format!("Could not join: {reason}."),
                };
                net.status = Status::Ended(text, tick);
                if net.role == Role::Join {
                    leave_session(&mut game, &mut loadout);
                }
            }
        }
    }
    tell_capabilities(&mut net, &mut game);
    hold_the_seat(&mut net, &mut game);
    duel_delay::update(&mut net, &mut game);
    // A host whose player left goes back to waiting once the banner was read.
    if net.role == Role::Host
        && let Status::Ended(_, since) = net.status
        && game.tick.saturating_sub(since) >= BANNER_TICKS
    {
        let port = net.session.local_addr().map(|a| a.port()).unwrap_or(0);
        net.status = Status::Waiting(port);
        net.peer = None;
    }
}

/// A match keeps its other player's seat through a drop: quiet for a second it
/// pauses, and the session holds the seat or knocks again once it has timed out.
fn hold_the_seat(net: &mut DuelNet, game: &mut Game) {
    let live = game.ext.online().match_live;
    net.session.set_seat_hold(live.then_some(SEAT_HOLD));
    let elapsed = game.ext.online().script.elapsed();
    if live
        && let Some((after, time)) = net.cut
        && elapsed.is_some_and(|elapsed| elapsed >= after)
    {
        net.cut = None;
        net.session.cut_for(time);
        let tick = game.tick;
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        game.event(serde_json::json!({"event":"net_cut","tick":tick,
            "seconds":time.as_secs_f32(),"unix_ms":unix_ms}));
    }
    let quiet = net.session.is_connected() && net.session.stats().silent_ms > PAUSE_AFTER_MS;
    let paused = live && (quiet || net.session.seat_waiting());
    if paused != net.paused {
        net.paused = paused;
        let tick = game.tick;
        game.event(serde_json::json!({"event":"duel_paused","tick":tick,"paused":paused}));
    }
}

/// The banner while a match waits for the other player's link.
pub fn waiting_text(net: &DuelNet) -> Option<String> {
    if !net.paused {
        return None;
    }
    let who = net.peer.as_deref().unwrap_or("the other player");
    let left = net
        .session
        .seat_time_left()
        .unwrap_or(SEAT_HOLD)
        .as_secs()
        .min(SEAT_HOLD.as_secs());
    Some(match net.role {
        Role::Host => format!("Waiting for {who}. {left} s"),
        Role::Join => format!("Reconnecting to {who}. {left} s"),
    })
}

/// [KHC-053] A direct host types a joiner's reply code while it waits: letters,
/// digits and dashes, Backspace, Enter to knock toward it.
fn type_reply_code(net: &mut DuelNet, typed: &mut MessageReader<KeyboardInput>) {
    let direct = net.role == Role::Host
        && matches!(net.status, Status::Waiting(_))
        && net.room_code.as_deref().is_some_and(|c| c.contains('-'));
    if !direct {
        typed.clear();
        return;
    }
    for key in typed.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Character(chars) => {
                for c in chars.chars() {
                    if (c.is_ascii_alphanumeric() || c == '-')
                        && net.reply_typed.len() < net::direct::DIRECT_CODE_LENGTH + 2
                    {
                        net.reply_typed.push(c.to_ascii_uppercase());
                    }
                }
            }
            Key::Backspace => {
                net.reply_typed.pop();
            }
            Key::Enter if !net.reply_typed.is_empty() => {
                let text = std::mem::take(&mut net.reply_typed);
                net.reply_said = Some(match net::direct::parse_code(&text) {
                    Some(joiner) => {
                        net.session.knock_toward(joiner);
                        "Letting your friend in...".into()
                    }
                    None => format!("{text} is not a reply code: check it"),
                });
            }
            _ => {}
        }
    }
}

/// The host's second line: the reply code typed, what came of it, or the
/// hint to type one [KHC-063: two short lines in all; the room note stays in
/// the log].
fn reply_line(net: &DuelNet) -> String {
    if !net.reply_typed.is_empty() {
        format!("Their reply code: {}_", net.reply_typed)
    } else if let Some(said) = &net.reply_said {
        said.clone()
    } else {
        "Stuck? Type their reply code".into()
    }
}

/// Why the other player is gone, by name (the link's reasons name no one:
/// "the other player left", "the host timed out").
fn gone(who: &str, reason: &str) -> String {
    if reason == net::session::STOPPED_REASON {
        "Opponent's game stopped on an error.".to_owned()
    } else if reason.ends_with(" left") {
        format!("{who} left the duel.")
    } else if reason.ends_with(" timed out") {
        format!("{who} stopped answering.")
    } else {
        format!("{who} left: {reason}.")
    }
}

/// The status balloon, one fixed step: its art is read once a session
/// runs; a changed status closes the window showing the old one (its own
/// close animation) and the new one opens once it has gone.
fn notice_tick(net: &mut DuelNet, gallery: &enemy_gallery::EnemyGallery, game: &mut Game) {
    if net.notice_art.is_none() && net.notice_job.is_none() {
        let source = Arc::clone(&gallery.source);
        net.notice_job = Some(std::thread::spawn(move || load_notice_art(&source)));
    }
    if net.notice_job.as_ref().is_some_and(|j| j.is_finished())
        && let Some(job) = net.notice_job.take()
    {
        let art = job
            .join()
            .unwrap_or_else(|_| Err("the reading thread stopped".into()));
        if let Err(e) = &art {
            game.event(serde_json::json!({"event":"duel_notice_art","error":e}));
        }
        net.notice_art = Some(art);
    }
    let Some(Ok(art)) = &net.notice_art else {
        return;
    };
    if net.notice.as_ref().map(|(t, _)| t) != net.notice_want.as_ref()
        && let Some((_, mut w)) = net.notice.take()
    {
        w.dismiss();
        net.notices_closing.push(w);
    }
    if net.notice.is_none()
        && net.notices_closing.is_empty()
        && let Some(text) = net.notice_want.clone()
    {
        let w = visual::dialogue::TalkWindow::notice(
            &art.sheet.sequence,
            notice_text(&text),
            art.colour,
            &art.font.widths,
        );
        let tick = game.tick;
        game.event(serde_json::json!({"event":"duel_notice","tick":tick,"text":text}));
        net.notice = Some((text, w));
    }
    if let Some((_, w)) = net.notice.as_mut() {
        w.step(false);
    }
    for w in &mut net.notices_closing {
        w.step(false);
    }
    net.notices_closing.retain(|w| !w.deleted());
}

/// How long the host shows why its player left before waiting again.
const BANNER_TICKS: u64 = 5 * 60;

/// The joiner's session ended: its Sora is its own again (the field
/// simulates him) and the room pauses as usual.
fn leave_session(game: &mut Game, loadout: &mut duel_loadout::DuelLoadout) {
    game.replica = false;
    game.ext.online_mut().net_pad = rival::RivalPad::default();
    game.versus = false;
    game.rivals.clear();
    duel_loadout::restore(game, loadout);
}

/// End of every fixed step: what this tick queued goes out, and what the
/// logs and the replay keep of it.
pub fn end(net: Option<ResMut<DuelNet>>, mut game: ResMut<Game>) {
    if let Some(mut net) = net {
        net.session.update();
        duel_log::tick(&mut net, &game);
        duel_replay::record(&mut net, &mut game);
        duel_predict::show(&mut game);
    }
}

/// Leaves cleanly when the room closes (the other side hears Bye at once).
pub fn close_on_exit(mut exits: MessageReader<AppExit>, net: Option<ResMut<DuelNet>>) {
    if exits.read().next().is_some()
        && let Some(mut net) = net
    {
        net.session.close();
    }
}

/// Rebuilds the overlay lines a few times a second.
pub fn sample(net: Option<ResMut<DuelNet>>, mut game: ResMut<Game>) {
    let Some(mut net) = net else { return };
    // The link's numbers in the log every 600 frames (the duel's
    // measurements).
    net.stats_frames += 1;
    if net.stats_frames >= 600 && net.session.is_connected() {
        net.stats_frames = 0;
        let s = net.session.stats();
        let tick = game.tick;
        let route = net.session.route().label();
        game.event(
            serde_json::json!({"event":"net_stats","tick":tick,"rtt_ms":s.rtt_ms,
            "loss":s.packet_loss,"send_kbps":s.sent_bytes_per_sec / 1024.,
            "recv_kbps":s.received_bytes_per_sec / 1024.,"route":route}),
        );
        // For the launcher, which shows it when the duel is over.
        println!("KHC link: {route} {:.0} ms", s.rtt_ms);
    }
    if net.overlay != game.ext.online().net_overlay {
        net.overlay = game.ext.online().net_overlay;
    }
    // F3 > Sora > Display > Simulated network.
    if let Some(preset) = game.ext.online().net_simulated
        && net.preset != Some(preset)
        && let Some(&(name, conditions)) = SIMULATED.get(preset)
    {
        net.preset = Some(preset);
        net.session.set_conditions(conditions);
        net.warn(format!("Simulated network: {name}"));
    }
    net.since_refresh += 1;
    if net.since_refresh < 15 && !net.lines.is_empty() {
        return;
    }
    net.since_refresh = 0;
    let s = net.session.stats();
    let c = net.session.conditions();
    let role = match net.role {
        Role::Host => "Host",
        Role::Join => "Joined",
    };
    let mut lines = vec![(
        format!(
            "{role}: {} vs {}",
            net.name,
            net.peer.as_deref().unwrap_or("-")
        ),
        false,
    )];
    if net.session.is_connected() {
        lines.push((
            format!(
                "{} {:.0} ms   loss {:.1}%",
                net.session.route().label(),
                s.rtt_ms,
                s.packet_loss * 100.
            ),
            s.packet_loss > 0.05,
        ));
        lines.push((
            format!(
                "Send {:.1} KB/s   recv {:.1} KB/s",
                s.sent_bytes_per_sec / 1024.,
                s.received_bytes_per_sec / 1024.
            ),
            false,
        ));
        match net.role {
            Role::Host => lines.push((
                format!("Buttons queued {}   late {}", net.inputs.len(), net.starved),
                net.starved > 3,
            )),
            Role::Join => {
                let age = net
                    .shown_snapshot
                    .zip(net.snapshots.last_key_value().map(|(k, _)| *k))
                    .map_or(0, |(shown, newest)| newest.saturating_sub(shown));
                lines.push((
                    format!(
                        "Snapshots buffered {}   behind {age}   stale {}",
                        net.snapshots.len(),
                        net.stale
                    ),
                    net.stale > 3,
                ));
                lines.push((
                    format!(
                        "Playout hold {:.0} ms   speed {:+.0}%",
                        net.playout.hold_ticks() * 1000. / 60.,
                        (net.playout.speed() - 1.) * 100.
                    ),
                    false,
                ));
            }
        }
    }
    if game.ext.online().predict.on {
        let (count, largest) = game.ext.online().predict.corrections.last_minute(game.tick);
        lines.push((
            format!("Corrections {count} a minute, largest {largest:.0}"),
            false,
        ));
    }
    if game.ext.online().duel_rules.latency_match {
        let ticks = match net.role {
            Role::Host => game.ext.online().own_delay.ticks,
            Role::Join => net.host_delay_ticks,
        };
        lines.push((format!("Host input delay: {ticks} ticks"), false));
    }
    if let Some(average) = game.ext.online().rewind.average() {
        lines.push((format!("Rewind average {average:.1} ticks"), false));
    }
    if !c.is_clear() {
        lines.push((
            format!(
                "Simulated +{} ms, jitter {} ms, {:.0}% lost, {} dropped",
                c.latency_ms, c.jitter_ms, c.loss_percent, s.simulated_drops
            ),
            true,
        ));
    }
    let warnings: Vec<String> = net.warnings.iter().cloned().collect();
    lines.extend(warnings.into_iter().map(|w| (w, true)));
    lines.push((
        if game.ext.online().duel_rules.all_kh3() {
            format!("KH3's damage, level 99   tick {}", game.tick)
        } else {
            format!(
                "Damage x{}  magic x{}  cap {}   tick {}",
                game.ext.online().duel_rules.damage,
                game.ext.online().duel_rules.magic,
                cap_text(game.ext.online().duel_rules.hit_cap),
                game.tick
            )
        },
        false,
    ));
    net.starved = 0;
    net.stale = 0;
    net.lines = lines;
}

/// Adds a late or skipped-step warning to the overlay.
pub fn warn(net: &mut DuelNet, text: String) {
    net.warn(text);
}

fn sheets(
    net: &mut DuelNet,
    frame: &mut Ui2dFrame,
    images: &mut Assets<Image>,
    font: &SystemFont,
) -> (usize, usize) {
    *net.sheets.get_or_insert_with(|| {
        (
            frame.add_sheet(
                images,
                font.rgba.clone(),
                font.size,
                font.texel_scale as f32,
            ),
            frame.add_sheet(images, vec![255; 16], [2, 2], 1.),
        )
    })
}

fn backing(white: usize, rect: [f32; 4]) -> (usize, Vec<data::sprite_sequence::Quad>) {
    faded_backing(white, rect, 1.)
}

/// A rectangle in one colour.
fn tinted(rect: [f32; 4], c: [f32; 4]) -> data::sprite_sequence::Quad {
    let [x0, y0, x1, y1] = rect;
    data::sprite_sequence::Quad {
        corners: [[x0, y0], [x1, y0], [x0, y1], [x1, y1]],
        uv: [0, 0, 1, 1],
        colors: [c; 4],
        blend: 0,
    }
}

/// Past every edge of the 2D layer (the flashes).
const SCREEN: [f32; 4] = [-512., -256., 1024., 768.];
/// The tie's coin: where, how large, its rim.
const COIN_CENTRE: [f32; 2] = [256., 150.];
const COIN_RADIUS: f32 = 26.;
const COIN_RIM: [f32; 4] = [1.1, 0.8, 0.3, 1.];

/// A disc `width` times as wide as it is tall, in horizontal strips.
fn disc(centre: [f32; 2], r: f32, width: f32, c: [f32; 4]) -> Vec<data::sprite_sequence::Quad> {
    const STRIPS: usize = 20;
    let half = |y: f32| r * width * (1. - (y / r).powi(2)).max(0.).sqrt();
    (0..STRIPS)
        .map(|i| {
            let y0 = -r + 2. * r * i as f32 / STRIPS as f32;
            let y1 = -r + 2. * r * (i + 1) as f32 / STRIPS as f32;
            let [cx, cy] = centre;
            data::sprite_sequence::Quad {
                corners: [
                    [cx - half(y0), cy + y0],
                    [cx + half(y0), cy + y0],
                    [cx - half(y1), cy + y1],
                    [cx + half(y1), cy + y1],
                ],
                uv: [0, 0, 1, 1],
                colors: [c; 4],
                blend: 0,
            }
        })
        .collect()
}

/// The backing at an opacity (the lobby's fade).
fn faded_backing(
    white: usize,
    rect: [f32; 4],
    alpha: f32,
) -> (usize, Vec<data::sprite_sequence::Quad>) {
    let [x0, y0, x1, y1] = rect;
    let c = [BACKING[0], BACKING[1], BACKING[2], BACKING[3] * alpha];
    (
        white,
        vec![data::sprite_sequence::Quad {
            corners: [[x0, y0], [x1, y0], [x0, y1], [x1, y1]],
            uv: [0, 0, 1, 1],
            colors: [c; 4],
            blend: 0,
        }],
    )
}

/// A hit cap as the lobby and the overlay show it.
pub fn cap_text(cap: u8) -> String {
    if cap >= 100 {
        "Off".into()
    } else {
        format!("{cap}%")
    }
}

/// The overlay at the top right and the session banner in the middle, on
/// the 2D layer.
#[allow(clippy::too_many_arguments)]
pub fn present(
    net: Option<ResMut<DuelNet>>,
    vote: Option<Res<duel_vote::VoteRoom>>,
    duel_match: Option<Res<duel_match_room::MatchRoom>>,
    camp: Option<Res<CampRes>>,
    game: Res<Game>,
    hud: Option<Res<Hud>>,
    fixed: Res<Time<Fixed>>,
    mut frame: ResMut<Ui2dFrame>,
    mut images: ResMut<Assets<Image>>,
    mut typed: MessageReader<KeyboardInput>,
) {
    let Some(mut net) = net else {
        typed.clear();
        return;
    };
    type_reply_code(&mut net, &mut typed);
    // The part of a fixed tick since the last: the cosmetics below follow
    // the tick, smoothly between ticks.
    let frac = fixed.overstep_fraction().clamp(0., 1.);
    let Some(font) = hud.as_ref().and_then(|h| h.menu.as_ref()).map(|m| &m.font) else {
        return;
    };
    let (sheet, white) = sheets(&mut net, &mut frame, &mut images, font);
    let f = field_command_menu::Font {
        widths: &font.widths,
        pad: 1,
    };
    let banner = match &net.status {
        Status::Waiting(_) if net.room_code.is_some() => Some(format!(
            "Room code {}\n{}",
            net.room_code.as_deref().unwrap_or_default(),
            reply_line(&net)
        )),
        Status::Waiting(port) if net.no_room_code.is_some() => Some(format!(
            "No room code: {}\nOn this network: UDP port {port}",
            net.no_room_code.as_deref().unwrap_or_default()
        )),
        Status::Waiting(port) if net.relay_silent => Some(format!(
            "The relay does not answer.\nOn this network: UDP port {port}"
        )),
        Status::Waiting(port) => Some(format!("Waiting for a player to join.\nUDP port {port}")),
        Status::Joining(host) => Some(format!("Joining {host}...")),
        Status::JoiningRoom(code) => Some(match &net.reply_code {
            Some(reply) => format!("Joining room {code}...\nNot in? Send your friend {reply}"),
            None => format!("Joining room {code}..."),
        }),
        Status::Playing => waiting_text(&net),
        Status::Ended(reason, _) => Some(reason.clone()),
    };
    // A match's result says who left, and an open menu covers the field:
    // the status gives way to them.
    let result_up = duel_match.as_ref().is_some_and(|m| m.menu_up());
    let menu_open = camp.as_ref().is_some_and(|c| c.open.is_some());
    let banner = banner.filter(|_| !result_up && !menu_open);
    net.notice_want = banner.clone();
    // The game's message balloon; without its art, a plain line.
    if net.notice_sheets.is_none()
        && let Some(Ok(art)) = net.notice_art.as_mut()
    {
        let window = frame.add_sheet(
            &mut images,
            std::mem::take(&mut art.sheet.texture_rgba),
            art.sheet.texture_size,
            art.sheet.texel_scale as f32,
        );
        let evt = frame.add_sheet(
            &mut images,
            std::mem::take(&mut art.font.rgba),
            art.font.size,
            art.font.texel_scale as f32,
        );
        net.notice_sheets = Some((window, evt));
    }
    if let Some((window, evt)) = net.notice_sheets {
        for w in net
            .notices_closing
            .iter()
            .chain(net.notice.as_ref().map(|(_, w)| w))
        {
            for d in w.draw(window, evt) {
                if let visual::gauge::Draw::Quads { quads, sheet, .. } = d {
                    frame.draws.push((sheet, quads));
                }
            }
        }
    }
    let plain = matches!(net.notice_art, Some(Err(_)));
    if let Some(text) = banner.filter(|_| plain) {
        let encoded = pause_menu::encode_text(&text.replace('\n', "  "));
        let width = field_command_menu::text_width(&encoded, &f, BANNER_SIZE);
        let x = BANNER_CENTRE - width / 2.;
        frame.draws.push(backing(
            white,
            [
                x - 8.,
                BANNER_Y - 4.,
                x + width + 8.,
                BANNER_Y + BANNER_SIZE + 6.,
            ],
        ));
        frame.draws.push((
            sheet,
            field_command_menu::text_quads_sized(&encoded, [x, BANNER_Y], TEXT, &f, BANNER_SIZE),
        ));
    }
    // The match: countdown, clock, rounds and the result, centred (with the
    // pause menu's art the result's choices and a vote are in the menu).
    let in_camp = camp.is_some();
    // The match's text is over the field, and over the result's own top
    // level; any other menu covers it.
    let covered = camp
        .as_ref()
        .and_then(|c| c.open.as_ref())
        .is_some_and(|m| {
            let result = m
                .data
                .duel
                .as_ref()
                .is_some_and(|d| d.owner.0 == duel_camp::RESULT);
            !(result && m.state == pause_menu::State::Top)
        });
    if let Some(c) = duel_match
        .as_ref()
        .filter(|_| !covered)
        .and_then(|m| duel_match_room::flash(m, &net, &game, frac))
    {
        frame.draws.push((white, vec![tinted(SCREEN, c)]));
    }
    // The game's sprites of the match (the Struggle's sheets, registered
    // once they are read), then the text that stays.
    let art = duel_match
        .as_ref()
        .and_then(|m| m.art.as_ref())
        .and_then(|a| a.as_ref().ok());
    if net.match_sheets.is_none()
        && let Some(a) = art
    {
        let mut add = |s: &Sheet| {
            frame.add_sheet(
                &mut images,
                s.texture_rgba.clone(),
                s.texture_size,
                s.texel_scale as f32,
            )
        };
        net.match_sheets = Some([add(&a.timer), add(&a.start), add(&a.end)]);
    }
    let drawn = net.match_sheets.filter(|_| art.is_some());
    if let (Some(ids), Some(m)) = (drawn, duel_match.as_ref().filter(|_| !covered)) {
        for (sheet, quads) in duel_match_room::sprites(m, &net, &game, in_camp) {
            frame.draws.push((ids[sheet as usize], quads));
        }
    }
    let labels = duel_match
        .filter(|_| !covered)
        .map(|m| duel_match_room::labels(&m, &net, &game, in_camp, drawn.is_some()))
        .unwrap_or_default();
    for l in &labels {
        let width = field_command_menu::text_width(&l.text, &f, l.size);
        let x = l.centre[0] - width / 2.;
        let y = l.centre[1] - l.size / 2.;
        if l.backed {
            frame.draws.push(backing(
                white,
                [x - 8., y - 3., x + width + 8., y + l.size + 3.],
            ));
        } else {
            // Over the field: a soft shadow keeps it readable on a flash.
            let d = (l.size / 16.).max(1.);
            let shade = [0., 0., 0., SHADOW * l.colour[3]];
            frame.draws.push((
                sheet,
                field_command_menu::text_quads_sized(&l.text, [x + d, y + d], shade, &f, l.size),
            ));
        }
        frame.draws.push((
            sheet,
            field_command_menu::text_quads_sized(&l.text, [x, y], l.colour, &f, l.size),
        ));
    }
    // A tie's coin, over the vote.
    if let Some((width, size)) = vote.as_ref().and_then(|v| duel_vote::coin(v, &game, frac)) {
        let r = COIN_RADIUS * size;
        frame
            .draws
            .push((white, disc(COIN_CENTRE, r, width, COIN_RIM)));
        frame
            .draws
            .push((white, disc(COIN_CENTRE, r * 0.82, width, CURSOR)));
    }
    // A vote without the pause menu's art: its own panel.
    let vote_lines: Vec<(Vec<u8>, bool)> = vote
        .filter(|_| !in_camp)
        .map(|v| duel_vote::lines(&v, &net, &game))
        .unwrap_or_default()
        .into_iter()
        .map(|(l, on)| (pause_menu::encode_text(&l), on))
        .collect();
    if !vote_lines.is_empty() {
        let width = vote_lines
            .iter()
            .map(|(l, _)| field_command_menu::text_width(l, &f, LOBBY_SIZE))
            .fold(0., f32::max);
        let x = BANNER_CENTRE - width / 2.;
        frame.draws.push(backing(
            white,
            [
                x - 10.,
                VOTE_TOP - 6.,
                x + width + 10.,
                VOTE_TOP + LOBBY_LINE * vote_lines.len() as f32 + 4.,
            ],
        ));
        let mut quads = Vec::new();
        for (n, (line, on)) in vote_lines.iter().enumerate() {
            quads.extend(field_command_menu::text_quads_sized(
                line,
                [x, VOTE_TOP + n as f32 * LOBBY_LINE],
                if *on { CURSOR } else { TEXT },
                &f,
                LOBBY_SIZE,
            ));
        }
        frame.draws.push((sheet, quads));
    }
    // The network overlay is for the field: menus keep their own corner.
    if !net.overlay || net.lines.is_empty() || menu_open {
        return;
    }
    let encoded: Vec<(Vec<u8>, bool)> = net
        .lines
        .iter()
        .map(|(l, warn)| (pause_menu::encode_text(l), *warn))
        .collect();
    let width = encoded
        .iter()
        .map(|(l, _)| field_command_menu::text_width(l, &f, SIZE))
        .fold(0., f32::max);
    let x = RIGHT - width;
    // Below the status balloon while it shows.
    let top = net.notice.as_ref().map_or(TOP, |(_, w)| {
        TOP.max((BALLOON_TOP + w.size().1 + BALLOON_PAD) as f32)
    });
    frame.draws.push(backing(
        white,
        [
            x - 4.,
            top - 2.,
            RIGHT + 4.,
            top + LINE * encoded.len() as f32 + 2.,
        ],
    ));
    let mut quads = Vec::new();
    for (n, (line, warn)) in encoded.iter().enumerate() {
        let colour = if *warn { WARN } else { TEXT };
        quads.extend(field_command_menu::text_quads_sized(
            line,
            [x, top + n as f32 * LINE],
            colour,
            &f,
            SIZE,
        ));
    }
    frame.draws.push((sheet, quads));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_departure_names_the_player_once() {
        assert_eq!(gone("Riku", "the other player left"), "Riku left the duel.");
        assert_eq!(
            gone("Riku", "the host timed out"),
            "Riku stopped answering."
        );
        assert_eq!(gone("Riku", "no answer"), "Riku left: no answer.");
        assert_eq!(
            gone("Riku", net::session::STOPPED_REASON),
            "Opponent's game stopped on an error."
        );
        let t = notice_text("Waiting.\nUDP port 1");
        assert_eq!(t.iter().filter(|&&b| b == 0x02).count(), 1);
        assert_eq!(t.last(), Some(&0));
    }

    #[test]
    fn addresses_take_the_default_port() {
        assert_eq!(
            parse_address("127.0.0.1").unwrap(),
            SocketAddr::from(([127, 0, 0, 1], net::DEFAULT_PORT))
        );
        assert_eq!(
            parse_address(" 192.168.1.20:5000 ").unwrap(),
            SocketAddr::from(([192, 168, 1, 20], 5000))
        );
        assert!(parse_address("not an address at all:x").is_err());
    }

    #[test]
    fn link_conditions_parse() {
        assert_eq!(
            parse_conditions("80, 20, 5").unwrap(),
            Conditions {
                latency_ms: 80,
                jitter_ms: 20,
                loss_percent: 5.
            }
        );
        assert!(parse_conditions("80,20").is_err());
        assert!(parse_conditions("80,20,150").is_err());
    }
}
