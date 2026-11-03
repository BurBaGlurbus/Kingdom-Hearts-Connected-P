//! [Test] The scripted runs' clock (`--script-after-load`, KHC-024). The
//! scripts press their buttons by tick; counted from the program's start, the
//! time a run spent loading would shift them. On this clock they start when
//! loading is over: at the fight's start on the host's clock in a session, and
//! once KH3's data is read without one. Until then the room Sora stands, and
//! at the start both Soras' clocks go back to zero: how long they stood must
//! not show.
//!
//! `--script-locked` (on both players) also takes the network's timing out of
//! a session, so that a networked run can repeat [duel choice, KHC-024]: the
//! joiner's script reads each snapshot once, its buttons carry that snapshot's
//! tick, and the host uses them exactly `LOCK_DELAY` ticks later.

use super::duel_match::Phase;
use super::duel_match_room::MatchRoom;
use super::duel_net::DuelNet;
use super::*;
use net::Role;

/// Ticks between a snapshot and the host using the buttons that answer it:
/// more than a loopback round trip and the longest frame a run should have.
pub const LOCK_DELAY: u64 = 60;
/// A run ends this many ticks after its match does (a rematch's lobby would
/// start on the network's time).
const AFTER_MATCH: u64 = 120;
/// The longest frame a locked run makes up for.
const CATCH_UP: std::time::Duration = std::time::Duration::from_secs(10);
/// A joiner stops this much later than the host, so that its leaving is not
/// in the host's last ticks.
const JOINER_LATER: u64 = 180;

#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptClock {
    /// `--script-after-load`.
    pub on: bool,
    /// `--script-locked`.
    pub locked: bool,
    /// `--script-stop`: the run ends this many ticks after the start.
    pub stop: Option<u64>,
    /// The start, on this instance's own tick and on the host's.
    start: Option<(u64, u64)>,
    /// Ticks since the start, on the host's clock.
    elapsed: u64,
    /// `--duel-crash-at`: this game panics at this tick, to try a crash report.
    pub crash_at: Option<u64>,
}

impl ScriptClock {
    /// The scripts have to wait.
    pub fn waiting(&self) -> bool {
        self.on && self.start.is_none()
    }

    /// The host's tick the script started.
    /// Ticks since the script's start, on the host's clock, once it has started.
    pub fn elapsed(&self) -> Option<u64> {
        self.start.map(|_| self.elapsed)
    }

    pub fn host_start(&self) -> Option<u64> {
        self.start.map(|(_, from)| from)
    }
}

/// [Test] `--duel-press-test`: how often a Sora jumps, and where in that the
/// joiner's turn comes.
const PRESS_EVERY: u64 = 120;
const PRESS_SECOND: u64 = 60;

/// [Test] A press-test Sora: he stands, and jumps once in a while, the two
/// players in turn.
pub fn press_pad(tick: u64, slot: u8) -> rival::RivalPad {
    let offset = if slot == 0 { 0 } else { PRESS_SECOND };
    let phase = tick.wrapping_add(PRESS_EVERY - offset) % PRESS_EVERY;
    rival::RivalPad {
        jump: phase == 0 && tick > 0,
        jump_held: phase < 10 && tick > 0,
        ..rival::RivalPad::default()
    }
}

/// [Test] `--duel-move-test`: a joiner's Sora only walks, runs and jumps: the
/// stick turns, eases off and stops, and he jumps now and then, far from
/// anything to swing at.
pub fn move_pad(tick: u64) -> rival::RivalPad {
    let phase = tick % 600;
    let angle = tick as f32 * 0.01;
    let speed = match phase {
        0..300 => 1.,
        300..450 => 0.5,
        450..500 => 0.,
        _ => 1.,
    };
    let jump_phase = tick % 240;
    rival::RivalPad {
        direction: [angle.sin() * speed, angle.cos() * speed],
        jump: jump_phase == 100,
        jump_held: (100..125).contains(&jump_phase),
        ..rival::RivalPad::default()
    }
}

/// [Test] `--duel-strafe`: a host's Sora close by steps side to side, and puts
/// up his guard now and then, for the joiner's combos to find.
pub fn strafe_pad(tick: u64, me: [f32; 3], foe: [f32; 3]) -> rival::RivalPad {
    let to = Vec2::new(foe[0] - me[0], foe[2] - me[2]);
    let chase = to.normalize_or_zero();
    let side = Vec2::new(-chase.y, chase.x);
    let way = if (tick / 120).is_multiple_of(2) {
        1.
    } else {
        -1.
    };
    let direction = if to.length() > 300. {
        chase
    } else {
        side * way * 0.6
    };
    let phase = tick % 90;
    rival::RivalPad {
        direction: direction.to_array(),
        guard: phase == 0 && tick > 0,
        guard_held: phase < 15 && tick > 0,
        ..rival::RivalPad::default()
    }
}

/// Milliseconds by the wall clock: two instances on one PC read the same one.
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// [Test] A press goes in the log, on the side that pressed.
pub fn note_press(game: &mut Game, pad: &rival::RivalPad, script_tick: u64) {
    if pad.jump {
        let (tick, slot, unix_ms) = (game.tick, game.local_slot, unix_ms());
        game.event(
            serde_json::json!({"event":"duel_press","tick":tick,"slot":slot,
            "script_tick":script_tick,"unix_ms":unix_ms}),
        );
    }
}

/// [Test] When each Sora this instance shows starts an action, for timing a
/// press against it.
pub fn press_seen(mut game: ResMut<Game>) {
    if !game.ext.online().press_test {
        return;
    }
    let local = usize::from(game.local_slot).min(1);
    let mut now = [None; 2];
    now[local] = Some(game.sora.player.action);
    for r in &game.rivals {
        if let Some(n) = now.get_mut(usize::from(r.slot)) {
            *n = Some(r.fighter.player.action);
        }
    }
    let seen = game.ext.online().press_seen;
    game.ext.online_mut().press_seen = now;
    for slot in 0..2 {
        if now[slot] != seen[slot]
            && let Some(action) = now[slot]
        {
            let (tick, unix_ms) = (game.tick, unix_ms());
            game.event(
                serde_json::json!({"event":"duel_action_start","tick":tick,"slot":slot,
                "action":format!("{action:?}"),"unix_ms":unix_ms}),
            );
        }
    }
}

/// The tick a script reads: `tick` itself without the clock, and nothing while
/// it waits.
pub fn now(game: &Game, tick: u64) -> Option<u64> {
    let clock = &game.ext.online().script;
    if !clock.on {
        return Some(tick);
    }
    clock.start.map(|_| clock.elapsed)
}

/// The host's clock on this instance.
fn host_clock(net: &DuelNet, game: &Game) -> u64 {
    match net.role {
        Role::Host => game.tick,
        Role::Join => net.shown_snapshot.unwrap_or(0),
    }
}

/// A Sora's clocks and KH3's Sora, as if he had just come in: his state must
/// not depend on how long he stood. KH3's Sora is made again on his next step.
fn restart(game: &mut Game) {
    game.sora.player.restart_clocks();
    let k = &mut game.sora.ext.fighter_mut().kh3;
    let abilities = k.abilities;
    *k = kh3_player::Kh3::default();
    k.abilities = abilities;
}

/// One fixed step, after the match's.
pub fn tick(
    net: Option<Res<DuelNet>>,
    room: Option<Res<MatchRoom>>,
    mut time: ResMut<Time<Virtual>>,
    mut game: ResMut<Game>,
) {
    let clock = game.ext.online().script;
    // A locked run answers every snapshot, so a long frame is made up for
    // tick by tick and not lost.
    if clock.locked {
        time.set_max_delta(CATCH_UP);
    }
    if let Some(at) = clock.crash_at
        && game.tick >= at
    {
        panic!("[Test] --duel-crash-at {at}: this game stops on purpose");
    }
    if !clock.on {
        return;
    }
    let host_now = net.as_deref().map(|n| host_clock(n, &game));
    if let Some((_, from)) = clock.start {
        let now = host_now.unwrap_or(game.tick);
        game.ext.online_mut().script.elapsed = now.saturating_sub(from);
        if room
            .as_deref()
            .and_then(|r| r.m)
            .is_some_and(|m| m.phase == Phase::Over)
        {
            let later = if game.replica { JOINER_LATER } else { 0 };
            let end = game.tick + AFTER_MATCH + later;
            game.limit = Some(game.limit.map_or(end, |l| l.min(end)));
        }
        return;
    }
    let from = match (host_now, room.as_deref().and_then(|r| r.m)) {
        // In a session: the first fight, at the tick it began.
        (Some(_), Some(m)) if m.phase == Phase::Fight => Some(m.since),
        (Some(_), _) => None,
        // Alone: once KH3's data is in, when a Sora needs it.
        (None, _) => {
            let o = game.ext.online();
            let loaded = !o.duel_rules.kh3() || (o.kh3_book.is_some() && !o.kh3_loading);
            loaded.then_some(game.tick)
        }
    };
    let Some(from) = from else { return };
    let local = game.tick;
    game.ext.online_mut().script.start = Some((local, from));
    if !game.replica {
        cleanup::each_sora(&mut game, restart);
    }
    if let Some(stop) = clock.stop {
        let later = if game.replica { JOINER_LATER } else { 0 };
        game.limit = Some(local + stop + later);
    }
    game.event(serde_json::json!({"event":"duel_script_start","tick":local,"host_tick":from}));
}
