//! The player's automatic target, translated from the PC executable: the
//! chooser FUN_1403be4b0 (mode 6 of FUN_1403be2b0) and the target holder's
//! automatic states in FUN_1403dcd70 / FUN_1403dd650 / FUN_1403dccd0.
//!
//! The parameters come from the player's 03system `plyr` record (obj+0x150):
//! +0x5c range, +0x60 / +0x64 Y-down height band, +0x68 hold ticks, +0x6c
//! height weight, +0x70 angle weight. Heights here are Y-up.

use crate::Vec3;
use crate::ptya::Target;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub range: f32,
    pub dy_min: f32,
    pub dy_max: f32,
    pub hold: f32,
    pub height_weight: f32,
    pub angle_weight: f32,
}

impl Params {
    /// Reads the fields from a `plyr` record (None when short).
    pub fn read(record: &[u8]) -> Option<Self> {
        let f = |at: usize| {
            record
                .get(at..at + 4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        };
        Some(Params {
            range: f(0x5c)?,
            dy_min: f(0x60)?,
            dy_max: f(0x64)?,
            hold: f(0x68)?,
            height_weight: f(0x6c)?,
            angle_weight: f(0x70)?,
        })
    }
}

/// `DAT_142a1141c` (FUN_1403aba10), 1.0 after FUN_1403abab0.
pub const SCALE: f32 = 1.;
/// `DAT_140623a4c`.
const HALF: f32 = 0.5;

/// FUN_1403be4b0: the lowest-scoring candidate shape, by index. `range` 0
/// or less skips the band and range tests.
pub fn choose(
    p: &Params,
    origin: Vec3,
    facing: [f32; 2],
    candidates: &[Target],
    range: f32,
) -> Option<usize> {
    let (mut dy_min, mut dy_max) = (p.dy_min, p.dy_max);
    if 1. < SCALE {
        dy_min *= SCALE;
        dy_max *= SCALE;
    }
    let mut best: Option<(usize, f32)> = None;
    for (i, t) in candidates.iter().enumerate() {
        let v = [t.centre[0] - origin[0], t.centre[2] - origin[2]];
        let dy = origin[1] - t.centre[1];
        let d = v[0].hypot(v[1]);
        let dir = if d != 0. { [v[0] / d, v[1] / d] } else { v };
        if range > 0.
            && !(dy_min - t.height <= dy && dy <= t.height + dy_max && d - t.radius <= range)
        {
            continue;
        }
        let cos = facing[0] * dir[0] + facing[1] * dir[1];
        let score = (d + dy.abs() * p.height_weight)
            * ((1. - cos) * HALF * (1. - p.angle_weight) + p.angle_weight);
        if best.is_none_or(|(_, s)| score < s) {
            best = Some((i, score));
        }
    }
    best.map(|(i, _)| i)
}

/// The holder's automatic states: 0 none, 1 automatic target.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Holder {
    /// The held candidate (an index the caller keeps stable).
    pub target: Option<usize>,
    /// +0x20: ticks the target is held without re-choosing.
    pub timer: f32,
}

impl Holder {
    /// FUN_1403dcd70 states 0 and 1, once per frame. `valid` says whether
    /// a candidate index can still be targeted (FUN_1403bde30).
    pub fn update(
        &mut self,
        p: &Params,
        origin: Vec3,
        facing: [f32; 2],
        candidates: &[Target],
        valid: impl Fn(usize) -> bool,
        dt: f32,
    ) {
        if let Some(t) = self.target
            && 0. < self.timer
            && valid(t)
        {
            self.timer -= dt;
            return;
        }
        let found = choose(p, origin, facing, candidates, SCALE * p.range).filter(|&i| valid(i));
        match (self.target, found) {
            // State 0 → 1 (FUN_1403dd650 with timer 0) or a switch.
            (_, Some(i)) => {
                if self.target != Some(i) {
                    self.timer = 0.;
                }
                self.target = Some(i);
            }
            // Nothing in range: released (FUN_1403dd510).
            (_, None) => *self = Holder::default(),
        }
    }

    /// FUN_1403dccd0: an attack turning to the target holds it for the
    /// `plyr` hold time.
    pub fn hold(&mut self, p: &Params) {
        if self.target.is_some() {
            self.timer = p.hold;
        }
    }

    /// FUN_1403aa560: when an attack starts without a target, take the best
    /// candidate at any range and hold it.
    pub fn acquire(
        &mut self,
        p: &Params,
        origin: Vec3,
        facing: [f32; 2],
        candidates: &[Target],
        valid: impl Fn(usize) -> bool,
    ) {
        if self.target.is_some_and(&valid) {
            return;
        }
        if let Some(i) = choose(p, origin, facing, candidates, 0.).filter(|&i| valid(i)) {
            self.target = Some(i);
            self.timer = p.hold;
        }
    }
}

/// Actor/shape identity belongs to the host rather than the chooser's array
/// index. This layer keeps a held target stable when other actors disappear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate<K> {
    pub key: K,
    pub shape: Target,
}

/// Inputs of the distinct native manual choosers. The owner origin and the
/// actor's range-bypass predicate must be resolved by the host, independently
/// of the automatic chooser's facing/height parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManualCandidate<K> {
    pub candidate: Candidate<K>,
    pub bypass_range: bool,
}

fn length(v: Vec3) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

pub fn choose_manual<K: Copy>(
    origin: Vec3,
    range: f32,
    candidates: &[ManualCandidate<K>],
) -> Option<K> {
    let mut best: Option<(K, f32)> = None;
    for candidate in candidates {
        let shape = candidate.candidate.shape;
        let distance =
            length([0, 1, 2].map(|axis| shape.centre[axis] - origin[axis])) - shape.radius;
        if !candidate.bypass_range && range < distance {
            continue;
        }
        if best.is_none_or(|(_, prior)| distance < prior) {
            best = Some((candidate.candidate.key, distance));
        }
    }
    best.map(|(key, _)| key)
}

pub fn cycle_manual<K: Copy + Eq>(
    origin: Vec3,
    range: f32,
    current: K,
    direction: f32,
    candidates: &[ManualCandidate<K>],
) -> Option<K> {
    let held = candidates
        .iter()
        .find(|candidate| candidate.candidate.key == current)?;
    let held = held.candidate.shape.centre;
    let angle = (held[0] - origin[0]).atan2(held[2] - origin[2]);
    let mut best: Option<(K, f32)> = None;
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.candidate.key != current)
    {
        let shape = candidate.candidate.shape;
        let delta = [0, 1, 2].map(|axis| shape.centre[axis] - origin[axis]);
        let distance = length(delta);
        if !candidate.bypass_range && range < distance - shape.radius {
            continue;
        }
        let normalized = if distance != 0. {
            delta.map(|value| value * (1. / distance))
        } else {
            delta
        };
        let difference =
            crate::follow_camera::wrap(normalized[0].atan2(normalized[2]) - angle) * direction;
        if difference > 0. && best.is_none_or(|(_, prior)| difference < prior) {
            best = Some((candidate.candidate.key, difference));
        }
    }
    best.map(|(key, _)| key)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    None,
    Automatic,
    Locked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StableHolder<K> {
    pub state: State,
    pub target: Option<K>,
    pub timer: f32,
}

impl<K> Default for StableHolder<K> {
    fn default() -> Self {
        Self {
            state: State::None,
            target: None,
            timer: 0.,
        }
    }
}

/// Native search modes 4 and cycling use separate camera-dependent choosers.
/// Their qualified result is supplied by the host, never replaced with the
/// automatic chooser or authored list order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ManualInput<K> {
    pub toggle: bool,
    pub initial_choice: Option<K>,
    pub cycle_choice: Option<K>,
}

impl<K: Copy + Eq> StableHolder<K> {
    pub fn index(&self, candidates: &[Candidate<K>]) -> Option<usize> {
        let key = self.target?;
        candidates.iter().position(|candidate| candidate.key == key)
    }

    /// Holder states 0/1/2: pressing lock with an automatic target preserves
    /// that exact shape. Another press or invalid actor releases manual lock.
    /// Candidate filtering remains the host's native actor/shape predicate.
    pub fn update(
        &mut self,
        p: &Params,
        origin: Vec3,
        facing: [f32; 2],
        candidates: &[Candidate<K>],
        manual: ManualInput<K>,
        dt: f32,
    ) {
        let valid = |key: K| candidates.iter().any(|candidate| candidate.key == key);
        match self.state {
            State::Locked => {
                if manual.toggle || self.target.is_none_or(|key| !valid(key)) {
                    *self = Self::default();
                } else if let Some(key) = manual.cycle_choice.filter(|&key| valid(key)) {
                    self.target = Some(key);
                    self.timer = 0.;
                }
                return;
            }
            State::Automatic if manual.toggle => {
                self.state = State::Locked;
                self.timer = 0.;
                self.target = self.target.filter(|&key| valid(key));
                return;
            }
            State::None if manual.toggle => {
                if let Some(key) = manual.initial_choice.filter(|&key| valid(key)) {
                    self.state = State::Locked;
                    self.target = Some(key);
                    self.timer = 0.;
                }
                return;
            }
            _ => {}
        }
        let shapes = candidates
            .iter()
            .map(|candidate| candidate.shape)
            .collect::<Vec<_>>();
        let mut automatic = Holder {
            target: self.index(candidates),
            timer: self.timer,
        };
        automatic.update(p, origin, facing, &shapes, |_| true, dt);
        self.target = automatic.target.map(|index| candidates[index].key);
        self.timer = automatic.timer;
        self.state = if self.target.is_some() {
            State::Automatic
        } else {
            State::None
        };
    }

    pub fn hold(&mut self, p: &Params) {
        if self.state == State::Automatic && self.target.is_some() {
            self.timer = p.hold;
        }
    }

    pub fn acquire(
        &mut self,
        p: &Params,
        origin: Vec3,
        facing: [f32; 2],
        candidates: &[Candidate<K>],
    ) {
        if self.state == State::Locked {
            return;
        }
        let shapes = candidates
            .iter()
            .map(|candidate| candidate.shape)
            .collect::<Vec<_>>();
        let mut automatic = Holder {
            target: self.index(candidates),
            timer: self.timer,
        };
        automatic.acquire(p, origin, facing, &shapes, |_| true);
        self.target = automatic.target.map(|index| candidates[index].key);
        self.timer = automatic.timer;
        self.state = if self.target.is_some() {
            State::Automatic
        } else {
            State::None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params {
            range: 300.,
            dy_min: -435.,
            dy_max: 50.,
            hold: 60.,
            height_weight: 0.4,
            angle_weight: 0.25,
        }
    }

    fn at(x: f32, z: f32) -> Target {
        Target {
            centre: [x, 50., z],
            radius: 45.,
            height: 45.,
        }
    }

    #[test]
    fn ahead_beats_behind_and_range_limits() {
        let p = params();
        let c = [at(0., -150.), at(0., 200.)];
        assert_eq!(choose(&p, [0.; 3], [0., 1.], &c, 300.), Some(1));
        let far = [at(0., 400.)];
        assert_eq!(choose(&p, [0.; 3], [0., 1.], &far, 300.), None);
        assert_eq!(choose(&p, [0.; 3], [0., 1.], &far, 0.), Some(0));
    }

    #[test]
    fn holding_keeps_a_target_out_of_range_until_the_timer_runs_out() {
        let p = params();
        let mut h = Holder::default();
        let near = [at(0., 100.)];
        h.update(&p, [0.; 3], [0., 1.], &near, |_| true, 1.);
        assert_eq!(h.target, Some(0));
        h.hold(&p);
        let gone = [at(0., 900.)];
        for _ in 0..60 {
            h.update(&p, [0.; 3], [0., 1.], &gone, |_| true, 1.);
            assert_eq!(h.target, Some(0));
        }
        h.update(&p, [0.; 3], [0., 1.], &gone, |_| true, 1.);
        assert_eq!(h.target, None);
    }

    fn manual(toggle: bool) -> ManualInput<u64> {
        ManualInput {
            toggle,
            initial_choice: None,
            cycle_choice: None,
        }
    }

    #[test]
    fn stable_lock_keeps_actor_identity_when_an_earlier_actor_is_removed() {
        let mut holder = StableHolder::default();
        let p = params();
        let all = [
            Candidate {
                key: 11,
                shape: at(0., -200.),
            },
            Candidate {
                key: 22,
                shape: at(0., 100.),
            },
        ];
        holder.update(&p, [0.; 3], [0., 1.], &all, manual(false), 1.);
        holder.update(&p, [0.; 3], [0., 1.], &all, manual(true), 1.);
        assert_eq!(holder.state, State::Locked);
        assert_eq!(holder.target, Some(22));
        holder.update(&p, [0.; 3], [0., -1.], &all[1..], manual(false), 1.);
        assert_eq!(holder.index(&all[1..]), Some(0));
        assert_eq!(holder.target, Some(22));
        holder.update(&p, [0.; 3], [0., 1.], &all[..1], manual(false), 1.);
        assert_eq!(holder, StableHolder::default());
    }

    #[test]
    fn lock_toggle_preserves_automatic_shape_then_releases_without_rechoosing() {
        let p = params();
        let mut holder = StableHolder::default();
        let all = [
            Candidate {
                key: 11,
                shape: at(0., 100.),
            },
            Candidate {
                key: 22,
                shape: at(0., -100.),
            },
        ];
        holder.update(&p, [0.; 3], [0., 1.], &all, manual(false), 1.);
        holder.update(&p, [0.; 3], [0., -1.], &all, manual(true), 1.);
        assert_eq!(holder.target, Some(11));
        holder.acquire(&p, [0.; 3], [0., -1.], &all);
        assert_eq!(holder.target, Some(11));
        holder.update(&p, [0.; 3], [0., -1.], &all, manual(true), 1.);
        assert_eq!(holder, StableHolder::default());
    }

    #[test]
    fn manual_search_and_cycle_use_supplied_native_choice_not_array_order() {
        let p = params();
        let all = [
            Candidate {
                key: 11,
                shape: at(0., 100.),
            },
            Candidate {
                key: 22,
                shape: at(0., -100.),
            },
        ];
        let mut holder = StableHolder::default();
        holder.update(
            &p,
            [0.; 3],
            [0., 1.],
            &all,
            ManualInput {
                toggle: true,
                initial_choice: Some(22),
                cycle_choice: None,
            },
            1.,
        );
        assert_eq!(holder.target, Some(22));
        holder.update(
            &p,
            [0.; 3],
            [0., 1.],
            &all,
            ManualInput {
                toggle: false,
                initial_choice: None,
                cycle_choice: Some(999),
            },
            1.,
        );
        assert_eq!(holder.target, Some(22));
        holder.update(
            &p,
            [0.; 3],
            [0., 1.],
            &all,
            ManualInput {
                toggle: false,
                initial_choice: None,
                cycle_choice: Some(11),
            },
            1.,
        );
        assert_eq!(holder.target, Some(11));
    }

    fn manual_candidate(key: u64, centre: Vec3, radius: f32) -> ManualCandidate<u64> {
        ManualCandidate {
            candidate: Candidate {
                key,
                shape: Target {
                    centre,
                    radius,
                    height: radius,
                },
            },
            bypass_range: false,
        }
    }

    #[test]
    fn initial_manual_search_uses_three_dimensional_surface_distance() {
        let candidates = [
            manual_candidate(11, [0., 0., 200.], 10.),
            manual_candidate(22, [0., 0., -100.], 10.),
            manual_candidate(33, [0., 900., 0.], 10.),
        ];
        assert_eq!(choose_manual([0.; 3], 300., &candidates), Some(22));
        let at_boundary = [manual_candidate(44, [0., 0., 400.], 100.)];
        assert_eq!(choose_manual([0.; 3], 300., &at_boundary), Some(44));
        let mut far = [manual_candidate(55, [0., 0., 1000.], 10.)];
        assert_eq!(choose_manual([0.; 3], 300., &far), None);
        far[0].bypass_range = true;
        assert_eq!(choose_manual([0.; 3], 300., &far), Some(55));
    }

    #[test]
    fn cycle_respects_signed_angle_and_does_not_wrap_to_the_opposite_half() {
        let candidates = [
            manual_candidate(11, [0., 0., 100.], 1.),
            manual_candidate(22, [100., 0., 0.], 1.),
            manual_candidate(33, [-100., 0., 0.], 1.),
        ];
        assert_eq!(cycle_manual([0.; 3], 300., 11, 1., &candidates), Some(22));
        assert_eq!(cycle_manual([0.; 3], 300., 11, -1., &candidates), Some(33));
        let no_right = [candidates[0], candidates[2]];
        assert_eq!(cycle_manual([0.; 3], 300., 11, 1., &no_right), None);
    }
}
