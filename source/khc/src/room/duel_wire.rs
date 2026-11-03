//! The duel's game messages over the session: the joiner's buttons
//! (unreliable, with the last few ticks repeated so a lost datagram costs
//! nothing), the host's snapshots of both Soras (unreliable, newest wins; most
//! say only what changed since one the joiner has), and events that must arrive
//! (reliable: hits to show, knock-outs).
//!
//! Only what drawing and the HUD need is sent; the host alone simulates.

use super::duel_rules::MatchRules;
use super::rival::{Command, RivalPad};
use net::wire::{Reader, Writer};
use player::Action;

pub const INPUT: u8 = 1;
pub const SNAPSHOT: u8 = 2;
pub const EVENTS: u8 = 3;
const RULES: u8 = 4;
/// The lobby (host to joiner), a joiner's lobby intent, and the host's
/// refusal of one (duel_lobby.rs).
pub const LOBBY: u8 = 5;
pub const INTENT: u8 = 6;
pub const REFUSED: u8 = 7;
/// A vote (host to joiner) and a joiner's ballot (duel_vote.rs).
pub const VOTE: u8 = 8;
pub const BALLOT: u8 = 9;
/// The match (host to joiner) and a joiner's rematch choice
/// (duel_match_room.rs).
pub const MATCH: u8 = 10;
pub const REMATCH: u8 = 11;
/// The host's pick of the match's music (duel_music.rs).
pub const MUSIC: u8 = 12;
/// A joiner's forfeit (duel_match_room.rs).
pub const FORFEIT: u8 = 13;
/// What a player's install holds, either way (duel_net.rs).
pub const CAPS: u8 = 14;
/// A snapshot as what changed since one the joiner has (`SNAPSHOT` is one whole).
pub const DELTA: u8 = 15;
/// [KHC-016] A vote's coin: each player's commitment, then their number (duel_vote.rs).
pub const COIN_COMMIT: u8 = 16;
pub const COIN_REVEAL: u8 = 17;

pub fn encode_caps(caps: &net::wire::Capabilities) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(CAPS);
    caps.put(&mut w);
    w.finish()
}

pub fn decode_caps(bytes: &[u8]) -> Option<net::wire::Capabilities> {
    let mut r = Reader::new(bytes);
    if r.u8()? != CAPS {
        return None;
    }
    let caps = net::wire::Capabilities::get(&mut r)?;
    r.is_empty().then_some(caps)
}

/// Ticks of buttons each input message repeats.
pub const INPUT_REDUNDANCY: usize = 6;

pub fn action_code(action: Action) -> (u8, u16) {
    let arg = |i: usize| u16::try_from(i).unwrap_or(u16::MAX);
    match action {
        Action::Idle => (0, 0),
        Action::Walk => (1, 0),
        Action::Run => (2, 0),
        Action::Jump => (3, 0),
        Action::Fall => (4, 0),
        Action::Land => (5, 0),
        Action::Attack(i) => (6, arg(i)),
        Action::AirAttack(i) => (7, arg(i)),
        Action::Cast(i) => (8, arg(i)),
        Action::AirCast(i) => (9, arg(i)),
        Action::Hurt(i) => (10, arg(i)),
        Action::Guard => (11, 0),
        Action::GuardRecoil => (12, 0),
        Action::WallRecoil => (13, 0),
        Action::Item => (14, 0),
        Action::Drive => (15, 0),
        Action::DriveIn => (16, 0),
        Action::HighJump => (17, 0),
        Action::Growth(m) => (18, m),
        Action::Scripted(m) => (19, m),
        Action::StanceSwitch => (20, 0),
        Action::LoweredWalk => (21, 0),
        Action::LoweredRun => (22, 0),
    }
}

pub fn action_from(code: u8, arg: u16) -> Option<Action> {
    let i = arg as usize;
    Some(match code {
        0 => Action::Idle,
        1 => Action::Walk,
        2 => Action::Run,
        3 => Action::Jump,
        4 => Action::Fall,
        5 => Action::Land,
        6 => Action::Attack(i),
        7 => Action::AirAttack(i),
        8 => Action::Cast(i),
        9 => Action::AirCast(i),
        10 => Action::Hurt(i),
        11 => Action::Guard,
        12 => Action::GuardRecoil,
        13 => Action::WallRecoil,
        14 => Action::Item,
        15 => Action::Drive,
        16 => Action::DriveIn,
        17 => Action::HighJump,
        18 => Action::Growth(arg),
        19 => Action::Scripted(arg),
        20 => Action::StanceSwitch,
        21 => Action::LoweredWalk,
        22 => Action::LoweredRun,
        _ => return None,
    })
}

/// The stick's axes (-1..=1) in 16 bits each [T249].
fn axis_bits(v: f32) -> u16 {
    (v.clamp(-1., 1.) * 32767.).round() as i16 as u16
}

fn axis_of(bits: u16) -> f32 {
    f32::from(bits as i16) / 32767.
}

fn put_pad(w: &mut Writer, pad: &RivalPad) {
    w.u16(axis_bits(pad.direction[0]));
    w.u16(axis_bits(pad.direction[1]));
    let bits = u8::from(pad.jump)
        | u8::from(pad.jump_held) << 1
        | u8::from(pad.attack) << 2
        | u8::from(pad.guard) << 3
        | u8::from(pad.guard_held) << 4
        | u8::from(pad.shotlock_held) << 5
        | u8::from(pad.flow) << 6;
    w.u8(bits);
    let (tag, arg) = match pad.command {
        Command::None => (0, 0),
        Command::Cast(t) => (1, t),
        Command::Item(s) => (2, s),
        Command::Drive(f) => (3, f),
    };
    w.u8(tag);
    w.u8(arg);
}

fn get_pad(r: &mut Reader) -> Option<RivalPad> {
    let direction = [axis_of(r.u16()?), axis_of(r.u16()?)];
    let bits = r.u8()?;
    let (tag, arg) = (r.u8()?, r.u8()?);
    let command = match tag {
        0 => Command::None,
        1 => Command::Cast(arg),
        2 => Command::Item(arg),
        3 => Command::Drive(arg),
        _ => return None,
    };
    Some(RivalPad {
        direction,
        jump: bits & 1 != 0,
        jump_held: bits & 2 != 0,
        attack: bits & 4 != 0,
        guard: bits & 8 != 0,
        guard_held: bits & 16 != 0,
        shotlock_held: bits & 32 != 0,
        flow: bits & 64 != 0,
        command,
    })
}

/// What a joiner's input message says beyond its buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputNote {
    /// The host's tick on the joiner's screen as they went out (duel_rewind.rs).
    pub shown_tick: u64,
    /// The newest snapshot the joiner has whole, for the next delta to start from.
    pub snapshot_ack: u64,
    /// The joiner predicts its own Sora, so the snapshots carry his motion.
    pub predicting: bool,
}

/// The joiner's buttons of its newest ticks, oldest first: (joiner tick,
/// pad). The first tick is whole, each next one its step from the last (the
/// newest run whose steps fit a byte).
pub fn encode_input(recent: &[(u32, RivalPad)], note: InputNote) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(INPUT);
    w.u32(note.shown_tick as u32);
    w.u32(note.snapshot_ack as u32);
    w.u8(u8::from(note.predicting));
    let mut recent = &recent[recent.len().saturating_sub(INPUT_REDUNDANCY)..];
    while let Some(i) = recent
        .windows(2)
        .rposition(|p| p[1].0.wrapping_sub(p[0].0) > u32::from(u8::MAX))
    {
        recent = &recent[i + 1..];
    }
    w.u8(recent.len() as u8);
    let mut last = None;
    for (tick, pad) in recent {
        match last {
            None => w.u32(*tick),
            Some(t) => w.u8(tick.wrapping_sub(t) as u8),
        }
        last = Some(*tick);
        put_pad(&mut w, pad);
    }
    w.finish()
}

pub fn decode_input(bytes: &[u8]) -> Option<(InputNote, Vec<(u32, RivalPad)>)> {
    let mut r = Reader::new(bytes);
    if r.u8()? != INPUT {
        return None;
    }
    let note = InputNote {
        shown_tick: u64::from(r.u32()?),
        snapshot_ack: u64::from(r.u32()?),
        predicting: r.u8()? != 0,
    };
    let n = r.u8()? as usize;
    if n > INPUT_REDUNDANCY {
        return None;
    }
    let mut out: Vec<(u32, RivalPad)> = Vec::with_capacity(n);
    for _ in 0..n {
        let tick = match out.last() {
            None => r.u32()?,
            Some(&(t, _)) => t.wrapping_add(u32::from(r.u8()?)),
        };
        out.push((tick, get_pad(&mut r)?));
    }
    r.is_empty().then_some((note, out))
}

/// A Sora's motion beyond his place, for the player predicting him
/// (duel_predict.rs): what his next tick reads and a correction cannot work out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub vertical_speed: f32,
    pub air_velocity: [f32; 2],
    pub arc: [f32; 4],
    pub combo: [u8; 2],
    pub connected: bool,
    pub dodge_ready: bool,
    pub jump_cut: [bool; 3],
    pub command_lock: f32,
    /// A jump waiting for its take-off: height and velocity.
    pub pending_jump: Option<(f32, [f32; 2])>,
    /// The physics mode's sampled velocity: its timer and per-frame displacement.
    pub sampler: (f32, [f32; 3]),
}

/// One Sora as the other instance draws him.
#[derive(Clone, Debug, PartialEq)]
pub struct FighterState {
    pub slot: u8,
    pub position: [f32; 3],
    pub yaw: f32,
    pub action: Action,
    pub frame: u32,
    pub armed: bool,
    pub blend: f32,
    /// The host tick the action started (a change restarts the blend).
    pub started: u64,
    pub hp: i32,
    pub max_hp: i32,
    pub mp: [i32; 2],
    pub drive: [u8; 4],
    pub invulnerable: f32,
    /// The Drive form his model shows (0 = none).
    pub form: u8,
    /// His item slots (0 = used up).
    pub items: Vec<u16>,
    /// His Wisdom bullets (and KH3 shotlock bolts): key, position,
    /// direction.
    pub bullets: Vec<(u64, [f32; 3], [f32; 3])>,
    /// [Wire] These and his KH3 actors as lines: `Encoder` fills them and the
    /// joiner's copies of the others are worked out from them.
    pub flights: Vec<Flight>,
    /// [KH3 ruleset] What his HUD and model need of the KH3 layer.
    pub kh3: Option<kh3_player::Shown>,
    /// The joiner's KH2 Sora's motion.
    pub motion: Option<Motion>,
}

impl FighterState {
    /// A fighter a delta has nothing to start from.
    fn blank(slot: u8) -> FighterState {
        FighterState {
            slot,
            position: [0.; 3],
            yaw: 0.,
            action: Action::Idle,
            frame: 0,
            armed: false,
            blend: 0.,
            started: 0,
            hp: 0,
            max_hp: 0,
            mp: [0; 2],
            drive: [0; 4],
            invulnerable: 0.,
            form: 0,
            items: Vec::new(),
            bullets: Vec::new(),
            flights: Vec::new(),
            kh3: None,
            motion: None,
        }
    }
}

/// The most flying things a snapshot carries per fighter: bullets and KH3's actors.
pub const MAX_FLIGHTS: usize = 96;

/// The most item slots a snapshot carries.
pub const MAX_ITEM_SLOTS: usize = 8;

/// Snapshot numbers in fewer bytes [duel choice, T249]: positions stay
/// f32 (they move the Soras); the yaw is a turn in 65536 steps, the blend
/// and invulnerability in 1/256 and 1/16 frame, HP and MP as 16 bits, the
/// frame as 16 bits, the action's start as ticks before the snapshot.
fn yaw_bits(yaw: f32) -> u16 {
    let turn = yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    ((turn * 65536.).round() as u32 & 0xffff) as u16
}

fn yaw_of(bits: u16) -> f32 {
    let yaw = f32::from(bits) / 65536. * std::f32::consts::TAU;
    // (-pi, pi], as the field keeps it.
    if yaw > std::f32::consts::PI {
        yaw - std::f32::consts::TAU
    } else {
        yaw
    }
}

fn fixed_bits(v: f32, scale: f32) -> u16 {
    (v * scale).round().clamp(0., 65535.) as u16
}

/// An angle in (-pi, pi] in 16 bits [T279].
fn angle_bits(a: f32) -> u16 {
    ((a / std::f32::consts::PI).clamp(-1., 1.) * 32767.).round() as i16 as u16
}

fn angle_of(bits: u16) -> f32 {
    f32::from(bits as i16) / 32767. * std::f32::consts::PI
}

fn small(v: i32) -> u16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16 as u16
}

fn small_of(bits: u16) -> i32 {
    i32::from(bits as i16)
}

/// The host's picture of one tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub tick: u64,
    /// The newest of the joiner's ticks the host has applied (0: none yet).
    pub input_ack: u32,
    pub fighters: Vec<FighterState>,
}

/// What each group of a fighter's state is called in his change mask.
mod group {
    pub const POSITION: u16 = 1 << 0;
    pub const YAW: u16 = 1 << 1;
    pub const ACTION: u16 = 1 << 2;
    pub const FRAME: u16 = 1 << 3;
    pub const BLEND: u16 = 1 << 4;
    pub const VITALS: u16 = 1 << 5;
    pub const GAUGES: u16 = 1 << 6;
    pub const ITEMS: u16 = 1 << 7;
    pub const FLIGHTS: u16 = 1 << 8;
    pub const KH3: u16 = 1 << 9;
    pub const KH3_MOTION: u16 = 1 << 10;
    pub const MOTION: u16 = 1 << 11;
    pub const ALL: u16 = (1 << 12) - 1;
}

/// A full snapshot goes at least this often: a joiner that lost track picks up
/// from the next one [duel choice, KHC-017: the brief's half second].
const FULL_EVERY: u64 = 30;
/// Snapshots kept on each side for a delta to start from.
const RING: usize = 128;
/// How far a flying thing may be from its line before the host sends a new one.
pub const FLIGHT_SLACK: f32 = 1.;
/// A new line at least this often, so one lost or never started does not last.
const FLIGHT_LONGEST: u64 = 200;
/// How far a direction may turn from its line, in `dir` steps (about half a
/// degree), and the steps themselves.
const DIR_SLACK: i32 = 300;
const DIR_STEPS: f32 = 32767.;
/// A line's speed, in 1/16 of a unit a tick.
const SPEED_STEPS: f32 = 16.;
/// What sets a KH3 actor's key apart from a bullet's.
const ACTOR_KEY: u64 = 1 << 63;

/// One flying thing: a bullet, or a KH3 spell's or shotlock's actor, as a straight
/// line from a tick, so it goes once and the joiner moves it [duel choice, KHC-017:
/// whether a layer steers it or not; a steered one just needs more lines].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flight {
    pub key: u64,
    pub at: u64,
    pub pos: [u32; 3],
    pub dir: [i16; 3],
    pub speed: [i16; 3],
}

impl Flight {
    fn at_tick(&self, tick: u64) -> [f32; 3] {
        let t = tick.saturating_sub(self.at) as f32;
        [0, 1, 2].map(|k| f32::from_bits(self.pos[k]) + f32::from(self.speed[k]) / SPEED_STEPS * t)
    }

    fn direction(&self) -> [f32; 3] {
        self.dir.map(|d| f32::from(d) / DIR_STEPS)
    }

    fn put(&self, w: &mut Writer, tick: u64) {
        w.u64(self.key);
        w.u8(tick.saturating_sub(self.at).min(255) as u8);
        for v in self.pos {
            w.u32(v);
        }
        for v in self.dir.iter().chain(&self.speed) {
            w.u16(*v as u16);
        }
    }

    fn get(r: &mut Reader, tick: u64) -> Option<Flight> {
        let key = r.u64()?;
        let at = tick.saturating_sub(u64::from(r.u8()?));
        let pos = [r.u32()?, r.u32()?, r.u32()?];
        if !pos.iter().all(|b| f32::from_bits(*b).is_finite()) {
            return None;
        }
        Some(Flight {
            key,
            at,
            pos,
            dir: [r.u16()? as i16, r.u16()? as i16, r.u16()? as i16],
            speed: [r.u16()? as i16, r.u16()? as i16, r.u16()? as i16],
        })
    }
}

fn dir_steps(dir: [f32; 3]) -> [i16; 3] {
    dir.map(|d| (d.clamp(-1., 1.) * DIR_STEPS).round() as i16)
}

fn actor_key(a: &kh3_player::ShownActor) -> u64 {
    ACTOR_KEY | u64::from(a.depth) << 32 | u64::from(a.cmd) << 16 | u64::from(a.serial)
}

/// The lines a host has drawn so far.
#[derive(Clone, Debug, Default)]
struct FlightBook {
    lines: std::collections::BTreeMap<(u8, u64), Flight>,
    /// Where each was last, for its speed.
    last: std::collections::BTreeMap<(u8, u64), (u64, [f32; 3])>,
}

impl FlightBook {
    /// A fighter's flying things this tick as lines, the old ones while they hold.
    fn lines_of(&mut self, tick: u64, f: &FighterState) -> Vec<Flight> {
        let mut flying: Vec<(u64, [f32; 3], [f32; 3])> = f.bullets.clone();
        if let Some(k) = &f.kh3 {
            flying.extend(k.actors.iter().map(|a| (actor_key(a), a.pos, a.dir)));
        }
        flying.sort_by_key(|x| x.0);
        flying.dedup_by_key(|x| x.0);
        let mut out = Vec::with_capacity(flying.len());
        for (key, pos, dir) in flying {
            let id = (f.slot, key);
            let steps = dir_steps(dir);
            let holds = self.lines.get(&id).filter(|line| {
                let on = line.at_tick(tick);
                let off = (0..3).map(|k| (on[k] - pos[k]).powi(2)).sum::<f32>().sqrt();
                let turned = (0..3)
                    .map(|k| (i32::from(line.dir[k]) - i32::from(steps[k])).abs())
                    .max()
                    .unwrap_or(0);
                off <= FLIGHT_SLACK
                    && turned <= DIR_SLACK
                    && tick.saturating_sub(line.at) < FLIGHT_LONGEST
            });
            let line = match holds {
                Some(line) => *line,
                None => {
                    let speed = self.last.get(&id).map_or([0; 3], |(then, was)| {
                        let dt = tick.saturating_sub(*then).max(1) as f32;
                        [0, 1, 2].map(|k| {
                            ((pos[k] - was[k]) / dt * SPEED_STEPS)
                                .round()
                                .clamp(-32768., 32767.) as i16
                        })
                    });
                    Flight {
                        key,
                        at: tick,
                        pos: pos.map(f32::to_bits),
                        dir: steps,
                        speed,
                    }
                }
            };
            self.lines.insert(id, line);
            self.last.insert(id, (tick, pos));
            out.push(line);
        }
        let slot = f.slot;
        let live: Vec<u64> = out.iter().map(|l| l.key).collect();
        self.lines
            .retain(|(s, k), _| *s != slot || live.contains(k));
        self.last.retain(|(s, k), _| *s != slot || live.contains(k));
        out
    }
}

/// What a fighter's KH3 state is, apart from his animation and his actors.
fn kh3_state(k: &Option<kh3_player::Shown>) -> Option<[u8; 7]> {
    k.as_ref().map(|k| {
        [
            k.focus,
            k.style,
            k.mode,
            k.locks,
            u8::from(k.aiming)
                | u8::from(k.syphon) << 1
                | u8::from(k.counter) << 2
                | u8::from(k.chase) << 3
                | u8::from(k.risk) << 4,
            k.offer,
            k.charge,
        ]
    })
}

/// His KH3 animation and lean, as they travel.
fn kh3_motion(k: &kh3_player::Shown) -> [u16; 8] {
    [
        angle_bits(k.lean[0]),
        angle_bits(k.lean[1]),
        k.row,
        u16::from(k.serial),
        k.time,
        k.axis as u16,
        u16::from(k.blend as u8),
        u16::from(k.looped),
    ]
}

/// The groups that differ from what the joiner has.
fn changes(f: &FighterState, base: Option<&FighterState>) -> u16 {
    let Some(b) = base else { return group::ALL };
    let mut mask = 0;
    let mut note = |bit: u16, same: bool| {
        if !same {
            mask |= bit;
        }
    };
    note(group::POSITION, f.position == b.position);
    note(group::YAW, yaw_bits(f.yaw) == yaw_bits(b.yaw));
    note(
        group::ACTION,
        f.action == b.action && f.started == b.started,
    );
    note(group::FRAME, f.frame.min(0xffff) == b.frame.min(0xffff));
    note(
        group::BLEND,
        fixed_bits(f.blend, 256.) == fixed_bits(b.blend, 256.),
    );
    note(
        group::VITALS,
        f.hp == b.hp
            && f.max_hp == b.max_hp
            && fixed_bits(f.invulnerable, 16.) == fixed_bits(b.invulnerable, 16.),
    );
    note(
        group::GAUGES,
        f.mp == b.mp && f.drive == b.drive && f.form == b.form && f.armed == b.armed,
    );
    note(group::ITEMS, f.items == b.items);
    note(group::FLIGHTS, f.flights == b.flights);
    note(group::KH3, kh3_state(&f.kh3) == kh3_state(&b.kh3));
    note(
        group::KH3_MOTION,
        match (&f.kh3, &b.kh3) {
            (Some(x), Some(y)) => kh3_motion(x) == kh3_motion(y),
            (None, _) => true,
            (Some(_), None) => false,
        },
    );
    note(group::MOTION, f.motion == b.motion);
    mask
}

/// One fighter: what changed from `base` (all of him without one). Returns the
/// bytes his flying things took.
fn put_fighter(w: &mut Writer, tick: u64, f: &FighterState, base: Option<&FighterState>) -> usize {
    let mask = changes(f, base);
    let mut flight_bytes = 0;
    w.u8(f.slot);
    w.u16(mask);
    if mask & group::POSITION != 0 {
        for v in f.position {
            w.f32(v);
        }
    }
    if mask & group::YAW != 0 {
        w.u16(yaw_bits(f.yaw));
    }
    if mask & group::ACTION != 0 {
        let (code, arg) = action_code(f.action);
        w.u8(code);
        w.u16(arg);
        w.u32(tick.saturating_sub(f.started).min(u64::from(u32::MAX)) as u32);
    }
    if mask & group::FRAME != 0 {
        w.u16(f.frame.min(0xffff) as u16);
    }
    if mask & group::BLEND != 0 {
        w.u16(fixed_bits(f.blend, 256.));
    }
    if mask & group::VITALS != 0 {
        w.u16(small(f.hp));
        w.u16(small(f.max_hp));
        w.u16(fixed_bits(f.invulnerable, 16.));
    }
    if mask & group::GAUGES != 0 {
        w.u16(small(f.mp[0]));
        w.u16(small(f.mp[1]));
        w.bytes(&f.drive);
        w.u8(f.form);
        w.u8(u8::from(f.armed));
    }
    if mask & group::ITEMS != 0 {
        let items = &f.items[..f.items.len().min(MAX_ITEM_SLOTS)];
        w.u8(items.len() as u8);
        for &id in items {
            w.u16(id);
        }
    }
    if mask & group::FLIGHTS != 0 {
        let before = w.len();
        let old: &[Flight] = base.map_or(&[], |b| &b.flights);
        let gone: Vec<u64> = old
            .iter()
            .filter(|o| !f.flights.iter().any(|n| n.key == o.key))
            .map(|o| o.key)
            .take(255)
            .collect();
        let new: Vec<&Flight> = f
            .flights
            .iter()
            .filter(|n| !old.contains(n))
            .take(MAX_FLIGHTS)
            .collect();
        w.u8(gone.len() as u8);
        for key in gone {
            w.u64(key);
        }
        w.u8(new.len() as u8);
        for line in new {
            line.put(w, tick);
        }
        flight_bytes = w.len() - before;
    }
    if mask & group::KH3 != 0 {
        match kh3_state(&f.kh3) {
            None => w.u8(0),
            Some(state) => {
                w.u8(1);
                w.bytes(&state);
            }
        }
    }
    if mask & group::KH3_MOTION != 0
        && let Some(k) = &f.kh3
    {
        let m = kh3_motion(k);
        w.u16(m[0]);
        w.u16(m[1]);
        w.u16(m[2]);
        w.u8(k.serial);
        w.u16(m[4]);
        w.u16(m[5]);
        w.u8(k.blend as u8);
        w.u8(u8::from(k.looped));
    }
    if mask & group::MOTION != 0 {
        match &f.motion {
            None => w.u8(0),
            Some(m) => {
                w.u8(1);
                w.f32(m.vertical_speed);
                w.f32(m.command_lock);
                w.f32(m.sampler.0);
                for v in m.sampler.1 {
                    w.f32(v);
                }
                for v in m.air_velocity.iter().chain(&m.arc) {
                    w.f32(*v);
                }
                w.bytes(&m.combo);
                w.u8(u8::from(m.connected)
                    | u8::from(m.dodge_ready) << 1
                    | u8::from(m.jump_cut[0]) << 2
                    | u8::from(m.jump_cut[1]) << 3
                    | u8::from(m.jump_cut[2]) << 4
                    | u8::from(m.pending_jump.is_some()) << 5);
                if let Some((height, velocity)) = m.pending_jump {
                    w.f32(height);
                    w.f32(velocity[0]);
                    w.f32(velocity[1]);
                }
            }
        }
    }
    flight_bytes
}

/// A fighter from what came and what the joiner had: `None` when it does not parse.
fn get_fighter(r: &mut Reader, tick: u64, ring: &[FighterState]) -> Option<FighterState> {
    let slot = r.u8()?;
    let mask = r.u16()?;
    if mask & !group::ALL != 0 {
        return None;
    }
    let mut f = ring
        .iter()
        .find(|b| b.slot == slot)
        .cloned()
        .unwrap_or_else(|| FighterState::blank(slot));
    if mask & group::POSITION != 0 {
        f.position = [r.f32()?, r.f32()?, r.f32()?];
        if !f.position.iter().all(|v| v.is_finite()) {
            return None;
        }
    }
    if mask & group::YAW != 0 {
        f.yaw = yaw_of(r.u16()?);
    }
    if mask & group::ACTION != 0 {
        f.action = action_from(r.u8()?, r.u16()?)?;
        f.started = tick.saturating_sub(u64::from(r.u32()?));
    }
    if mask & group::FRAME != 0 {
        f.frame = u32::from(r.u16()?);
    }
    if mask & group::BLEND != 0 {
        f.blend = f32::from(r.u16()?) / 256.;
    }
    if mask & group::VITALS != 0 {
        f.hp = small_of(r.u16()?);
        f.max_hp = small_of(r.u16()?);
        f.invulnerable = f32::from(r.u16()?) / 16.;
    }
    if mask & group::GAUGES != 0 {
        f.mp = [small_of(r.u16()?), small_of(r.u16()?)];
        f.drive = r.array()?;
        f.form = r.u8()?;
        f.armed = r.u8()? != 0;
    }
    if mask & group::ITEMS != 0 {
        let count = r.u8()? as usize;
        if count > MAX_ITEM_SLOTS {
            return None;
        }
        f.items = (0..count).map(|_| r.u16()).collect::<Option<Vec<_>>>()?;
    }
    if mask & group::FLIGHTS != 0 {
        let gone: Vec<u64> = (0..r.u8()?).map(|_| r.u64()).collect::<Option<_>>()?;
        let count = r.u8()? as usize;
        if count > MAX_FLIGHTS {
            return None;
        }
        f.flights.retain(|l| !gone.contains(&l.key));
        for _ in 0..count {
            let line = Flight::get(r, tick)?;
            match f.flights.iter_mut().find(|l| l.key == line.key) {
                Some(old) => *old = line,
                None => f.flights.push(line),
            }
        }
        f.flights.sort_by_key(|l| l.key);
    }
    if mask & group::KH3 != 0 {
        match r.u8()? {
            0 => f.kh3 = None,
            1 => {
                let b: [u8; 7] = r.array()?;
                let k = f.kh3.get_or_insert_with(kh3_player::Shown::default);
                k.focus = b[0];
                k.style = b[1];
                k.mode = b[2];
                k.locks = b[3];
                k.aiming = b[4] & 1 != 0;
                k.syphon = b[4] & 2 != 0;
                k.counter = b[4] & 4 != 0;
                k.chase = b[4] & 8 != 0;
                k.risk = b[4] & 16 != 0;
                k.offer = b[5];
                k.charge = b[6];
            }
            _ => return None,
        }
    }
    if mask & group::KH3_MOTION != 0
        && let Some(k) = f.kh3.as_mut()
    {
        k.lean = [angle_of(r.u16()?), angle_of(r.u16()?)];
        k.row = r.u16()?;
        k.serial = r.u8()?;
        k.time = r.u16()?;
        k.axis = r.u16()? as i16;
        k.blend = r.u8()? as i8;
        k.looped = r.u8()? != 0;
    }
    if mask & group::MOTION != 0 {
        f.motion = match r.u8()? {
            0 => None,
            1 => {
                let vertical_speed = r.f32()?;
                let command_lock = r.f32()?;
                let sampler = (r.f32()?, [r.f32()?, r.f32()?, r.f32()?]);
                if !sampler.0.is_finite() || !sampler.1.iter().all(|x| x.is_finite()) {
                    return None;
                }
                let v = [
                    vertical_speed,
                    r.f32()?,
                    r.f32()?,
                    r.f32()?,
                    r.f32()?,
                    r.f32()?,
                    r.f32()?,
                    command_lock,
                ];
                if !v.iter().all(|x| x.is_finite()) {
                    return None;
                }
                let combo = r.array()?;
                let bits = r.u8()?;
                let pending_jump = if bits & 32 != 0 {
                    let j = [r.f32()?, r.f32()?, r.f32()?];
                    if !j.iter().all(|x| x.is_finite()) {
                        return None;
                    }
                    Some((j[0], [j[1], j[2]]))
                } else {
                    None
                };
                Some(Motion {
                    vertical_speed: v[0],
                    air_velocity: [v[1], v[2]],
                    arc: [v[3], v[4], v[5], v[6]],
                    combo,
                    connected: bits & 1 != 0,
                    dodge_ready: bits & 2 != 0,
                    jump_cut: [bits & 4 != 0, bits & 8 != 0, bits & 16 != 0],
                    command_lock,
                    pending_jump,
                    sampler,
                })
            }
            _ => return None,
        };
    }
    // What the rest of the joiner reads, from the lines.
    f.bullets.clear();
    let mut actors = Vec::new();
    for line in &f.flights {
        let (pos, dir) = (line.at_tick(tick), line.direction());
        if line.key & ACTOR_KEY == 0 {
            f.bullets.push((line.key, pos, dir));
        } else {
            actors.push(kh3_player::ShownActor {
                serial: line.key as u16,
                cmd: (line.key >> 16) as u16,
                depth: (line.key >> 32) as u8,
                pos,
                dir,
            });
        }
    }
    if let Some(k) = f.kh3.as_mut() {
        k.actors = actors;
    }
    Some(f)
}

/// A host's snapshots on their way out: each one whole, or what changed since the
/// newest one the joiner says it has. A lost one costs nothing: the next starts
/// from what did arrive.
#[derive(Clone, Debug, Default)]
pub struct Encoder {
    ring: std::collections::VecDeque<Snapshot>,
    ack: Option<u64>,
    last_full: Option<u64>,
    book: FlightBook,
}

/// An encoded snapshot, and how much of it was flying things.
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub flight_bytes: usize,
}

impl Encoder {
    /// The joiner has this snapshot whole.
    pub fn ack(&mut self, tick: u64) {
        self.ack = Some(self.ack.map_or(tick, |a| a.max(tick)));
    }

    pub fn encode(&mut self, snapshot: &Snapshot) -> Encoded {
        let tick = snapshot.tick;
        let mut s = snapshot.clone();
        for f in &mut s.fighters {
            f.flights = self.book.lines_of(tick, f);
        }
        let due_full = self
            .last_full
            .is_none_or(|t| tick.saturating_sub(t) >= FULL_EVERY);
        let base = self
            .ack
            .filter(|_| !due_full)
            .and_then(|a| self.ring.iter().find(|r| r.tick == a))
            .filter(|b| tick.saturating_sub(b.tick) <= u64::from(u16::MAX));
        let mut w = Writer::default();
        let mut flight_bytes = 0;
        match base {
            Some(b) => {
                w.u8(DELTA);
                w.u32(tick as u32);
                w.u16((tick - b.tick) as u16);
            }
            None => {
                w.u8(SNAPSHOT);
                w.u32(tick as u32);
                self.last_full = Some(tick);
            }
        }
        w.u32(s.input_ack);
        w.u8(s.fighters.len() as u8);
        for f in &s.fighters {
            let old = base.and_then(|b| b.fighters.iter().find(|o| o.slot == f.slot));
            flight_bytes += put_fighter(&mut w, tick, f, old);
        }
        if self.ring.len() >= RING {
            self.ring.pop_front();
        }
        self.ring.push_back(s);
        Encoded {
            bytes: w.finish(),
            flight_bytes,
        }
    }
}

/// A joiner's snapshots on their way in. Any order will do; a delta from one it
/// does not have is dropped, and the next full one puts it right.
#[derive(Clone, Debug, Default)]
pub struct Decoder {
    ring: std::collections::VecDeque<Snapshot>,
    newest: Option<u64>,
}

impl Decoder {
    /// The newest snapshot it holds whole: what the joiner acknowledges.
    pub fn newest(&self) -> Option<u64> {
        self.newest
    }

    pub fn decode(&mut self, bytes: &[u8]) -> Option<Snapshot> {
        let s = self.read(bytes)?;
        self.newest = Some(self.newest.map_or(s.tick, |n| n.max(s.tick)));
        if self.ring.len() >= RING {
            // The oldest goes first, whatever order they came in.
            let at = self
                .ring
                .iter()
                .enumerate()
                .min_by_key(|(_, r)| r.tick)
                .map_or(0, |(i, _)| i);
            self.ring.remove(at);
        }
        self.ring.push_back(s.clone());
        Some(s)
    }

    fn read(&self, bytes: &[u8]) -> Option<Snapshot> {
        let mut r = Reader::new(bytes);
        let kind = r.u8()?;
        let tick = u64::from(r.u32()?);
        let base: &[FighterState] = match kind {
            SNAPSHOT => &[],
            DELTA => {
                let back = u64::from(r.u16()?);
                let from = tick.checked_sub(back)?;
                &self.ring.iter().find(|s| s.tick == from)?.fighters
            }
            _ => return None,
        };
        let input_ack = r.u32()?;
        let n = r.u8()? as usize;
        if n > 8 {
            return None;
        }
        let mut fighters = Vec::with_capacity(n);
        for _ in 0..n {
            fighters.push(get_fighter(&mut r, tick, base)?);
        }
        r.is_empty().then_some(Snapshot {
            tick,
            input_ack,
            fighters,
        })
    }
}

/// The host's match rules (reliable, on connecting and on every change).
pub fn encode_rules(rules: &MatchRules) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(RULES);
    w.u8(rules.mode_code());
    w.f32(rules.damage);
    w.f32(rules.magic);
    w.u8(rules.hit_cap);
    w.u16(rules.time_limit);
    w.u8(rules.arena);
    w.u8(rules.ruleset_code());
    w.u8(u8::from(rules.picks[0]) | u8::from(rules.picks[1]) << 1);
    w.u8(u8::from(rules.latency_match));
    w.u8(rules.dodge.code());
    w.u8(u8::from(rules.parity));
    w.finish()
}

pub fn decode_rules(bytes: &[u8]) -> Option<MatchRules> {
    let mut r = Reader::new(bytes);
    if r.u8()? != RULES {
        return None;
    }
    let rules = MatchRules {
        mode: MatchRules::mode_from(r.u8()?)?,
        damage: r.f32()?,
        magic: r.f32()?,
        hit_cap: r.u8()?,
        time_limit: r.u16()?,
        arena: r.u8()?,
        ruleset: MatchRules::ruleset_from(r.u8()?)?,
        picks: {
            let bits = r.u8()?;
            [bits & 1 != 0, bits & 2 != 0]
        },
        latency_match: r.u8()? != 0,
        dodge: super::duel_rewind::DodgePolicy::from_code(r.u8()?)?,
        parity: r.u8()? != 0,
    };
    (r.is_empty() && rules.valid()).then_some(rules)
}

/// The base32 alphabet of a rules code (RFC 4648: letters and 2 to 7, which
/// the game's font draws).
const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
/// Most bans a code holds (both players' two).
const CODE_BANS: usize = 4;

/// [KHC-016] The host's rules and bans as a short code to save or share: `PROTOCOL`, the rules message, the bans,
/// then two bytes of a SHA-256 over all of it, in base32 and in groups of four.
pub fn encode_code(rules: &MatchRules, bans: &[u16]) -> String {
    use sha2::Digest;
    let mut w = Writer::default();
    w.u16(net::wire::PROTOCOL);
    w.bytes(&encode_rules(rules));
    w.u8(bans.len().min(CODE_BANS) as u8);
    for &b in bans.iter().take(CODE_BANS) {
        w.u16(b);
    }
    let mut bytes = w.finish();
    let sum = sha2::Sha256::digest(&bytes);
    bytes.extend_from_slice(&sum[..2]);
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0);
    for b in bytes {
        acc = acc << 8 | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(CODE_ALPHABET[(acc >> bits & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(CODE_ALPHABET[(acc << (5 - bits) & 31) as usize] as char);
    }
    out.as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("-")
}

/// A rules code read back (any case, dashes and spaces ignored), or why not: the game's font has no parentheses.
pub fn decode_code(code: &str) -> Result<(MatchRules, Vec<u16>), &'static str> {
    use sha2::Digest;
    let mut bytes = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in code.chars().filter(|c| !matches!(c, '-' | ' ')) {
        let c = c.to_ascii_uppercase() as u8;
        let v = CODE_ALPHABET
            .iter()
            .position(|&a| a == c)
            .ok_or("a rules code has only letters and the digits 2 to 7")?;
        acc = (acc << 5 | v as u32) & 0xffff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((acc >> bits) as u8);
        }
    }
    if bytes.len() < 5 {
        return Err("that rules code is too short");
    }
    let (body, sum) = bytes.split_at(bytes.len() - 2);
    if sha2::Sha256::digest(body)[..2] != *sum {
        return Err("that rules code has a typo: its check does not match");
    }
    let mut r = Reader::new(body);
    if r.u16() != Some(net::wire::PROTOCOL) {
        return Err("that rules code is from another version of KHC");
    }
    // The rules message has a fixed length; the bans follow it.
    let rules_len = encode_rules(&MatchRules::default()).len();
    let rules_bytes = r.take(rules_len).ok_or("that rules code is too short")?;
    let rules = decode_rules(rules_bytes)
        .ok_or("that rules code holds rules this version does not allow")?;
    let n = usize::from(r.u8().ok_or("that rules code is too short")?);
    if n > CODE_BANS {
        return Err("that rules code holds too many bans");
    }
    let bans = (0..n)
        .map(|_| r.u16())
        .collect::<Option<Vec<_>>>()
        .ok_or("that rules code is too short")?;
    if !r.is_empty() {
        return Err("that rules code is too long");
    }
    Ok((rules, bans))
}

/// What the joiner shows of a host-side moment: sounds and sparks are
/// played and drawn locally from these, never replicated as such.
#[derive(Clone, Debug, PartialEq)]
pub enum DuelEvent {
    /// A sound by id (bank·1000 + sound).
    Sound(u32),
    /// A swing's sound of the joiner's own Sora: a joiner predicting him has
    /// already played it.
    OwnSwing(u32),
    /// A hit spark: effect, random count, contact point, direction, and
    /// whether it is the attacker's own (Sora's object effects).
    Spark {
        effect: u8,
        random: u8,
        contact: [f32; 3],
        direction: [f32; 3],
        own: bool,
    },
    /// A guard met a swing: the system flash (11, or the clash's 13) at
    /// this point.
    Guard([f32; 3], u16),
    /// A Sora was knocked out (slot).
    KnockOut(u8),
    /// A knocked-out Sora stood up again (slot).
    StoodUp(u8),
    /// A spell or item effect call (the spell scripts' `pax_start` and
    /// `effect_*`), on the effect container by name; a bound start names the
    /// Sora by slot (`Anchor::Rival`).
    Spell {
        pax: Option<String>,
        fx: player::magic::Fx,
    },
    /// A Drive or object effect element started on a Sora (by slot) or at a
    /// place.
    DriveFx {
        pax: String,
        id: u16,
        owner: u8,
        anchor: FxAnchor,
    },
    /// [KH3 ruleset] A composite look (`kh3_player::look`) by its id, on
    /// the Sora of a seat or at a place (and yaw), at a size in tenths:
    /// the joiner starts its layers from its own copy of the table.
    Look {
        look: u8,
        slot: u8,
        size: u8,
        at: Option<([f32; 3], f32)>,
    },
    /// [KH3 mode] An effect and a sound of KH3's by the book's names (the
    /// index; 0xffff none), at a place.
    Kh3Fx {
        effect: u16,
        sound: u16,
        at: [f32; 3],
        yaw: f32,
        attach: Option<Kh3Attach>,
    },
}

/// [KH3 mode] An effect on a Sora, `bone` by the book's names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kh3Attach {
    pub seat: u8,
    pub bone: u16,
    pub offset: [f32; 3],
    pub handle: u32,
    pub group: u32,
    /// 0 starts; 1 stops; 2 stops at once.
    pub stop: u8,
}

/// Where a forwarded Drive effect starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxAnchor {
    Slot(u8),
    World { at: [f32; 3], pitch: f32, yaw: f32 },
}

fn put_vec(w: &mut Writer, v: [f32; 3]) {
    for x in v {
        w.f32(x);
    }
}

fn get_vec(r: &mut Reader) -> Option<[f32; 3]> {
    let v = [r.f32()?, r.f32()?, r.f32()?];
    v.iter().all(|x| x.is_finite()).then_some(v)
}

fn put_fx(w: &mut Writer, fx: &player::magic::Fx) {
    use player::magic::{Anchor, Fx};
    match fx {
        Fx::Start { key, id, bound, at } => {
            w.u8(0);
            w.u64(*key);
            w.u16(*id);
            w.u8(match bound {
                Some(Anchor::Rival(slot)) => *slot,
                _ => u8::MAX,
            });
            match at {
                Some(v) => {
                    w.u8(1);
                    put_vec(w, *v);
                }
                None => w.u8(0),
            }
        }
        Fx::Pos(key, v) => {
            w.u8(1);
            w.u64(*key);
            put_vec(w, *v);
        }
        Fx::Dir(key, v) => {
            w.u8(2);
            w.u64(*key);
            put_vec(w, *v);
        }
        Fx::Scale(key, s) => {
            w.u8(3);
            w.u64(*key);
            w.f32(*s);
        }
        Fx::Fade(key, f) => {
            w.u8(4);
            w.u64(*key);
            w.f32(*f);
        }
        Fx::Kill(key) => {
            w.u8(5);
            w.u64(*key);
        }
        Fx::LoopEnd(key) => {
            w.u8(6);
            w.u64(*key);
        }
    }
}

fn get_fx(r: &mut Reader) -> Option<player::magic::Fx> {
    use player::magic::{Anchor, Fx};
    let tag = r.u8()?;
    let key = r.u64()?;
    Some(match tag {
        0 => {
            let id = r.u16()?;
            let slot = r.u8()?;
            let at = match r.u8()? {
                0 => None,
                _ => Some(get_vec(r)?),
            };
            Fx::Start {
                key,
                id,
                bound: (slot != u8::MAX).then_some(Anchor::Rival(slot)),
                at,
            }
        }
        1 => Fx::Pos(key, get_vec(r)?),
        2 => Fx::Dir(key, get_vec(r)?),
        3 => Fx::Scale(key, r.f32()?),
        4 => Fx::Fade(key, r.f32()?),
        5 => Fx::Kill(key),
        6 => Fx::LoopEnd(key),
        _ => return None,
    })
}

pub fn encode_events(tick: u64, events: &[DuelEvent]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(EVENTS);
    w.u64(tick);
    w.u8(events.len().min(255) as u8);
    for e in events.iter().take(255) {
        match e {
            DuelEvent::Sound(id) => {
                w.u8(0);
                w.u32(*id);
            }
            DuelEvent::Spark {
                effect,
                random,
                contact,
                direction,
                own,
            } => {
                w.u8(1);
                w.u8(*effect);
                w.u8(*random);
                for v in contact.iter().chain(direction) {
                    w.f32(*v);
                }
                w.u8(u8::from(*own));
            }
            DuelEvent::OwnSwing(id) => {
                w.u8(9);
                w.u32(*id);
            }
            DuelEvent::KnockOut(slot) => {
                w.u8(2);
                w.u8(*slot);
            }
            DuelEvent::StoodUp(slot) => {
                w.u8(3);
                w.u8(*slot);
            }
            DuelEvent::Guard(at, flash) => {
                w.u8(4);
                for v in at {
                    w.f32(*v);
                }
                w.u16(*flash);
            }
            DuelEvent::Spell { pax, fx } => {
                w.u8(5);
                w.text(pax.as_deref().unwrap_or(""), 255);
                put_fx(&mut w, fx);
            }
            DuelEvent::DriveFx {
                pax,
                id,
                owner,
                anchor,
            } => {
                w.u8(6);
                w.text(pax, 255);
                w.u16(*id);
                w.u8(*owner);
                match anchor {
                    FxAnchor::Slot(slot) => {
                        w.u8(0);
                        w.u8(*slot);
                    }
                    FxAnchor::World { at, pitch, yaw } => {
                        w.u8(1);
                        put_vec(&mut w, *at);
                        w.f32(*pitch);
                        w.f32(*yaw);
                    }
                }
            }
            DuelEvent::Look {
                look,
                slot,
                size,
                at,
            } => {
                w.u8(7);
                w.u8(*look);
                w.u8(*slot);
                w.u8(*size);
                match at {
                    None => w.u8(0),
                    Some((at, yaw)) => {
                        w.u8(1);
                        put_vec(&mut w, *at);
                        w.f32(*yaw);
                    }
                }
            }
            DuelEvent::Kh3Fx {
                effect,
                sound,
                at,
                yaw,
                attach,
            } => {
                w.u8(8);
                w.u16(*effect);
                w.u16(*sound);
                put_vec(&mut w, *at);
                w.f32(*yaw);
                match attach {
                    Some(a) => {
                        w.u8(1);
                        w.u8(a.seat);
                        w.u16(a.bone);
                        put_vec(&mut w, a.offset);
                        w.u32(a.handle);
                        w.u32(a.group);
                        w.u8(a.stop);
                    }
                    None => w.u8(0),
                }
            }
        }
    }
    w.finish()
}

pub fn decode_events(bytes: &[u8]) -> Option<(u64, Vec<DuelEvent>)> {
    let mut r = Reader::new(bytes);
    if r.u8()? != EVENTS {
        return None;
    }
    let tick = r.u64()?;
    let n = r.u8()? as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(match r.u8()? {
            0 => DuelEvent::Sound(r.u32()?),
            1 => {
                let effect = r.u8()?;
                let random = r.u8()?;
                let contact = [r.f32()?, r.f32()?, r.f32()?];
                let direction = [r.f32()?, r.f32()?, r.f32()?];
                let own = r.u8()? != 0;
                if !contact.iter().chain(&direction).all(|v| v.is_finite()) {
                    return None;
                }
                DuelEvent::Spark {
                    effect,
                    random,
                    contact,
                    direction,
                    own,
                }
            }
            9 => DuelEvent::OwnSwing(r.u32()?),
            2 => DuelEvent::KnockOut(r.u8()?),
            3 => DuelEvent::StoodUp(r.u8()?),
            5 => {
                let pax = r.text(255)?;
                DuelEvent::Spell {
                    pax: (!pax.is_empty()).then_some(pax),
                    fx: get_fx(&mut r)?,
                }
            }
            6 => {
                let pax = r.text(255)?;
                let id = r.u16()?;
                let owner = r.u8()?;
                let anchor = match r.u8()? {
                    0 => FxAnchor::Slot(r.u8()?),
                    1 => {
                        let at = get_vec(&mut r)?;
                        let (pitch, yaw) = (r.f32()?, r.f32()?);
                        if !(pitch.is_finite() && yaw.is_finite()) {
                            return None;
                        }
                        FxAnchor::World { at, pitch, yaw }
                    }
                    _ => return None,
                };
                DuelEvent::DriveFx {
                    pax,
                    id,
                    owner,
                    anchor,
                }
            }
            4 => {
                let at = [r.f32()?, r.f32()?, r.f32()?];
                if !at.iter().all(|v| v.is_finite()) {
                    return None;
                }
                DuelEvent::Guard(at, r.u16()?)
            }
            7 => {
                let (look, slot, size) = (r.u8()?, r.u8()?, r.u8()?);
                let at = match r.u8()? {
                    0 => None,
                    1 => {
                        let at = get_vec(&mut r)?;
                        let yaw = r.f32()?;
                        if !yaw.is_finite() {
                            return None;
                        }
                        Some((at, yaw))
                    }
                    _ => return None,
                };
                DuelEvent::Look {
                    look,
                    slot,
                    size,
                    at,
                }
            }
            8 => {
                let (effect, sound) = (r.u16()?, r.u16()?);
                let at = [r.f32()?, r.f32()?, r.f32()?];
                let yaw = r.f32()?;
                let attach = match r.u8()? {
                    0 => None,
                    _ => Some(Kh3Attach {
                        seat: r.u8()?,
                        bone: r.u16()?,
                        offset: [r.f32()?, r.f32()?, r.f32()?],
                        handle: r.u32()?,
                        group: r.u32()?,
                        stop: r.u8()?,
                    }),
                };
                DuelEvent::Kh3Fx {
                    effect,
                    sound,
                    at,
                    yaw,
                    attach,
                }
            }
            _ => return None,
        });
    }
    r.is_empty().then_some((tick, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(n: u8) -> RivalPad {
        RivalPad {
            // On the wire's steps, so they come back the same.
            direction: [0.1 * f32::from(n) - 0.5, -0.5].map(|v| axis_of(axis_bits(v))),
            jump: n & 1 != 0,
            jump_held: n & 2 != 0,
            attack: n & 4 != 0,
            guard: n & 8 != 0,
            guard_held: n & 16 != 0,
            shotlock_held: n & 32 != 0,
            flow: n & 64 != 0,
            command: match n % 4 {
                0 => Command::None,
                1 => Command::Cast(n),
                2 => Command::Item(n),
                _ => Command::Drive(n % 7),
            },
        }
    }

    #[test]
    fn every_action_round_trips() {
        let actions = [
            Action::Idle,
            Action::Walk,
            Action::Run,
            Action::Jump,
            Action::Fall,
            Action::Land,
            Action::Attack(3),
            Action::AirAttack(70),
            Action::Cast(1),
            Action::AirCast(2),
            Action::Hurt(9),
            Action::Guard,
            Action::GuardRecoil,
            Action::WallRecoil,
            Action::Item,
            Action::Drive,
            Action::DriveIn,
            Action::HighJump,
            Action::Growth(205),
            Action::Scripted(0x2b),
            Action::StanceSwitch,
            Action::LoweredWalk,
            Action::LoweredRun,
        ];
        for a in actions {
            let (c, arg) = action_code(a);
            assert_eq!(action_from(c, arg), Some(a));
        }
        assert_eq!(action_from(200, 0), None);
    }

    #[test]
    fn inputs_keep_the_newest_ticks() {
        let recent: Vec<_> = (0..10u32).map(|t| (t, pad(t as u8))).collect();
        let note = InputNote {
            shown_tick: 1234,
            snapshot_ack: 1230,
            predicting: true,
        };
        let decoded = decode_input(&encode_input(&recent, note)).unwrap();
        assert_eq!(decoded, (note, recent[4..].to_vec()));
        assert_eq!(
            decode_input(&encode_input(&[], InputNote::default())).unwrap(),
            (InputNote::default(), Vec::new())
        );
        let bytes = encode_input(&recent, note);
        assert_eq!(decode_input(&bytes[..bytes.len() - 1]), None);
        // A step too long for a byte: the newest run alone.
        let gap = [(1, pad(1)), (2, pad(2)), (900, pad(3)), (901, pad(4))];
        assert_eq!(
            decode_input(&encode_input(&gap, note)).unwrap(),
            (note, gap[2..].to_vec())
        );
        assert_eq!(axis_of(axis_bits(2.)), 1.);
    }

    #[test]
    fn snapshot_numbers_keep_their_precision() {
        for yaw in [-3.1, -1., 0., 0.25, 3.] {
            let back = yaw_of(yaw_bits(yaw));
            assert!((back - yaw).abs() < 1e-4, "{yaw} -> {back}");
        }
        assert!((yaw_of(yaw_bits(7.)) - (7. - std::f32::consts::TAU)).abs() < 1e-4);
        assert_eq!(f32::from(fixed_bits(2.5, 256.)) / 256., 2.5);
        assert_eq!(fixed_bits(-1., 16.), 0);
        assert_eq!(small_of(small(-35)), -35);
        assert_eq!(small_of(small(1 << 20)), i32::from(i16::MAX));
    }

    /// What a joiner gets of `s`: lines left out, which only the wire reads.
    fn without_lines(mut s: Snapshot) -> Snapshot {
        for f in &mut s.fighters {
            f.flights.clear();
        }
        s
    }

    fn sample_snapshot() -> Snapshot {
        Snapshot {
            input_ack: 77,
            tick: 12_345,
            fighters: vec![
                FighterState {
                    slot: 0,
                    position: [1.5, -2., 300.25],
                    // Values the snapshot's steps hold exactly.
                    yaw: yaw_of(0xc000),
                    action: Action::Attack(4),
                    frame: 17,
                    armed: true,
                    blend: 8.,
                    started: 12_330,
                    hp: 120,
                    max_hp: 160,
                    mp: [80, 100],
                    drive: [0, 40, 3, 3],
                    invulnerable: 0.,
                    form: 1,
                    items: vec![1, 0, 5],
                    bullets: vec![(1 << 60 | 3, [1., 2., 3.], [0., 0., 1.])],
                    flights: Vec::new(),
                    kh3: Some(kh3_player::Shown {
                        focus: 63,
                        style: 40,
                        mode: 12,
                        locks: 9,
                        aiming: true,
                        syphon: true,
                        counter: false,
                        chase: true,
                        risk: false,
                        offer: 1,
                        lean: [angle_of(angle_bits(-1.5)), 0.],
                        row: 7,
                        serial: 3,
                        time: 480,
                        axis: -12,
                        blend: -1,
                        looped: true,
                        charge: 200,
                        actors: vec![kh3_player::ShownActor {
                            serial: 9,
                            cmd: 29,
                            depth: 1,
                            pos: [1., 2., 3.],
                            dir: [0., 0., 1.],
                        }],
                    }),
                    motion: None,
                },
                FighterState {
                    slot: 1,
                    position: [0.; 3],
                    yaw: 0.,
                    action: Action::Hurt(2),
                    frame: 0,
                    armed: false,
                    blend: 0.,
                    started: 1,
                    hp: 0,
                    max_hp: 160,
                    mp: [0, 100],
                    drive: [0; 4],
                    invulnerable: 80.,
                    form: 0,
                    items: Vec::new(),
                    bullets: Vec::new(),
                    flights: Vec::new(),
                    kh3: None,
                    motion: Some(Motion {
                        vertical_speed: -3.5,
                        air_velocity: [1.5, -2.5],
                        arc: [4., 12.5, -30., 22.],
                        combo: [2, 1],
                        connected: true,
                        dodge_ready: false,
                        jump_cut: [true, false, true],
                        command_lock: 3.,
                        pending_jump: Some((60., [1., -1.])),
                        sampler: (-0.5, [1., 2., 3.]),
                    }),
                },
            ],
        }
    }

    #[test]
    fn snapshots_round_trip() {
        let s = sample_snapshot();
        let bytes = Encoder::default().encode(&s).bytes;
        let mut d = Decoder::default();
        assert_eq!(d.decode(&bytes).map(without_lines), Some(s.clone()));
        assert_eq!(d.newest(), Some(s.tick));
        assert_eq!(Decoder::default().decode(&bytes[..bytes.len() - 3]), None);
        // The first fighter's position, past the kind, tick, ack, count, slot and mask.
        let mut nan = bytes.clone();
        nan[13..17].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(Decoder::default().decode(&nan), None);
    }

    /// The sample a tick on: he moved and his frame went on, and that is all.
    fn next_tick(s: &Snapshot) -> Snapshot {
        let mut n = s.clone();
        n.tick += 1;
        n.input_ack += 1;
        n.fighters[0].position[0] += 3.;
        n.fighters[0].frame += 1;
        n.fighters[1].position[2] -= 2.;
        n
    }

    #[test]
    fn a_delta_says_only_what_changed() {
        let mut e = Encoder::default();
        let mut d = Decoder::default();
        let first = sample_snapshot();
        let full = e.encode(&first).bytes;
        d.decode(&full).unwrap();
        e.ack(first.tick);
        let second = next_tick(&first);
        let delta = e.encode(&second).bytes;
        assert_eq!(delta[0], DELTA);
        assert!(delta.len() < 60, "{} of {}", delta.len(), full.len());
        assert_eq!(d.decode(&delta).map(without_lines), Some(second));
    }

    #[test]
    fn a_delta_after_a_lost_full_one_waits_for_the_next_full_one() {
        let mut e = Encoder::default();
        let mut d = Decoder::default();
        let mut s = sample_snapshot();
        // The joiner has this one; the host goes on from it.
        let first = e.encode(&s).bytes;
        d.decode(&first).unwrap();
        e.ack(s.tick);
        let acked = s.tick;
        let mut full_seen = false;
        for n in 1..40 {
            s = next_tick(&s);
            let bytes = e.encode(&s).bytes;
            if bytes[0] == SNAPSHOT && !full_seen {
                // The full one is lost on the way; no ack moves.
                full_seen = true;
                assert_eq!(s.tick - acked, FULL_EVERY, "at {n}");
                continue;
            }
            // Every delta still starts from what the joiner has.
            assert_eq!(
                d.decode(&bytes).map(without_lines),
                Some(s.clone()),
                "at {n}"
            );
        }
        assert!(full_seen);
    }

    #[test]
    fn a_delta_from_a_snapshot_it_never_had_is_dropped_until_a_full_one() {
        let mut e = Encoder::default();
        let mut stale = Decoder::default();
        let first = sample_snapshot();
        // The joiner never got the first; a host told otherwise sends a delta.
        let _ = e.encode(&first);
        e.ack(first.tick);
        let mut s = next_tick(&first);
        let delta = e.encode(&s).bytes;
        assert_eq!(delta[0], DELTA);
        assert_eq!(stale.decode(&delta), None);
        assert_eq!(stale.newest(), None);
        while s.tick < first.tick + FULL_EVERY {
            s = next_tick(&s);
            let bytes = e.encode(&s).bytes;
            if bytes[0] == SNAPSHOT {
                assert_eq!(stale.decode(&bytes).map(without_lines), Some(s.clone()));
                return;
            }
        }
        panic!("no full snapshot came");
    }

    #[test]
    fn a_new_newest_wins_whatever_order_they_come_in() {
        let mut e = Encoder::default();
        let mut d = Decoder::default();
        let first = sample_snapshot();
        d.decode(&e.encode(&first).bytes).unwrap();
        e.ack(first.tick);
        let second = next_tick(&first);
        let third = next_tick(&second);
        let (b2, b3) = (e.encode(&second).bytes, e.encode(&third).bytes);
        assert_eq!(d.decode(&b3).map(without_lines), Some(third.clone()));
        assert_eq!(d.decode(&b2).map(without_lines), Some(second));
        assert_eq!(d.newest(), Some(third.tick));
    }

    #[test]
    fn something_flying_straight_goes_once_and_stays_within_its_slack() {
        let mut e = Encoder::default();
        let mut d = Decoder::default();
        let key = 1 << 60 | 7;
        let mut s = sample_snapshot();
        s.fighters[0].kh3 = None;
        let (mut pos, dir) = ([0., 50., 0.], [0., 0., 1.]);
        let (mut sizes, mut carried) = (Vec::new(), 0);
        for tick in 0..120u64 {
            s.tick += 1;
            pos[2] += 40.;
            s.fighters[0].bullets = vec![(key, pos, dir)];
            s.fighters[1].motion = None;
            let enc = e.encode(&s);
            carried += enc.flight_bytes;
            sizes.push(enc.bytes.len());
            let got = d.decode(&enc.bytes).unwrap();
            e.ack(s.tick);
            let (k, at, way) = got.fighters[0].bullets[0];
            assert_eq!(k, key);
            assert_eq!(way, dir);
            let off = (0..3).map(|i| (at[i] - pos[i]).powi(2)).sum::<f32>().sqrt();
            assert!(off <= FLIGHT_SLACK, "tick {tick}: {off}");
        }
        // A spawn, a line once its speed is known, and the odd fresh one: not 120.
        assert!(carried < 200 * 8, "{carried} bytes of lines");
        // Between the full ones (every 30), the bullet costs nothing at all.
        assert!(sizes[61] < 40, "{sizes:?}");
    }

    #[test]
    fn a_steered_shot_needs_more_lines_and_stays_within_its_slack() {
        let mut e = Encoder::default();
        let mut d = Decoder::default();
        let mut s = sample_snapshot();
        let actor = |pos, dir| kh3_player::ShownActor {
            serial: 0x8003,
            cmd: kh3_player::SHOT_KOH,
            depth: 0,
            pos,
            dir,
        };
        let (mut pos, mut angle) = ([0f32, 50., 0.], 0f32);
        for tick in 0..120 {
            s.tick += 1;
            angle += 0.03;
            let dir = [angle.sin(), 0., angle.cos()];
            for k in 0..3 {
                pos[k] += dir[k] * 30.;
            }
            if let Some(k) = s.fighters[0].kh3.as_mut() {
                k.actors = vec![actor(pos, dir)];
            }
            let enc = e.encode(&s);
            let got = d.decode(&enc.bytes).unwrap();
            e.ack(s.tick);
            let a = got.fighters[0].kh3.as_ref().unwrap().actors[0];
            assert_eq!(
                (a.serial, a.cmd, a.depth),
                (0x8003, kh3_player::SHOT_KOH, 0)
            );
            let off = (0..3)
                .map(|i| (a.pos[i] - pos[i]).powi(2))
                .sum::<f32>()
                .sqrt();
            assert!(off <= FLIGHT_SLACK, "tick {tick}: {off}");
        }
    }

    #[test]
    fn a_bullet_gone_is_gone_after_a_delta() {
        let mut e = Encoder::default();
        let mut d = Decoder::default();
        let mut s = sample_snapshot();
        d.decode(&e.encode(&s).bytes).unwrap();
        e.ack(s.tick);
        s.tick += 1;
        s.fighters[0].bullets.clear();
        let got = d.decode(&e.encode(&s).bytes).unwrap();
        assert!(got.fighters[0].bullets.is_empty());
    }

    #[test]
    fn capabilities_round_trip() {
        use net::wire::{Cap, Capabilities};
        for caps in [
            Capabilities::default(),
            Capabilities {
                kh3: Cap::Hash([7; 32]),
                bbs: Cap::Unread,
            },
        ] {
            let bytes = encode_caps(&caps);
            assert_eq!(decode_caps(&bytes), Some(caps));
            assert_eq!(decode_caps(&bytes[..bytes.len() - 1]), None);
        }
    }

    #[test]
    fn rules_round_trip() {
        let rules = MatchRules {
            mode: super::super::duel_rules::Mode::Timed,
            damage: 3.5,
            magic: 1.5,
            hit_cap: 50,
            time_limit: 90,
            arena: 1,
            ruleset: super::super::duel_rules::Ruleset::Each,
            picks: [false, true],
            latency_match: true,
            dodge: super::super::duel_rewind::DodgePolicy::DefenderWins,
            parity: true,
        };
        assert_eq!(decode_rules(&encode_rules(&rules)), Some(rules));
        assert_eq!(decode_rules(&[RULES]), None);
        let mut nan = encode_rules(&rules);
        nan[2..6].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(decode_rules(&nan), None);
    }

    /// KHC-016: a rules code comes back exact, and a typo or another version is refused.
    #[test]
    fn a_rules_code_round_trips_and_a_bad_check_is_refused() {
        let rules = MatchRules {
            mode: super::super::duel_rules::Mode::BestOf3,
            damage: 2.,
            hit_cap: 30,
            arena: 1,
            ruleset: super::super::duel_rules::Ruleset::Each,
            ..MatchRules::default()
        };
        let code = encode_code(&rules, &[0x52, 0x19f]);
        assert!(code.len() <= 60, "{code}");
        assert_eq!(decode_code(&code), Ok((rules, vec![0x52, 0x19f])));
        assert_eq!(
            decode_code(&code.to_lowercase().replace('-', " ")),
            Ok((rules, vec![0x52, 0x19f])),
            "any case, spaces for dashes"
        );
        assert_eq!(
            decode_code(&encode_code(&MatchRules::default(), &[])).map(|c| c.1),
            Ok(vec![])
        );
        // One letter changed: the check refuses it.
        let mut typo: Vec<char> = code.chars().collect();
        typo[5] = if typo[5] == 'A' { 'B' } else { 'A' };
        let typo: String = typo.into_iter().collect();
        assert!(decode_code(&typo).unwrap_err().contains("check"));
        assert!(decode_code("ABC1").is_err(), "1 is not in the alphabet");
        assert!(decode_code("AAAA").is_err(), "too short");
    }

    #[test]
    fn events_round_trip() {
        let events = vec![
            DuelEvent::Sound(500_012),
            DuelEvent::OwnSwing(500_013),
            DuelEvent::Spark {
                effect: 3,
                random: 2,
                contact: [1., 2., 3.],
                direction: [0., 0., 1.],
                own: true,
            },
            DuelEvent::KnockOut(1),
            DuelEvent::StoodUp(0),
            DuelEvent::Guard([4., 5., 6.], 13),
            DuelEvent::Spell {
                pax: Some("magic/FIRE_3.mag".into()),
                fx: player::magic::Fx::Start {
                    key: 1 << 60 | 5,
                    id: 12,
                    bound: Some(player::magic::Anchor::Rival(1)),
                    at: Some([1., 2., 3.]),
                },
            },
            DuelEvent::Spell {
                pax: None,
                fx: player::magic::Fx::Dir(9, [0., 1., 0.]),
            },
            DuelEvent::Spell {
                pax: None,
                fx: player::magic::Fx::Fade(9, 15.),
            },
            DuelEvent::DriveFx {
                pax: "form:1".into(),
                id: 3,
                owner: 1,
                anchor: FxAnchor::Slot(0),
            },
            DuelEvent::DriveFx {
                pax: "system".into(),
                id: 11,
                owner: 0xff,
                anchor: FxAnchor::World {
                    at: [1., 0., 2.],
                    pitch: 0.5,
                    yaw: -1.,
                },
            },
            DuelEvent::Look {
                look: 3,
                slot: 1,
                size: 10,
                at: None,
            },
            DuelEvent::Look {
                look: 30,
                slot: 0,
                size: 13,
                at: Some(([4., 90., -2.], 0.25)),
            },
        ];
        assert_eq!(
            decode_events(&encode_events(99, &events)),
            Some((99, events))
        );
        // A look on a Sora is five bytes: it replaces an element list.
        let one = |e: DuelEvent| encode_events(0, &[e]).len();
        assert_eq!(
            one(DuelEvent::Look {
                look: 0,
                slot: 0,
                size: 10,
                at: None
            }) - one(DuelEvent::KnockOut(0)),
            3
        );
        assert_eq!(decode_events(&[EVENTS]), None);
        assert_eq!(
            decode_events(&encode_input(&[], InputNote::default())),
            None
        );
    }
}
