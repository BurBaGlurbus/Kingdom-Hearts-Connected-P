//! Replays (KHC-023): a `.khcr` file holds the messages a joiner gets, with
//! the host's tick, and playing one feeds them to the joiner's own code at 60
//! Hz from a session with no socket (`--replay <file>`). Nothing is simulated
//! again. What it draws comes from the player's install, so a file names the
//! installs it needs.
//!
//! The file: a header, then records of tick, channel and a length-prefixed
//! message. Messages before the first match (the lobby's) are reliable ones
//! only; a replay gives them all at once and starts at the first snapshot.

use super::duel_net::DuelNet;
use super::*;
use net::wire::{Reader, Writer};
use net::{RELIABLE, Role, UNRELIABLE};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

const MAGIC: [u8; 4] = *b"KHCR";
/// Bumped when the file's own layout changes.
const FORMAT: u16 = 1;
pub const EXTENSION: &str = "khcr";
/// Ticks a replay waits at its start for the lobby's messages to settle.
const WARM_TICKS: u32 = 120;
/// Ticks between flushes while recording.
const FLUSH_EVERY: u64 = 60;
/// How long an unattended replay runs past its last message.
const END_TICKS: u32 = 60;
/// The speeds: 1, 2 and 3 on the keyboard.
const SPEEDS: [f32; 3] = [0.5, 1., 2.];

#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub protocol: u16,
    /// The executable that saved it (a warning if it is not this one).
    pub exe: [u8; 32],
    pub host_name: String,
    pub join_name: String,
    /// `duel_wire::encode_rules`'s bytes.
    pub rules: Vec<u8>,
    /// The KH3 install, when a Sora fights with it.
    pub kh3: Option<[u8; 32]>,
}

impl Header {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(&MAGIC);
        w.u16(FORMAT);
        w.u16(self.protocol);
        w.bytes(&self.exe);
        w.text(&self.host_name, net::wire::NAME_LIMIT);
        w.text(&self.join_name, net::wire::NAME_LIMIT);
        w.u32(self.rules.len() as u32);
        w.bytes(&self.rules);
        w.u8(u8::from(self.kh3.is_some()));
        w.bytes(&self.kh3.unwrap_or([0; 32]));
        w.finish()
    }

    fn decode(r: &mut Reader) -> Result<Header, String> {
        let cut = || "the file is cut short".to_owned();
        if r.take(4).ok_or_else(cut)? != MAGIC {
            return Err("this is not a replay file".into());
        }
        let format = r.u16().ok_or_else(cut)?;
        if format != FORMAT {
            return Err(format!(
                "this replay's file layout is {format}, and this game reads {FORMAT}"
            ));
        }
        let protocol = r.u16().ok_or_else(cut)?;
        let exe = r.array().ok_or_else(cut)?;
        let host_name = r.text(net::wire::NAME_LIMIT).ok_or_else(cut)?;
        let join_name = r.text(net::wire::NAME_LIMIT).ok_or_else(cut)?;
        let n = r.u32().ok_or_else(cut)? as usize;
        let rules = r.take(n).ok_or_else(cut)?.to_vec();
        let has_kh3 = r.u8().ok_or_else(cut)? != 0;
        let kh3: [u8; 32] = r.array().ok_or_else(cut)?;
        Ok(Header {
            protocol,
            exe,
            host_name,
            join_name,
            rules,
            kh3: has_kh3.then_some(kh3),
        })
    }
}

/// One message of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub tick: u64,
    pub channel: u8,
    pub message: Vec<u8>,
}

fn encode_record(tick: u64, channel: u8, message: &[u8]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u32(tick as u32);
    w.u8(channel);
    w.u32(message.len() as u32);
    w.bytes(message);
    w.finish()
}

/// A whole file.
#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub header: Header,
    pub records: Vec<Record>,
}

impl Replay {
    /// A file cut short by a crash keeps the records before the cut.
    pub fn parse(bytes: &[u8]) -> Result<Replay, String> {
        let mut r = Reader::new(bytes);
        let header = Header::decode(&mut r)?;
        let mut records = Vec::new();
        while let (Some(tick), Some(channel), Some(n)) = (r.u32(), r.u8(), r.u32()) {
            let Some(message) = r.take(n as usize) else {
                break;
            };
            records.push(Record {
                tick: u64::from(tick),
                channel,
                message: message.to_vec(),
            });
        }
        Ok(Replay { header, records })
    }

    pub fn read(path: &Path) -> Result<Replay, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Replay::parse(&bytes)
    }
}

/// Whether this game can play a file: Err says why not; Ok lists warnings.
pub fn check(
    header: &Header,
    protocol: u16,
    kh3: Option<[u8; 32]>,
    exe: Option<[u8; 32]>,
) -> Result<Vec<String>, String> {
    if header.protocol != protocol {
        return Err(format!(
            "this replay was saved with duel protocol {}, and this game speaks {protocol}",
            header.protocol
        ));
    }
    if let Some(wanted) = header.kh3 {
        match kh3 {
            None => return Err("this replay needs KINGDOM HEARTS III, which is not set".into()),
            Some(own) if own != wanted => {
                return Err(
                    "this replay was saved with another KINGDOM HEARTS III install: \
                     its effects and moves would not match"
                        .into(),
                );
            }
            Some(_) => {}
        }
    }
    let mut warnings = Vec::new();
    if exe.is_some_and(|own| own != header.exe) {
        warnings.push("Saved by another build of this game: it may not look the same.".to_owned());
    }
    Ok(warnings)
}

/// Taking a session's messages down, from the first match on.
pub struct Recorder {
    path: PathBuf,
    pending: Vec<Record>,
    file: Option<BufWriter<File>>,
}

impl Recorder {
    pub fn new(path: PathBuf) -> Recorder {
        Recorder {
            path,
            pending: Vec::new(),
            file: None,
        }
    }
}

/// Where a session's replay goes without `--replay-out`.
pub fn folder() -> PathBuf {
    crate::launcher::data_dir().join("replays")
}

pub fn auto_path() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    folder().join(format!("duel-{stamp}.{EXTENSION}"))
}

fn open_file(path: &Path, header: &Header) -> Result<BufWriter<File>, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BufWriter::new(file);
    out.write_all(&header.encode())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(out)
}

/// End of a fixed tick: what the session tapped goes in the file. A failure
/// gives up on the replay, not the duel.
pub fn record(net: &mut DuelNet, game: &mut Game) {
    if net.replay.is_none() {
        return;
    }
    let tapped = net.session.take_tapped();
    let clock = match net.role {
        Role::Host => game.tick,
        Role::Join => net
            .snapshots
            .last_key_value()
            .map(|(t, _)| *t)
            .or(net.shown_snapshot)
            .unwrap_or(0),
    };
    let tick = game.tick;
    let header = || {
        let o = game.ext.online();
        let (host_name, join_name) = {
            let peer = net.peer.clone().unwrap_or_default();
            match net.role {
                Role::Host => (net.name.clone(), peer),
                Role::Join => (peer, net.name.clone()),
            }
        };
        Header {
            protocol: net::wire::PROTOCOL,
            exe: net.build,
            host_name,
            join_name,
            rules: duel_wire::encode_rules(&o.duel_rules),
            kh3: o
                .duel_rules
                .kh3()
                .then(|| o.kh3_install.as_ref().map(|i| i.identity))
                .flatten(),
        }
    };
    let opening =
        game.ext.online().duel_match && net.replay.as_ref().is_some_and(|r| r.file.is_none());
    let header = opening.then(header);
    let mut failure = None;
    if let Some(r) = net.replay.as_mut() {
        if let Some(header) = header {
            match open_file(&r.path, &header) {
                Ok(mut out) => {
                    for p in std::mem::take(&mut r.pending) {
                        if let Err(e) = out.write_all(&encode_record(p.tick, p.channel, &p.message))
                        {
                            failure = Some(e.to_string());
                        }
                    }
                    r.file = Some(out);
                }
                Err(e) => failure = Some(e),
            }
        }
        for (channel, message) in tapped {
            match r.file.as_mut() {
                Some(out) => {
                    if let Err(e) = out.write_all(&encode_record(clock, channel, &message)) {
                        failure = Some(e.to_string());
                    }
                }
                // Before the first match: the lobby's, not the field's.
                None if channel == RELIABLE => r.pending.push(Record {
                    tick: clock,
                    channel,
                    message,
                }),
                None => {}
            }
        }
        if tick.is_multiple_of(FLUSH_EVERY)
            && let Some(out) = r.file.as_mut()
            && let Err(e) = out.flush()
        {
            failure = Some(e.to_string());
        }
    }
    if let Some(error) = failure {
        game.event(serde_json::json!({"event":"duel_replay_failed","tick":tick,"error":error}));
        net.replay = None;
        net.session.set_tap(false);
    }
}

/// Playing a file.
#[derive(Resource)]
pub struct ReplayPlayer {
    replay: Replay,
    next: usize,
    /// The host's tick reached.
    at: u64,
    /// Parts of a tick from a slow speed.
    carry: f32,
    speed: usize,
    paused: bool,
    /// Single steps asked for while paused.
    steps: u32,
    warm: u32,
    /// The lobby's messages have gone in.
    begun: bool,
    /// Ticks since the last message went in.
    ended: Option<u32>,
    warnings: Vec<String>,
}

impl ReplayPlayer {
    pub fn new(replay: Replay, warnings: Vec<String>) -> ReplayPlayer {
        ReplayPlayer {
            replay,
            next: 0,
            at: 0,
            carry: 0.,
            speed: 1,
            paused: false,
            steps: 0,
            warm: 0,
            begun: false,
            ended: None,
            warnings,
        }
    }

    /// What this many more ticks of play release: every message, as a snapshot
    /// may start from the one before. Of those a tick the joiner's code shows the
    /// newest, so its buffer never overflows at a fast speed.
    fn release(&mut self, ticks: u64) -> Vec<Record> {
        self.at += ticks;
        let records = &self.replay.records;
        let mut out: Vec<Record> = Vec::new();
        while let Some(r) = records.get(self.next).filter(|r| r.tick <= self.at) {
            out.push(r.clone());
            self.next += 1;
        }
        out
    }

    /// The whole lobby: the reliable messages before the first snapshot.
    fn begin(&mut self) -> Vec<Record> {
        let records = &self.replay.records;
        let first = records
            .iter()
            .position(|r| r.channel == UNRELIABLE)
            .unwrap_or(records.len());
        self.next = first;
        self.at = records.get(first).map_or(0, |r| r.tick.saturating_sub(1));
        self.begun = true;
        records[..first].to_vec()
    }

    /// How many ticks of play this fixed tick is.
    fn advance(&mut self) -> u64 {
        if self.paused {
            return u64::from(std::mem::take(&mut self.steps));
        }
        self.carry += SPEEDS[self.speed];
        let whole = self.carry.floor();
        self.carry -= whole;
        whole as u64
    }
}

/// Pause, speed and single step, each drawn frame: Space, 1 2 3, and the full
/// stop key.
pub fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    player: Option<ResMut<ReplayPlayer>>,
    net: Option<ResMut<DuelNet>>,
) {
    let (Some(mut player), Some(mut net)) = (player, net) else {
        return;
    };
    if keys.just_pressed(KeyCode::Space) {
        player.paused = !player.paused;
        let text = if player.paused {
            "Replay paused: Space goes on, the full stop steps one tick."
        } else {
            "Replay playing."
        };
        duel_net::warn(&mut net, text.to_owned());
    }
    if keys.just_pressed(KeyCode::Period) && player.paused {
        player.steps += 1;
    }
    for (key, speed) in [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
    ] {
        if keys.just_pressed(key) {
            player.speed = speed;
            duel_net::warn(&mut net, format!("Replay speed x{}", SPEEDS[speed]));
        }
    }
}

/// Before the session's tick: what this tick releases goes to the session.
pub fn feed(
    player: Option<ResMut<ReplayPlayer>>,
    net: Option<ResMut<DuelNet>>,
    mut exits: MessageWriter<AppExit>,
    mut game: ResMut<Game>,
) {
    let (Some(mut player), Some(mut net)) = (player, net) else {
        return;
    };
    for w in std::mem::take(&mut player.warnings) {
        duel_net::warn(&mut net, w);
    }
    if !player.begun {
        for r in player.begin() {
            net.session.feed(r.channel, r.message);
        }
        return;
    }
    // The room reads what it needs first, as a joiner's does before a match.
    let o = game.ext.online();
    let loading = o.kh3_loading || (o.duel_rules.kh3() && o.kh3_book.is_none());
    if player.warm < WARM_TICKS || loading {
        player.warm += 1;
        return;
    }
    let ticks = player.advance();
    for r in player.release(ticks) {
        net.session.feed(r.channel, r.message);
    }
    if player.next >= player.replay.records.len() {
        let waited = player.ended.map_or(0, |n| n + 1);
        if player.ended.is_none() {
            let tick = game.tick;
            game.event(serde_json::json!({"event":"duel_replay_end","tick":tick}));
            duel_net::warn(&mut net, "The replay has ended. Esc leaves.".to_owned());
        }
        player.ended = Some(waited);
        if game.autoplay.is_some() && waited >= END_TICKS {
            exits.write(AppExit::Success);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            protocol: net::wire::PROTOCOL,
            exe: [3; 32],
            host_name: "Roxas".into(),
            join_name: "Xion".into(),
            rules: vec![4, 5, 6],
            kh3: Some([9; 32]),
        }
    }

    fn file(records: &[(u64, u8, &[u8])]) -> Vec<u8> {
        let mut bytes = header().encode();
        for (tick, channel, message) in records {
            bytes.extend(encode_record(*tick, *channel, message));
        }
        bytes
    }

    #[test]
    fn a_file_round_trips() {
        let bytes = file(&[(1000, RELIABLE, b"lobby"), (1001, UNRELIABLE, b"snapshot")]);
        let replay = Replay::parse(&bytes).unwrap();
        assert_eq!(replay.header, header());
        assert_eq!(
            replay.records,
            vec![
                Record {
                    tick: 1000,
                    channel: RELIABLE,
                    message: b"lobby".to_vec()
                },
                Record {
                    tick: 1001,
                    channel: UNRELIABLE,
                    message: b"snapshot".to_vec()
                },
            ]
        );
        let mut no_kh3 = header();
        no_kh3.kh3 = None;
        let replay = Replay::parse(&no_kh3.encode()).unwrap();
        assert_eq!(replay.header.kh3, None);
    }

    #[test]
    fn a_cut_file_keeps_what_came_before_the_cut() {
        let bytes = file(&[(1, RELIABLE, b"a"), (2, UNRELIABLE, b"bcd")]);
        let replay = Replay::parse(&bytes[..bytes.len() - 2]).unwrap();
        assert_eq!(replay.records.len(), 1);
        assert!(Replay::parse(&bytes[..10]).is_err());
        assert!(Replay::parse(b"not a replay at all").is_err());
    }

    #[test]
    fn another_protocol_or_install_is_refused_with_the_reason() {
        let h = header();
        let own = Some([9; 32]);
        let exe = Some([3; 32]);
        assert_eq!(check(&h, h.protocol, own, exe), Ok(Vec::new()));
        let why = check(&h, h.protocol + 1, own, exe).unwrap_err();
        assert!(why.contains("protocol"), "{why}");
        let why = check(&h, h.protocol, Some([8; 32]), exe).unwrap_err();
        assert!(why.contains("another KINGDOM HEARTS III"), "{why}");
        let why = check(&h, h.protocol, None, exe).unwrap_err();
        assert!(why.contains("needs KINGDOM HEARTS III"), "{why}");
        // A KH2 match needs no KH3 at all.
        let mut kh2 = header();
        kh2.kh3 = None;
        assert_eq!(check(&kh2, h.protocol, None, exe), Ok(Vec::new()));
        // Another build still plays, with a warning.
        assert_eq!(check(&h, h.protocol, own, Some([1; 32])).unwrap().len(), 1);
    }

    fn player(records: &[(u64, u8, &[u8])]) -> ReplayPlayer {
        ReplayPlayer::new(Replay::parse(&file(records)).unwrap(), Vec::new())
    }

    #[test]
    fn the_lobby_goes_in_at_once_and_snapshots_follow_the_clock() {
        let mut p = player(&[
            (10, RELIABLE, b"lobby"),
            (11, RELIABLE, b"match"),
            (100, UNRELIABLE, b"s100"),
            (101, RELIABLE, b"e101"),
            (101, UNRELIABLE, b"s101"),
            (102, UNRELIABLE, b"s102"),
        ]);
        let lobby: Vec<Vec<u8>> = p.begin().into_iter().map(|r| r.message).collect();
        assert_eq!(lobby, [b"lobby".to_vec(), b"match".to_vec()]);
        assert_eq!(p.release(1)[0].message, b"s100");
        let next: Vec<Vec<u8>> = p.release(1).into_iter().map(|r| r.message).collect();
        assert_eq!(next, [b"e101".to_vec(), b"s101".to_vec()]);
        assert!(p.release(0).is_empty());
    }

    #[test]
    fn a_fast_speed_releases_every_message() {
        let mut p = player(&[
            (100, UNRELIABLE, b"s100"),
            (100, RELIABLE, b"e100"),
            (101, UNRELIABLE, b"s101"),
        ]);
        p.begin();
        let out: Vec<Vec<u8>> = p.release(2).into_iter().map(|r| r.message).collect();
        assert_eq!(out, [b"s100".to_vec(), b"e100".to_vec(), b"s101".to_vec()]);
    }

    #[test]
    fn pause_and_speed_set_the_ticks_played() {
        let mut p = player(&[]);
        assert_eq!(p.advance(), 1);
        p.speed = 2;
        assert_eq!(p.advance(), 2);
        p.speed = 0;
        assert_eq!(p.advance() + p.advance(), 1);
        p.paused = true;
        assert_eq!(p.advance(), 0);
        p.steps = 1;
        assert_eq!(p.advance(), 1);
        assert_eq!(p.advance(), 0);
    }
}
