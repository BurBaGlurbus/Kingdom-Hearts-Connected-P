//! A player's own Sora at once (`--predict`, KHC-033): the joiner steps its
//! Sora itself from its own buttons, as the host's tick does for that seat, and
//! corrects him from the host's snapshots. Movement and animation only: HP, MP,
//! items, reactions and knock-outs stay the host's, and a command waits for the
//! host as before. KH3's seats are not predicted [duel choice, KHC-033]: a KH3
//! step starts effects and cues, so running it again for a replay is not free
//! of what it did.
//!
//! The ring and the correction know nothing of the room, so any seat can use
//! them and a test can run them on a toy.

use super::duel_net::fighter_slots;
use super::duel_wire::{FighterState, Motion, Snapshot};
use super::rival::RivalPad;
use super::*;
use std::collections::VecDeque;

/// Ticks of buttons and predictions kept.
const RING: usize = 64;
/// Host and prediction agree when within this of each other, in one action.
const POSITION_TOLERANCE: f32 = 4.;
/// A correction's drawn offset fades over this many ticks ...
const BLEND_TICKS: u32 = 6;
/// ... and a bigger one is drawn at once (a round's start, a teleport).
pub const BLEND_MAX: f32 = 120.;
/// What the pass line calls a large correction.
pub const LARGE_CORRECTION: f32 = 10.;
/// The overlay's window, in ticks.
const MINUTE: u64 = 3600;

/// What a correction needs of a predicted state.
pub trait Predicted: Clone {
    type Truth;
    fn position(&self) -> [f32; 3];
    /// Whether this state and the host's agree.
    fn agrees(&self, truth: &Self::Truth) -> bool;
    /// Takes the host's state.
    fn restore(&mut self, truth: &Self::Truth);
}

struct Entry<S> {
    tick: u32,
    pad: RivalPad,
    state: S,
}

/// Each tick's buttons and the state they led to.
pub struct Ring<S> {
    entries: VecDeque<Entry<S>>,
}

impl<S> Default for Ring<S> {
    fn default() -> Self {
        Ring {
            entries: VecDeque::new(),
        }
    }
}

/// What came of comparing with the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// Nothing to compare with yet.
    Waiting,
    Agreed,
    /// The state moved this far.
    Corrected {
        moved: f32,
    },
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    Vec3::from_array(a).distance(Vec3::from_array(b))
}

impl<S: Predicted> Ring<S> {
    pub fn push(&mut self, tick: u32, pad: RivalPad, state: S) {
        if self.entries.len() >= RING {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry { tick, pad, state });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// The host's state after its tick of the buttons `ack`, against what this
    /// side predicted there: agreed, nothing changes; otherwise the host's
    /// state, then every later tick's buttons again, ending in `now`.
    pub fn reconcile(
        &mut self,
        ack: u32,
        truth: &S::Truth,
        now: &mut S,
        mut step: impl FnMut(&mut S, u32, &RivalPad),
    ) -> Outcome {
        let Some(front) = self.entries.front().map(|e| e.tick) else {
            return Outcome::Waiting;
        };
        // Older ones are of no more use.
        while self.entries.front().is_some_and(|e| e.tick < ack) {
            self.entries.pop_front();
        }
        let at = match self.entries.front() {
            Some(e) if e.tick == ack => Some(0),
            _ => None,
        };
        let mut base = match at {
            Some(i) if self.entries[i].state.agrees(truth) => return Outcome::Agreed,
            Some(i) => self.entries[i].state.clone(),
            // Older than the ring (a very long round trip, or the ring just
            // began): the host's state, on what this side has now.
            None if ack < front => now.clone(),
            // No such tick: the buttons that made it are not here.
            None => return Outcome::Waiting,
        };
        base.restore(truth);
        let mut from = 0;
        if let Some(i) = at {
            self.entries[i].state = base.clone();
            from = i + 1;
        }
        for e in self.entries.iter_mut().skip(from) {
            step(&mut base, e.tick, &e.pad);
            e.state = base.clone();
        }
        let moved = distance(now.position(), base.position());
        *now = base;
        Outcome::Corrected { moved }
    }
}

/// A correction's offset on the drawn Sora, fading away. It is never longer
/// than `BLEND_MAX`, and only ever shrinks without a new correction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blend {
    pub offset: [f32; 3],
    left: u32,
}

impl Blend {
    /// The drawn Sora should not jump: `from` was where he was, `to` is where
    /// he is. Too far is drawn at once.
    pub fn start(&mut self, from: [f32; 3], to: [f32; 3]) {
        let moved = Vec3::from_array(from) - Vec3::from_array(to);
        let offset = Vec3::from_array(self.offset) + moved;
        if offset.length() > BLEND_MAX {
            *self = Blend::default();
            return;
        }
        self.offset = offset.to_array();
        self.left = BLEND_TICKS;
    }

    /// One tick less of it.
    pub fn tick(&mut self) {
        if self.left == 0 {
            self.offset = [0.; 3];
            return;
        }
        let keep = (self.left - 1) as f32 / self.left as f32;
        self.offset = self.offset.map(|v| v * keep);
        self.left -= 1;
    }
}

/// How often the prediction was wrong, for the overlay and the log.
#[derive(Clone, Debug, Default)]
pub struct Corrections {
    recent: VecDeque<(u64, f32)>,
    pub total: u64,
    pub large: u64,
}

impl Corrections {
    pub fn note(&mut self, tick: u64, moved: f32) {
        self.total += 1;
        if moved > LARGE_CORRECTION {
            self.large += 1;
        }
        self.recent.push_back((tick, moved));
        while self.recent.front().is_some_and(|(t, _)| t + MINUTE <= tick) {
            self.recent.pop_front();
        }
    }

    /// Those of the last minute: how many and the largest.
    pub fn last_minute(&self, tick: u64) -> (usize, f32) {
        let recent = self.recent.iter().filter(|(t, _)| t + MINUTE > tick);
        recent.fold((0, 0.), |(n, big), &(_, m)| (n + 1, f32::max(big, m)))
    }
}

/// The joiner's own Sora as predicted.
#[derive(Clone)]
pub struct OwnState {
    player: Player,
    floor: Option<([f32; 3], u8)>,
}

/// What a snapshot says of a Sora's motion.
pub struct OwnTruth {
    position: [f32; 3],
    yaw: f32,
    action: Action,
    frame: u32,
    armed: bool,
    blend: f32,
    motion: Option<Motion>,
}

impl OwnTruth {
    fn of(s: &FighterState) -> OwnTruth {
        OwnTruth {
            position: s.position,
            yaw: s.yaw,
            action: s.action,
            frame: s.frame,
            armed: s.armed,
            blend: s.blend,
            motion: s.motion,
        }
    }
}

impl Predicted for OwnState {
    type Truth = OwnTruth;

    fn position(&self) -> [f32; 3] {
        self.player.position
    }

    fn agrees(&self, t: &OwnTruth) -> bool {
        self.player.action == t.action
            && distance(self.player.position, t.position) < POSITION_TOLERANCE
    }

    fn restore(&mut self, t: &OwnTruth) {
        restore_player(&mut self.player, t);
    }
}

fn restore_player(p: &mut Player, t: &OwnTruth) {
    p.position = t.position;
    p.yaw = t.yaw;
    p.action = t.action;
    p.frame = t.frame;
    p.armed = t.armed;
    p.blend = t.blend;
    p.knock = None;
    p.flight = None;
    p.pending_jump = None;
    if let Some(m) = t.motion {
        p.vertical_speed = m.vertical_speed;
        p.air_velocity = m.air_velocity;
        p.arc = player::actor_physics::Arc {
            t: m.arc[0],
            rise: m.arc[1],
            base: m.arc[2],
            span: m.arc[3],
        };
        p.combo.step = m.combo[0];
        p.combo.finish = m.combo[1];
        p.connected = m.connected;
        p.dodge_ready = m.dodge_ready;
        p.jump_cut.variable = m.jump_cut[0];
        p.jump_cut.held = m.jump_cut[1];
        p.jump_cut.released = m.jump_cut[2];
        p.set_command_lock(m.command_lock);
        p.pending_jump = m.pending_jump;
        p.sampler.timer = m.sampler.0;
        p.sampler.velocity = m.sampler.1;
    }
}

/// What a snapshot carries of a Sora's motion beyond his place.
pub fn motion_of(p: &Player) -> Motion {
    Motion {
        vertical_speed: p.vertical_speed,
        air_velocity: p.air_velocity,
        arc: [p.arc.t, p.arc.rise, p.arc.base, p.arc.span],
        combo: [p.combo.step, p.combo.finish],
        connected: p.connected,
        dodge_ready: p.dodge_ready,
        jump_cut: [p.jump_cut.variable, p.jump_cut.held, p.jump_cut.released],
        command_lock: p.command_lock(),
        pending_jump: p.pending_jump,
        sampler: (p.sampler.timer, p.sampler.velocity),
    }
}

/// Actions only the host starts or runs: while the host's Sora is in one, this
/// side shows him as the host has him and does not step him.
fn host_driven(action: Action) -> bool {
    matches!(
        action,
        Action::Hurt(_)
            | Action::Cast(_)
            | Action::AirCast(_)
            | Action::GuardRecoil
            | Action::WallRecoil
            | Action::Item
            | Action::Drive
            | Action::DriveIn
            | Action::Growth(_)
            | Action::Scripted(_)
    )
}

/// Where the other Sora will be when the host uses a tick of this side's
/// buttons: his newest place, carried on at his last speed. The host's tick of
/// `ack` made the newest snapshot, so a later tick of buttons meets him that
/// many ticks further on. A swing's hop aims at him, and the host's hop aims at
/// him then, not at where this side shows him.
#[derive(Clone, Copy, Debug, Default)]
pub struct FoeTrack {
    tick: u64,
    ack: u32,
    at: [f32; 3],
    velocity: [f32; 3],
}

/// The furthest the other Sora is carried on, in ticks.
const FOE_LEAD: u32 = 30;

impl FoeTrack {
    /// A new snapshot of him.
    fn update(&mut self, tick: u64, ack: u32, at: [f32; 3]) {
        let dt = tick.saturating_sub(self.tick);
        self.velocity = if self.tick == 0 || dt == 0 || dt > 15 {
            [0.; 3]
        } else {
            let k = 1. / dt as f32;
            std::array::from_fn(|i| (at[i] - self.at[i]) * k)
        };
        self.tick = tick;
        self.ack = ack;
        self.at = at;
    }

    /// Where he will be for this tick of buttons.
    pub fn at(&self, key: u32) -> [f32; 3] {
        let lead = key.saturating_sub(self.ack).min(FOE_LEAD) as f32;
        std::array::from_fn(|i| self.at[i] + self.velocity[i] * lead)
    }
}

/// Swings played here that the host has not yet said it made too.
#[derive(Clone, Debug, Default)]
pub struct Swings {
    played: VecDeque<(u32, u64)>,
}

/// How long a swing played here waits for the host's: past any round trip the
/// game can stand.
const SWING_WINDOW: u64 = 90;

impl Swings {
    pub fn played(&mut self, id: u32, tick: u64) {
        self.played.push_back((id, tick));
    }

    /// The host's event for a swing: true when it was played here, once.
    pub fn heard(&mut self, id: u32, tick: u64) -> bool {
        while self
            .played
            .front()
            .is_some_and(|&(_, at)| at + SWING_WINDOW < tick)
        {
            self.played.pop_front();
        }
        match self.played.iter().position(|&(i, _)| i == id) {
            Some(i) => {
                self.played.remove(i);
                true
            }
            None => false,
        }
    }
}

/// The prediction of this player's own Sora.
#[derive(Default)]
pub struct Predict {
    /// `--predict`.
    pub on: bool,
    ring: Ring<OwnState>,
    pub blend: Blend,
    /// The offset now on the drawn Sora.
    drawn: [f32; 3],
    /// The host has him in an action of its own.
    following: bool,
    /// The newest snapshot and buttons already compared.
    compared: Option<(u64, u32)>,
    pub foe: FoeTrack,
    pub swings: Swings,
    pub host: HostShown,
    pub corrections: Corrections,
}

impl Predict {
    pub fn new(on: bool) -> Predict {
        Predict {
            on,
            ..Predict::default()
        }
    }
}

/// Whether this instance is predicting its own Sora this tick.
pub fn active(game: &Game) -> bool {
    let o = game.ext.online();
    o.predict.on
        && game.replica
        && !o.duel_rules.seat_kh3(game.local_slot)
        && !o.duel_frozen
        && !o.duel_idle
        && !o.kh3_loading
        && !o.script.waiting()
        && !o.script.locked
}

fn floor_of(game: &Game) -> Option<([f32; 3], u8)> {
    match &game.world {
        RoomWorld::Map(map) => {
            let f = map.swap_floor_state(area::FLOOR_NONE);
            map.swap_floor_state(f);
            Some(f)
        }
        RoomWorld::Triangles(_) => None,
    }
}

fn set_floor(game: &Game, floor: Option<([f32; 3], u8)>) {
    if let (RoomWorld::Map(map), Some(f)) = (&game.world, floor) {
        map.swap_floor_state(f);
    }
}

/// One tick of the joiner's own Sora from this tick's buttons.
pub fn step_own(game: &mut Game, pad: &RivalPad) -> player::Step {
    if !active(game) || game.ext.online().predict.following {
        return player::Step::default();
    }
    let key = game.tick as u32 + 1;
    rival_play::predict_step(game, pad, Some(key))
}

/// After the field's tick: this tick's buttons and what they led to.
pub fn remember(game: &mut Game, tick: u32, pad: RivalPad) {
    if !game.ext.online().predict.on {
        return;
    }
    if !active(game) {
        let p = &mut game.ext.online_mut().predict;
        p.ring.clear();
        p.following = false;
        p.compared = None;
        return;
    }
    let state = OwnState {
        player: game.sora.player.clone(),
        floor: floor_of(game),
    };
    game.ext.online_mut().predict.ring.push(tick, pad, state);
}

/// A new snapshot: the joiner's own Sora against the host's.
pub fn compare(game: &mut Game, snapshot: &Snapshot) {
    let slot = game.local_slot;
    let Some(s) = snapshot.fighters.iter().find(|f| f.slot == slot) else {
        return;
    };
    let ack = snapshot.input_ack;
    let predict = &mut game.ext.online_mut().predict;
    if predict
        .compared
        .is_some_and(|(tick, _)| snapshot.tick <= tick)
    {
        return;
    }
    let same_ack = predict.compared.is_some_and(|(_, a)| a == ack);
    predict.compared = Some((snapshot.tick, ack));
    if let Some(foe) = snapshot.fighters.iter().find(|f| f.slot != slot) {
        predict.foe.update(snapshot.tick, ack, foe.position);
    }
    // The form's kit first: the action names one of its motions.
    duel_sync::show_form(game, slot, s.form);
    if game.drive.as_ref().is_some_and(|d| d.shown != s.form) {
        // The kit is still loading: nothing he does can be stepped.
        game.ext.online_mut().predict.following = true;
        return;
    }
    let truth = OwnTruth::of(s);
    let old = game.sora.player.clone();
    if host_driven(s.action) {
        restore_player(&mut game.sora.player, &truth);
        game.ext.online_mut().predict.following = true;
        settle(game, &old);
        return;
    }
    let was_following = std::mem::take(&mut game.ext.online_mut().predict.following);
    // The host's buttons have not moved on: it has nothing to say yet of
    // what this side has done.
    if same_ack && !was_following {
        return;
    }
    let mut now = OwnState {
        player: old.clone(),
        floor: floor_of(game),
    };
    let mut ring = std::mem::take(&mut game.ext.online_mut().predict.ring);
    let outcome = ring.reconcile(ack, &truth, &mut now, |state, key, pad| {
        std::mem::swap(&mut game.sora.player, &mut state.player);
        set_floor(game, state.floor);
        rival_play::predict_step(game, pad, Some(key));
        state.floor = floor_of(game);
        std::mem::swap(&mut game.sora.player, &mut state.player);
    });
    game.ext.online_mut().predict.ring = ring;
    set_floor(game, now.floor);
    let Outcome::Corrected { moved } = outcome else {
        // A hit only the host can know of: the next press of the combo chains
        // on it here too, or this side's Sora would refuse what the host's did.
        if s.action == old.action && s.motion.is_some_and(|m| m.connected) {
            game.sora.player.connected = true;
        }
        return;
    };
    game.sora.player = now.player;
    settle(game, &old);
    let tick = game.tick;
    let (position, action) = (game.sora.player.position, game.sora.player.action);
    // The swing's scan goes on from here, whatever he was doing: what the
    // correction skipped comes from the host.
    let frame = game.sora.player.motion_frame(&game.clips);
    game.sound_frame = Some((game.action_started, frame));
    let predict = &mut game.ext.online_mut().predict;
    predict.blend.start(old.position, position);
    if !was_following {
        predict.corrections.note(tick, moved);
        game.event(
            serde_json::json!({"event":"duel_correction","tick":tick,"moved":moved,
            "ack":ack,"was":format!("{:?}", old.action),"now":format!("{action:?}"),
            "host":format!("{:?}", s.action)}),
        );
    }
}

/// What stays the host's after the Sora's motion was replaced, and the action's
/// start for the presenter.
fn settle(game: &mut Game, old: &Player) {
    let p = &mut game.sora.player;
    p.mp = old.mp;
    p.drive = old.drive;
    p.abilities = old.abilities.clone();
    if p.action != old.action || p.frame < old.frame {
        game.action_started = game.tick;
    }
}

/// The host's own Sora as his player sees him while his buttons wait
/// (duel_delay.rs): each tick the simulated Sora, then the buttons still on
/// their way. A hit on the simulated Sora shows the same tick, as this is made
/// again from him every time.
#[derive(Default)]
pub struct HostShown {
    /// While shown, the simulated Sora; otherwise the predicted one.
    other: Option<Player>,
    swapped: bool,
    /// The tick the shown action began, for the presenter's blend, and what
    /// he showed last.
    started: u64,
    last: Option<(Action, u32)>,
}

/// In the room's tick, before the camera: the host's own Sora as he will be
/// once the buttons on their way have reached him.
pub fn host_predict(game: &mut Game) -> Option<Player> {
    let online = game.ext.online();
    if game.replica
        || online.own_delay.ticks == 0
        || online.duel_frozen
        || online.duel_rules.seat_kh3(game.local_slot)
    {
        let host = &mut game.ext.online_mut().predict.host;
        host.other = None;
        host.last = None;
        return None;
    }
    let pads: Vec<RivalPad> = online.own_delay.fifo.waiting().copied().collect();
    let sim = game.sora.player.clone();
    let floor = floor_of(game);
    for pad in &pads {
        rival_play::predict_step(game, pad, None);
    }
    let shown = std::mem::replace(&mut game.sora.player, sim);
    set_floor(game, floor);
    let tick = game.tick;
    let press_test = online_press_test(game);
    let host = &mut game.ext.online_mut().predict.host;
    let new_action = host
        .last
        .is_none_or(|(action, frame)| action != shown.action || shown.frame < frame);
    if new_action {
        host.started = tick;
    }
    host.last = Some((shown.action, shown.frame));
    host.other = Some(shown.clone());
    if press_test && new_action {
        let action = format!("{:?}", shown.action);
        game.event(serde_json::json!({"event":"duel_own_shown","tick":tick,"action":action}));
    }
    Some(shown)
}

fn online_press_test(game: &Game) -> bool {
    game.ext.online().press_test
}

/// The shown Sora in place of the simulated one, or back.
fn host_swap(game: &mut Game, shown: bool) {
    let Game {
        sora,
        action_started,
        ext,
        ..
    } = game;
    let host = &mut ext.online_mut().predict.host;
    if host.swapped == shown {
        return;
    }
    let Some(other) = host.other.as_mut() else {
        host.swapped = false;
        return;
    };
    std::mem::swap(&mut sora.player, other);
    std::mem::swap(action_started, &mut host.started);
    host.swapped = shown;
}

/// After the hits: effects, the HUD and the frames follow the shown Sora.
pub fn host_show(mut game: ResMut<Game>) {
    host_swap(&mut game, true);
}

/// Before the snapshot and the state's hash: what went out is the simulated one.
pub fn host_hide(mut game: ResMut<Game>) {
    host_swap(&mut game, false);
}

pub fn host_show_again(mut game: ResMut<Game>) {
    host_swap(&mut game, true);
}

/// The room's sound scan on the Sora this side moves: his swings are played
/// here, and the host's matching events dropped (`already_played`). His voices,
/// cloth and steps stay the host's: a voice's pick is a roll of the host's.
pub fn own_sounds(game: &mut Game) {
    if !active(game) || game.ext.online().predict.following {
        return;
    }
    let from = game.sound_queue.len();
    super::sora_sounds(game);
    let tick = game.tick;
    let mut swings = Vec::new();
    let mut i = 0;
    game.sound_queue.retain(|&(id, cause)| {
        let keep = i < from || cause == "swing";
        if i >= from && cause == "swing" {
            swings.push(id);
        }
        i += 1;
        keep
    });
    for id in swings {
        game.ext.online_mut().predict.swings.played(id, tick);
        game.event(serde_json::json!({"event":"duel_swing_local","tick":tick,"id":id}));
    }
}

/// A swing's sound the host says this side's Sora made: whether it was already
/// played here.
pub fn already_played(game: &mut Game, id: u32) -> bool {
    let tick = game.tick;
    let known = game.ext.online_mut().predict.swings.heard(id, tick);
    game.event(serde_json::json!({"event":"duel_swing_host","tick":tick,"id":id,"known":known}));
    known
}

/// Start of a fixed step: the drawn offset comes off, the simulation has its
/// own place back.
pub fn hide(game: &mut Game) {
    host_swap(game, false);
    let drawn = std::mem::take(&mut game.ext.online_mut().predict.drawn);
    if drawn != [0.; 3] {
        let p = &mut game.sora.player.position;
        for i in 0..3 {
            p[i] -= drawn[i];
        }
    }
}

/// End of a fixed step: what the frames draw is the offset's way home.
pub fn show(game: &mut Game) {
    let predict = &mut game.ext.online_mut().predict;
    if !predict.on {
        return;
    }
    predict.blend.tick();
    let drawn = predict.blend.offset;
    predict.drawn = drawn;
    let p = &mut game.sora.player.position;
    for i in 0..3 {
        p[i] += drawn[i];
    }
}

/// The joiner's seat in a snapshot carries his motion.
pub fn carries_motion(slot: u8) -> bool {
    slot == fighter_slots::JOIN
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A toy: a point on a line, moved by its stick, and a bump the host may
    /// have and this side does not.
    #[derive(Clone, Debug, PartialEq)]
    struct Toy {
        x: f32,
    }

    impl Predicted for Toy {
        type Truth = f32;
        fn position(&self) -> [f32; 3] {
            [self.x, 0., 0.]
        }
        fn agrees(&self, t: &f32) -> bool {
            (self.x - t).abs() < POSITION_TOLERANCE
        }
        fn restore(&mut self, t: &f32) {
            self.x = *t;
        }
    }

    fn pad(x: f32) -> RivalPad {
        RivalPad {
            direction: [x, 0.],
            ..RivalPad::default()
        }
    }

    fn step(s: &mut Toy, _tick: u32, p: &RivalPad) {
        s.x += p.direction[0] * 10.;
    }

    fn run(pads: &[f32]) -> (Ring<Toy>, Toy) {
        let mut ring = Ring::default();
        let mut now = Toy { x: 0. };
        for (i, &x) in pads.iter().enumerate() {
            step(&mut now, i as u32 + 1, &pad(x));
            ring.push(i as u32 + 1, pad(x), now.clone());
        }
        (ring, now)
    }

    #[test]
    fn agreeing_states_change_nothing() {
        let (mut ring, mut now) = run(&[1., 1., 1., 1.]);
        let before = now.clone();
        let mut steps = 0;
        // Tick 2 led to x = 20, and within the tolerance is as good.
        let out = ring.reconcile(2, &(20. + POSITION_TOLERANCE - 1.), &mut now, |s, t, p| {
            steps += 1;
            step(s, t, p);
        });
        assert_eq!(out, Outcome::Agreed);
        assert_eq!(now, before);
        assert_eq!(steps, 0);
    }

    #[test]
    fn one_input_that_differed_replays_the_later_ones() {
        // This side moved at tick 2; the host stood (it missed that button).
        let (mut ring, mut now) = run(&[1., 1., 1., 1.]);
        let out = ring.reconcile(2, &10., &mut now, step);
        assert_eq!(now.x, 30., "the host's 10, then ticks 3 and 4");
        assert_eq!(out, Outcome::Corrected { moved: 10. });
        // The replayed states agree with what the host says next.
        assert_eq!(ring.reconcile(4, &30., &mut now, step), Outcome::Agreed);
    }

    #[test]
    fn a_tick_not_there_yet_waits() {
        let (mut ring, mut now) = run(&[1., 1.]);
        let before = now.clone();
        assert_eq!(ring.reconcile(9, &0., &mut now, step), Outcome::Waiting);
        assert_eq!(now, before);
        let mut empty = Ring::<Toy>::default();
        assert_eq!(empty.reconcile(9, &0., &mut now, step), Outcome::Waiting);
    }

    #[test]
    fn a_tick_older_than_the_ring_takes_the_host_and_replays_all() {
        let (mut ring, mut now) = run(&[1., 1.]);
        let out = ring.reconcile(0, &100., &mut now, step);
        assert_eq!(now.x, 120.);
        assert_eq!(out, Outcome::Corrected { moved: 100. });
    }

    #[test]
    fn the_ring_keeps_its_length() {
        let (ring, _) = run(&[1.; 200]);
        assert_eq!(ring.entries.len(), RING);
        const { assert!(RING >= 32, "the brief's floor") };
    }

    #[test]
    fn a_blend_never_exceeds_its_bound_and_fades_in_its_ticks() {
        let mut b = Blend::default();
        b.start([50., 0., 0.], [0., 0., 0.]);
        let mut last = Vec3::from_array(b.offset).length();
        assert_eq!(last, 50.);
        for _ in 0..BLEND_TICKS {
            b.tick();
            let now = Vec3::from_array(b.offset).length();
            assert!(now < last || now == 0.);
            last = now;
        }
        assert_eq!(last, 0., "gone after its ticks");
        // Corrections pile up, up to the bound; past it he is drawn at once.
        let mut b = Blend::default();
        for _ in 0..200 {
            b.start([10., 0., 0.], [0., 0., 0.]);
            assert!(Vec3::from_array(b.offset).length() <= BLEND_MAX);
        }
        let mut far = Blend::default();
        far.start([BLEND_MAX + 1., 0., 0.], [0., 0., 0.]);
        assert_eq!(far, Blend::default());
    }

    #[test]
    fn corrections_count_a_rolling_minute() {
        let mut c = Corrections::default();
        c.note(10, 3.);
        c.note(20, 12.);
        assert_eq!(c.last_minute(20), (2, 12.));
        assert_eq!((c.total, c.large), (2, 1));
        assert_eq!(c.last_minute(10 + MINUTE), (1, 12.));
        assert_eq!(c.last_minute(20 + MINUTE), (0, 0.));
    }

    #[test]
    fn the_other_sora_is_carried_on_at_his_speed() {
        let mut f = FoeTrack::default();
        f.update(100, 50, [0., 0., 0.]);
        assert_eq!(f.at(55), [0., 0., 0.], "no speed known yet");
        f.update(102, 52, [16., 0., 0.]);
        assert_eq!(f.at(52), [16., 0., 0.]);
        assert_eq!(f.at(55), [40., 0., 0.]);
        assert_eq!(f.at(500), [16. + 8. * FOE_LEAD as f32, 0., 0.]);
        f.update(200, 60, [0., 0., 0.]);
        assert_eq!(
            f.at(70),
            [0., 0., 0.],
            "a long gap says nothing of his speed"
        );
    }

    #[test]
    fn a_swing_plays_once_on_either_side() {
        let mut s = Swings::default();
        s.played(7, 100);
        s.played(7, 110);
        assert!(s.heard(7, 105), "first the one played here");
        assert!(s.heard(7, 115), "then the second");
        assert!(!s.heard(7, 116), "a third is the host's alone");
        s.played(8, 100);
        assert!(!s.heard(9, 101), "another sound");
        assert!(
            !s.heard(8, 100 + SWING_WINDOW + 1),
            "too late to be that one"
        );
    }

    #[test]
    fn only_the_host_starts_what_the_host_runs() {
        assert!(host_driven(Action::Hurt(0)));
        assert!(host_driven(Action::Cast(0)));
        assert!(!host_driven(Action::Run));
        assert!(!host_driven(Action::Attack(3)));
        assert!(!host_driven(Action::Jump));
        assert!(!host_driven(Action::Guard));
    }
}
