//! Which PTYA attack a battler plays, translated from the PC executable:
//! Sora's attack command FUN_1403a8690, the context FUN_1404028d0, the
//! chooser FUN_140402d20 with FUN_1404029f0 / FUN_1404027f0 /
//! FUN_140402c50, the record gate FUN_140402750, the combo counters of
//! FUN_1403d6ca0 and the limits FUN_1403d7850 / FUN_1403d7780.
//!
//! Records come from 00battle `ptya` (the set at obj+0xd50, 0x44 bytes
//! each) and are scanned in file order: the first marked record wins, so
//! the data's order is its priority. Heights here are Y-up; the executable
//! works Y-down, so its "dy" is the negated rise.

use crate::Vec3;
use crate::abilities::{Abilities, bit};

/// +4 flag bits.
pub mod flag {
    /// The record is an air attack: it is allowed only while airborne.
    pub const AIR: u32 = 1;
    /// FUN_1404027f0 lowers the height floor by the jump height
    /// (FUN_1403b5a40); FUN_1403d78d0 raises the hop cap by it.
    pub const JUMP_REACH: u32 = 2;
    /// A finisher: allowed once the combo count reaches the limit
    /// (FUN_1403d7780) instead of while below it (FUN_1403d7850).
    pub const FINISHER: u32 = 4;
    /// The target's height is subtracted from the ceiling instead of added.
    pub const CEILING_IN: u32 = 8;
    /// The target's height is added to the floor instead of subtracted.
    pub const FLOOR_IN: u32 = 0x10;
}

/// +1 selection kind.
pub mod kind {
    /// Only by id (guard, counters, Square abilities).
    pub const MANUAL: u8 = 0;
    /// Against the current target only.
    pub const TARGET: u8 = 1;
    /// Against any enemy's target shapes.
    pub const ANY: u8 = 2;
    /// Always considered.
    pub const ALWAYS: u8 = 3;
}

/// One PTYA record (0x44 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Record {
    /// +0.
    pub id: u8,
    /// +1, see [`kind`].
    pub kind: u8,
    /// +2: the record used instead when this one would repeat the current
    /// motion (−1: none).
    pub alternate: i8,
    /// +3: added to the combo count by the finisher test.
    pub combo_offset: i8,
    /// +4, see [`flag`].
    pub flags: u32,
    /// +8.
    pub motion: u16,
    /// +0xa: FUN_1403c8310's second argument.
    pub blend_arg: u16,
    /// +0xc: hop height (0: no hop).
    pub jump: f32,
    /// +0x10 / +0x14: hop height limits toward a target.
    pub jump_max: f32,
    pub jump_min: f32,
    /// +0x18 / +0x1c: hop speed limits.
    pub speed_min: f32,
    pub speed_max: f32,
    /// +0x20 / +0x24: horizontal distance band (minus the target's radius),
    /// `near <= d < far`.
    pub near: f32,
    pub far: f32,
    /// +0x28 / +0x2c: Y-down height band of the target relative to the
    /// battler, widened by the target's height (`low < dy <= high`).
    pub high: f32,
    pub low: f32,
    /// +0x30 / +0x34: cosine band of the target's direction against the
    /// facing (`cos_min < c <= cos_max`).
    pub cos_min: f32,
    pub cos_max: f32,
    /// +0x38: blend into the next motion.
    pub blend: f32,
    /// +0x3c: hop distance adjustment.
    pub reach: f32,
    /// +0x40: required ability bit (0: none).
    pub ability: u16,
    /// +0x42.
    pub w42: u16,
}

impl Record {
    pub const SIZE: usize = 0x44;

    pub fn read(bytes: &[u8]) -> Option<Self> {
        let b = bytes.get(..Self::SIZE)?;
        let f = |at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let h = |at: usize| u16::from_le_bytes([b[at], b[at + 1]]);
        Some(Record {
            id: b[0],
            kind: b[1],
            alternate: b[2] as i8,
            combo_offset: b[3] as i8,
            flags: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            motion: h(8),
            blend_arg: h(0xa),
            jump: f(0xc),
            jump_max: f(0x10),
            jump_min: f(0x14),
            speed_min: f(0x18),
            speed_max: f(0x1c),
            near: f(0x20),
            far: f(0x24),
            high: f(0x28),
            low: f(0x2c),
            cos_min: f(0x30),
            cos_max: f(0x34),
            blend: f(0x38),
            reach: f(0x3c),
            ability: h(0x40),
            w42: h(0x42),
        })
    }

    pub fn air(&self) -> bool {
        self.flags & flag::AIR != 0
    }

    /// Whether starting it leaves the ground: an air attack, or a ground
    /// record with a hop (FUN_1403d78d0 with +0xc > 0).
    pub fn leaves_ground(&self) -> bool {
        self.air() || self.jump > 0.
    }
}

/// FUN_1403ebee0: the first record with this id.
pub fn find(records: &[Record], id: u8) -> Option<usize> {
    records.iter().position(|r| r.id == id)
}

/// The combo counters obj+0xd74 (combo step) and obj+0xd75 (finisher
/// step), both capped at 100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Combo {
    pub step: u8,
    pub finish: u8,
}

/// Battler state the gates read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State<'a> {
    pub abilities: &'a Abilities,
    /// FUN_1403ba9a0.
    pub airborne: bool,
    /// MP is charging (stats +0x1c0 > 0, FUN_1403c1d10).
    pub mp_charging: bool,
    /// ANB range 11, "allow combo" (obj+0x18c bit 10).
    pub combo_window: bool,
    /// ANB range 50 (obj+0x194 bit 5): a finisher may chain.
    pub finisher_window: bool,
    /// ANB ranges 0, 1 or 2 (obj+0x18c & 0x1c).
    pub actionable: bool,
}

impl State<'_> {
    /// Stats +0x1dc bit 11 with MP charging: no combo limit.
    fn berserk(&self) -> bool {
        self.abilities.has(bit::BERSERK) && self.mp_charging
    }
    fn limit(&self) -> i32 {
        if self.airborne {
            self.abilities.air_combo as i32
        } else {
            self.abilities.ground_combo as i32
        }
    }
}

/// FUN_1403d7850: the combo has reached its limit (finishers only).
pub fn limit_reached(s: &State, c: Combo) -> bool {
    if s.berserk() {
        return false;
    }
    if !s.finisher_window && (c.step as i32) < s.limit() {
        return false;
    }
    c.finish > 1
}

/// FUN_1403d7780: a finisher record (offset +3) may play.
pub fn finisher_allowed(s: &State, c: Combo, offset: i8) -> bool {
    if s.berserk() {
        return false;
    }
    if s.finisher_window {
        return true;
    }
    s.limit() <= c.step as i32 + offset as i32
}

/// FUN_140402750: ability, ground/air and combo gates for one record.
pub fn allowed(r: &Record, s: &State, c: Combo) -> bool {
    if r.motion == 0 {
        return false;
    }
    if r.ability != 0 && !s.abilities.has(r.ability) {
        return false;
    }
    if s.airborne != r.air() {
        return false;
    }
    if r.flags & flag::FINISHER == 0 {
        !limit_reached(s, c)
    } else {
        finisher_allowed(s, c, r.combo_offset)
    }
}

/// FUN_1403d6ca0: counts an attack press. `chain` is whether the last
/// attack may be continued (it connected, obj+0x120 bit 25, or Combo
/// Master). Returns whether the attack starts at all.
pub fn advance(c: &mut Combo, s: &State, chain: bool) -> bool {
    if s.finisher_window && chain && c.finish < s.abilities.finishers {
        if c.step < 100 {
            c.step += 1;
        }
        if c.finish < 100 {
            c.finish += 1;
        }
        return true;
    }
    if s.combo_window && chain {
        if c.step < 100 {
            c.step += 1;
        }
        return true;
    }
    if !s.actionable {
        return false;
    }
    *c = Combo { step: 1, finish: 1 };
    true
}

/// A collision shape the chooser measures against (FUN_1403c9c00's world
/// shape): centre (Y-up), radius and height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Target {
    pub centre: Vec3,
    pub radius: f32,
    pub height: f32,
}

/// Where the battler is and what is around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene<'a> {
    /// obj+0x670 (feet position, Y-up).
    pub origin: Vec3,
    /// Facing unit vector in XZ (obj+0x680).
    pub facing: [f32; 2],
    /// The current target's shape (obj+0xdc8 target), if valid.
    pub target: Option<Target>,
    /// Every attackable object's TARGET shapes (obj+0x6c8 bit 17).
    pub others: &'a [Target],
    /// FUN_1403b5a40: the jump height (obj+0x130, or the High Jump level's
    /// height).
    pub jump_height: f32,
    /// obj+0x180.
    pub current_motion: u16,
}

/// FUN_1404027f0: the distance, height and angle bands.
fn in_bands(r: &Record, dist: f32, dy: f32, cos: f32, height: f32, jump_height: f32) -> bool {
    let mut low = r.low;
    if r.flags & flag::JUMP_REACH != 0 {
        low -= jump_height;
    }
    let high = if r.flags & flag::CEILING_IN == 0 {
        r.high + height
    } else {
        r.high - height
    };
    let low = if r.flags & flag::FLOOR_IN == 0 {
        low - height
    } else {
        low + height
    };
    !(dist < r.near
        || r.far <= dist
        || high < dy
        || dy <= low
        || r.cos_max < cos
        || cos <= r.cos_min)
}

/// FUN_1404029f0: marks the records of one kind that fit this shape.
#[allow(clippy::too_many_arguments)]
fn mark_for(
    records: &[Record],
    marks: &mut [bool],
    kind: u8,
    t: &Target,
    origin: Vec3,
    forward: [f32; 2],
    jump_height: f32,
    s: &State,
    c: Combo,
) {
    let v = [t.centre[0] - origin[0], t.centre[2] - origin[2]];
    // Y-down rise of the shape's centre over the feet.
    let dy = origin[1] - t.centre[1];
    let len = v[0].hypot(v[1]);
    let dist = (len - t.radius).max(0.);
    let dir = if len != 0. {
        [v[0] / len, v[1] / len]
    } else {
        v
    };
    let cos = (dir[0] * forward[0] + dir[1] * forward[1]).min(1.);
    for (r, m) in records.iter().zip(marks.iter_mut()) {
        if r.kind == kind && allowed(r, s, c) && in_bands(r, dist, dy, cos, t.height, jump_height) {
            *m = true;
        }
    }
}

/// FUN_140402c50: the first marked record, unless it would repeat the
/// current motion; then its alternate (by id) is kept as a fallback while
/// later marked records are still tried.
fn pick(records: &[Record], marks: &[bool], current_motion: u16) -> Option<usize> {
    let mut fallback = None;
    for (i, r) in records.iter().enumerate() {
        if !marks[i] {
            continue;
        }
        if r.alternate as i32 == r.id as i32 || r.motion as u32 != current_motion as u32 {
            return Some(i);
        }
        if fallback.is_none() && r.alternate >= 0 {
            fallback = find(records, r.alternate as u8);
        }
    }
    fallback
}

/// FUN_140402d20 with FUN_1404028d0's context: the attack to play.
/// `probe_blocked` is FUN_1403ba750's result for the grounded probe that
/// selects record 1 [U: what the probe's result bit means].
pub fn select(
    records: &[Record],
    s: &State,
    c: Combo,
    scene: &Scene,
    probe_blocked: bool,
) -> Option<usize> {
    let mut marks = vec![false; records.len()];
    // The facing used for the angle bands: toward the target when there is
    // one, else the battler's own facing.
    let forward = match scene.target {
        Some(t) => {
            let v = [t.centre[0] - scene.origin[0], t.centre[2] - scene.origin[2]];
            let len = v[0].hypot(v[1]);
            if len != 0. {
                [v[0] / len, v[1] / len]
            } else {
                v
            }
        }
        None => scene.facing,
    };
    for t in scene.others {
        mark_for(
            records,
            &mut marks,
            kind::ANY,
            t,
            scene.origin,
            forward,
            scene.jump_height,
            s,
            c,
        );
    }
    if let Some(t) = scene.target {
        mark_for(
            records,
            &mut marks,
            kind::TARGET,
            &t,
            scene.origin,
            forward,
            scene.jump_height,
            s,
            c,
        );
    }
    for (r, m) in records.iter().zip(marks.iter_mut()) {
        if r.kind == kind::ALWAYS && allowed(r, s, c) {
            *m = true;
        }
    }
    if !s.airborne
        && probe_blocked
        && let Some(i) = find(records, 1)
    {
        marks[i] = true;
    }
    if let Some(i) = pick(records, &marks, scene.current_motion) {
        return Some(i);
    }
    // Nothing fits: the basic ground (5) or air (9) attack, ungated.
    let i = find(records, if s.airborne { 9 } else { 5 })?;
    marks[i] = true;
    pick(records, &marks, scene.current_motion)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: u8, kind: u8, alternate: i8, flags: u32, motion: u16) -> Record {
        Record {
            id,
            kind,
            alternate,
            flags,
            motion,
            cos_min: 0.99996,
            cos_max: 1.,
            ..Record::default()
        }
    }

    /// Sora's ground chain without a target: records 6 (0x1b), 8 (3),
    /// 21 (5) and the fallback 6 of 5's alternate.
    fn sora() -> Vec<Record> {
        let mut leap = rec(0x1b, kind::ALWAYS, -1, flag::FINISHER, 165);
        leap.ability = 93;
        let mut near = rec(4, kind::ANY, -1, 0, 152);
        (near.far, near.high, near.low) = (180., -20., -110.);
        (near.cos_min, near.cos_max) = (-std::f32::consts::FRAC_1_SQRT_2, 0.92388);
        let mut dash = rec(6, kind::TARGET, -1, 0, 153);
        (dash.near, dash.far, dash.high, dash.low) = (180., 300., -20., -110.);
        vec![
            leap,
            rec(3, kind::ALWAYS, 2, flag::FINISHER, 155),
            rec(2, kind::ANY, -1, flag::FINISHER, 154),
            dash,
            near,
            rec(5, kind::ALWAYS, 6, 0, 151),
            rec(9, kind::ALWAYS, 10, flag::AIR, 181),
            rec(10, kind::TARGET, -1, flag::AIR | 8, 182),
        ]
    }

    fn state(a: &Abilities) -> State<'_> {
        State {
            abilities: a,
            airborne: false,
            mp_charging: false,
            combo_window: false,
            finisher_window: false,
            actionable: true,
        }
    }

    fn scene(motion: u16) -> Scene<'static> {
        Scene {
            origin: [0.; 3],
            facing: [0., 1.],
            target: None,
            others: &[],
            jump_height: 160.,
            current_motion: motion,
        }
    }

    #[test]
    fn whiffs_alternate_and_hits_reach_the_finisher() {
        let a = Abilities::default();
        let records = sora();
        let mut c = Combo::default();
        let mut s = state(&a);
        // First press: fresh combo, record 5 (motion 151).
        assert!(advance(&mut c, &s, false));
        assert_eq!(c, Combo { step: 1, finish: 1 });
        let i = select(&records, &s, c, &scene(0), false).unwrap();
        assert_eq!(records[i].motion, 151);
        // A press in the combo window after a hit: step 2, and 5 would
        // repeat 151, so its alternate 6 (motion 153) plays.
        s.combo_window = true;
        s.actionable = false;
        assert!(advance(&mut c, &s, true));
        let i = select(&records, &s, c, &scene(151), false).unwrap();
        assert_eq!(records[i].motion, 153);
        // Third: the step reaches the limit 3, so the finisher (155).
        assert!(advance(&mut c, &s, true));
        assert_eq!(c.step, 3);
        let i = select(&records, &s, c, &scene(153), false).unwrap();
        assert_eq!(records[i].motion, 155);
        // A miss cannot chain inside the window without range 0–2.
        let mut d = Combo { step: 1, finish: 1 };
        assert!(!advance(&mut d, &s, false));
    }

    #[test]
    fn finishing_plus_chains_the_finisher_into_its_alternate() {
        let a = Abilities {
            finishers: 2,
            ..Abilities::default()
        };
        let records = sora();
        let mut s = state(&a);
        s.finisher_window = true;
        s.actionable = false;
        let mut c = Combo { step: 3, finish: 1 };
        assert!(advance(&mut c, &s, true));
        assert_eq!(c, Combo { step: 4, finish: 2 });
        // 3 would repeat 155: its alternate 2 (154) is the fallback.
        let i = select(&records, &s, c, &scene(155), false).unwrap();
        assert_eq!(records[i].motion, 154);
        // Without the ability the window does not chain.
        let b = Abilities::default();
        let s2 = State { abilities: &b, ..s };
        let mut c2 = Combo { step: 3, finish: 1 };
        assert!(!advance(&mut c2, &s2, true));
    }

    #[test]
    fn target_bands_pick_the_dash_and_side_swings() {
        let a = Abilities::default();
        let records = sora();
        let s = state(&a);
        let c = Combo { step: 1, finish: 1 };
        // A target 250 ahead (centre 50 up): the dash (6, motion 153).
        let far = Target {
            centre: [0., 50., 250.],
            radius: 10.,
            height: 20.,
        };
        let sc = Scene {
            target: Some(far),
            others: &[],
            ..scene(0)
        };
        assert_eq!(
            records[select(&records, &s, c, &sc, false).unwrap()].motion,
            153
        );
        // A shape 100 away to the side while facing +Z with no target:
        // record 4 (motion 152).
        let side = [Target {
            centre: [100., 50., 20.],
            radius: 10.,
            height: 20.,
        }];
        let sc = Scene {
            target: None,
            others: &side,
            ..scene(0)
        };
        assert_eq!(
            records[select(&records, &s, c, &sc, false).unwrap()].motion,
            152
        );
        // The ability gate: 0x1b needs bit 93.
        let mut b = Abilities::default();
        b.set(93);
        let s3 = state(&b);
        let fin = Combo { step: 3, finish: 1 };
        assert_eq!(
            records[select(&records, &s3, fin, &scene(0), false).unwrap()].motion,
            165
        );
    }

    #[test]
    fn berserk_lifts_the_limit_while_mp_charges() {
        let mut a = Abilities::default();
        a.set(bit::BERSERK);
        let records = sora();
        let mut s = state(&a);
        s.mp_charging = true;
        let c = Combo { step: 5, finish: 5 };
        assert!(!limit_reached(&s, c));
        assert!(!finisher_allowed(&s, c, 0));
        let i = select(&records, &s, c, &scene(151), false).unwrap();
        assert_eq!(records[i].motion, 153);
    }

    #[test]
    fn airborne_uses_the_air_set_and_falls_back_to_nine() {
        let a = Abilities::default();
        let records = sora();
        let mut s = state(&a);
        s.airborne = true;
        let c = Combo { step: 1, finish: 1 };
        let i = select(&records, &s, c, &scene(4), false).unwrap();
        assert_eq!(records[i].motion, 181);
        // Repeating 181: 9's alternate 10 (182).
        let i = select(&records, &s, c, &scene(181), false).unwrap();
        assert_eq!(records[i].motion, 182);
    }

    #[test]
    fn record_bytes_round_trip() {
        let mut b = [0u8; 0x44];
        b[0] = 5;
        b[1] = 3;
        b[2] = 6;
        b[8..10].copy_from_slice(&151u16.to_le_bytes());
        b[0x30..0x34].copy_from_slice(&1f32.to_le_bytes());
        b[0x40..0x42].copy_from_slice(&97u16.to_le_bytes());
        let r = Record::read(&b).unwrap();
        assert_eq!(
            (r.id, r.kind, r.alternate, r.motion, r.ability),
            (5, 3, 6, 151, 97)
        );
        assert_eq!(r.cos_min, 1.);
        assert!(Record::read(&b[..10]).is_none());
    }
}
