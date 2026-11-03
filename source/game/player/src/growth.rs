//! The growth abilities (High Jump, Quick Run, Aerial Dodge, Glide, Dodge
//! Roll), translated from the PC executable: the level accessor
//! FUN_140401680, the parameter rows FUN_1403e92d0 (03system `pref/fmab`),
//! the Square handler's growth branches (FUN_1403a81f0), the jump height
//! FUN_1403b59b0, Aerial Dodge FUN_1403cb9f0, and the Quick Run and Glide
//! physics modes 0x10..0x12 (FUN_140423480 .. FUN_140423b90).

/// Family indices into `Abilities::growth` (`DAT_1405c9f00` order).
pub const HIGH_JUMP: usize = 0;
pub const QUICK_RUN: usize = 1;
pub const AERIAL_DODGE: usize = 2;
pub const GLIDE: usize = 3;
pub const DODGE_ROLL: usize = 4;

/// The abilities' motions.
pub mod motion {
    pub const HIGH_JUMP: u16 = 201;
    pub const AERIAL_DODGE: u16 = 202;
    pub const QUICK_RUN: u16 = 203;
    pub const QUICK_RUN_END: u16 = 204;
    pub const GLIDE: u16 = 205;
    /// Banking (FUN_140423a70: 206 when the cross product's y > 0).
    pub const GLIDE_RIGHT: u16 = 206;
    pub const GLIDE_LEFT: u16 = 207;
    /// Mode 0x11's standing glide.
    pub const GLIDE_STAND: u16 = 208;
    pub const DODGE_ROLL: u16 = 209;
}

/// Every growth motion, for loading.
pub const MOTIONS: [u16; 9] = [201, 202, 203, 204, 205, 206, 207, 208, 209];

/// Mode 0x11's velocity keep per step (`DAT_1405aaa40`).
pub const GLIDE_STAND_KEEP: f32 = 0.9;
/// Mode 0x12 flies straight while facing · request ≥ 0.98
/// (`DAT_1405d4fe8`).
pub const GLIDE_STRAIGHT: f32 = 0.98;
/// Glide motions and the fall after it blend over 16 (`DAT_140623bc4`).
pub const GLIDE_BLEND: f32 = 16.;
/// Dodge Roll's frame trigger 34: holding Square at roll tick 10 turns the
/// roll into a Quick Run (FUN_14040dd70 case 0x22 → FUN_1403a9830).
pub const ROLL_CHAIN_TICK: u32 = 10;

/// One `fmab` row (0x44 bytes): every growth parameter at one level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Row {
    /// +0x00 High Jump height.
    pub high_jump: f32,
    /// +0x04 / +0x08 Aerial Dodge arc height and horizontal speed.
    pub dodge_height: f32,
    pub dodge_speed: f32,
    /// +0x0c Quick Run hold time (frames), +0x10 start speed, +0x14 decay
    /// per step while held, +0x18 decay afterwards, +0x1c invulnerability.
    pub run_hold: f32,
    pub run_speed: f32,
    pub run_decay: f32,
    pub run_decay_after: f32,
    pub run_invulnerable: f32,
    /// +0x20 Glide target speed, +0x24 descent factor, +0x28 floor
    /// height, +0x2c maximum descent per step, +0x30 speed keep factor,
    /// +0x34 minimum start height, +0x38 end height, +0x3c turn rate.
    pub glide_speed: f32,
    pub glide_descent: f32,
    pub glide_floor: f32,
    pub glide_max_descent: f32,
    pub glide_keep: f32,
    pub glide_start: f32,
    pub glide_end: f32,
    pub glide_turn: f32,
    /// +0x40 Dodge Roll invulnerability (frames).
    pub roll_invulnerable: f32,
}

impl Row {
    pub const SIZE: usize = 0x44;

    pub fn read(b: &[u8]) -> Option<Self> {
        let f = |at: usize| {
            b.get(at..at + 4)
                .map(|w| f32::from_le_bytes(w.try_into().unwrap()))
        };
        Some(Row {
            high_jump: f(0x00)?,
            dodge_height: f(0x04)?,
            dodge_speed: f(0x08)?,
            run_hold: f(0x0c)?,
            run_speed: f(0x10)?,
            run_decay: f(0x14)?,
            run_decay_after: f(0x18)?,
            run_invulnerable: f(0x1c)?,
            glide_speed: f(0x20)?,
            glide_descent: f(0x24)?,
            glide_floor: f(0x28)?,
            glide_max_descent: f(0x2c)?,
            glide_keep: f(0x30)?,
            glide_start: f(0x34)?,
            glide_end: f(0x38)?,
            glide_turn: f(0x3c)?,
            roll_invulnerable: f(0x40)?,
        })
    }

    /// FUN_1404237a0: the glide's sink per step (Y down), never a rise:
    /// `min((height − floor) × factor, cap)`, 0 at or below the floor.
    pub fn glide_sink(&self, height: f32) -> f32 {
        let h = height - self.glide_floor;
        let v = if h <= 0. { 0. } else { h * self.glide_descent };
        v.min(self.glide_max_descent)
    }
}

/// 03system `pref/fmab`: u32 count, u32 offsets from the table start,
/// then the rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fmab {
    rows: Vec<Option<Row>>,
}

impl Fmab {
    pub fn read(table: &[u8]) -> Option<Self> {
        let count = u32::from_le_bytes(table.get(0..4)?.try_into().ok()?) as usize;
        if count > 64 {
            return None;
        }
        let rows = (0..count)
            .map(|i| {
                let off = u32::from_le_bytes(table.get(4 + i * 4..8 + i * 4)?.try_into().ok()?);
                if off == 0 {
                    Some(None)
                } else {
                    Some(Some(Row::read(table.get(off as usize..)?)?))
                }
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Fmab { rows })
    }

    /// FUN_1403e92d0: the row of `level` (1-based); an index past the
    /// table or an empty offset reads row 0.
    pub fn row(&self, level: u8) -> Option<&Row> {
        let i = (level as usize).wrapping_sub(1);
        match self.rows.get(i) {
            Some(Some(r)) => Some(r),
            _ => self.rows.first().and_then(Option::as_ref),
        }
    }
}

/// Quick Run's state (physics mode 0x10: obj+0x20 velocity, +0x108 decay,
/// +0x10c hold timer, +0x110 decay afterwards).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickRun {
    pub velocity: [f32; 2],
    pub decay: f32,
    pub hold: f32,
    pub decay_after: f32,
}

impl QuickRun {
    /// FUN_1403a9830: velocity = direction × start speed.
    pub fn start(direction: [f32; 2], row: &Row) -> Self {
        QuickRun {
            velocity: [direction[0] * row.run_speed, direction[1] * row.run_speed],
            decay: row.run_decay,
            hold: row.run_hold,
            decay_after: row.run_decay_after,
        }
    }

    /// FUN_140423500: the hold runs down while Square is held; on release
    /// or timeout the decay switches. Returns true on that switch (motion
    /// 204 then plays).
    pub fn frame(&mut self, held: bool, dt: f32) -> bool {
        if self.hold <= 0. {
            return false;
        }
        if !held {
            self.hold = 0.;
        } else {
            self.hold -= dt;
            if self.hold > 0. {
                return false;
            }
        }
        self.decay = self.decay_after;
        true
    }

    /// FUN_140423480 on the two-frame sampler: decay first, then move.
    pub fn step(&mut self, span: f32) -> [f32; 3] {
        self.velocity = [self.velocity[0] * self.decay, self.velocity[1] * self.decay];
        [self.velocity[0] * span, 0., self.velocity[1] * span]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<u8> {
        let mut t = vec![0u8; 4 + 2 * 4];
        t[0] = 2;
        let first = t.len() as u32;
        t[4..8].copy_from_slice(&first.to_le_bytes());
        for v in [
            235f32, 145., 18., 14., 30., 0.94, 0.93, 0., 16., 0.015, 0., 3., 0.91, 60., 10.,
        ] {
            t.extend(v.to_le_bytes());
        }
        t.extend(0.05236f32.to_le_bytes());
        t.extend(10f32.to_le_bytes());
        t
    }

    #[test]
    fn rows_read_by_level_with_row_zero_as_the_fallback() {
        let f = Fmab::read(&table()).unwrap();
        assert_eq!(f.row(1).unwrap().high_jump, 235.);
        // Level 2's offset is 0: row 0.
        assert_eq!(f.row(2).unwrap().dodge_height, 145.);
        assert_eq!(f.row(9).unwrap().roll_invulnerable, 10.);
    }

    #[test]
    fn malformed_nonzero_rows_are_not_empty_row_fallbacks() {
        let mut t = table();
        t[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Fmab::read(&t).is_none());
        let short = (t.len() - Row::SIZE + 1) as u32;
        t[8..12].copy_from_slice(&short.to_le_bytes());
        assert!(Fmab::read(&t).is_none());
        t[8..12].copy_from_slice(&0u32.to_le_bytes());
        assert!(Fmab::read(&t).is_some());
    }

    #[test]
    fn quick_run_decays_while_held_then_faster() {
        let f = Fmab::read(&table()).unwrap();
        let row = f.row(1).unwrap();
        let mut q = QuickRun::start([0., 1.], row);
        let d = q.step(2.);
        assert!((d[2] - 30. * 0.94 * 2.).abs() < 1e-4);
        assert!(!q.frame(true, 1.));
        assert!(q.frame(false, 1.));
        assert_eq!(q.decay, 0.93);
        assert!(!q.frame(false, 1.));
    }

    #[test]
    fn glide_sinks_toward_its_floor() {
        let row = Row {
            glide_descent: 0.015,
            glide_floor: 30.,
            glide_max_descent: 2.5,
            ..Row::default()
        };
        assert_eq!(row.glide_sink(20.), 0.);
        assert!((row.glide_sink(130.) - 1.5).abs() < 1e-5);
        assert_eq!(row.glide_sink(1000.), 2.5);
    }
}
