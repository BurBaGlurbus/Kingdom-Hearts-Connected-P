//! `bg_get_random_pos` and `bg_get_random_pos_air` (1:247, 1:248): a random
//! point on the map around a centre, translated from FUN_1403f3fa0 /
//! FUN_1403f4180 (on the ground) and FUN_1403f4080 / FUN_1403f44a0 (in the
//! air), which share a start-up and walk a ring of eight points around the
//! centre, shrinking the ring by 100 after each lap, until one passes.
//!
//! The functions work in the engine's +Y-up frame: where the executable
//! (+Y down) raises a point by subtracting from y, these add. Every map test
//! is FUN_14040c2b0 with the trap's mask as the collision class (1 from
//! FUN_14042ead0, 2 from FUN_14042eb90), through
//! [`Env::line_hit_class`](crate::env::Env::line_hit_class).

use super::math::{native, read_native, wrap_angle};
use crate::env::Env;
use crate::script::{Fault, Memory, Slot};
use crate::world::{Ctx, Lcg, TrapFn};

type Args = [Slot; 16];
type Vec4 = [f32; 4];

const ANGLE_STEP: f32 = std::f32::consts::FRAC_PI_4;
/// 2^-32 as the executable's single-precision constant.
const UNIT: f32 = 2.328_306_4e-10;
/// The ray starts this far above the ring point and ends this far below it.
const RAY_UP: f32 = 200.0;
const RAY_DOWN: f32 = 1200.0;
/// A found point is checked for a clear line from the centre to just past it.
const REACH: f32 = 20.0;
const LOS_LIFT: f32 = 100.0;
const LAP_SHRINK: f32 = 100.0;
/// The fallback ray: straight down from just above the centre.
const FALLBACK_UP: f32 = 200.0;
const FALLBACK_DOWN: f32 = 1200.0;
/// The masks the two traps pass (FUN_14042ead0, FUN_14042eb90).
const GROUND_CLASS: u32 = 1;
const AIR_CLASS: u32 = 2;

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    Some(match (table, index) {
        (1, 247) => bg_random_pos,
        (1, 248) => bg_random_pos_air,
        _ => return None,
    })
}

/// The ring a search walks (the executable's 0x24-byte search record).
struct Ring {
    center: Vec4,
    near: f32,
    radius: f32,
    start: f32,
    count: i32,
}

impl Ring {
    /// FUN_1403f3fa0 / FUN_1403f4080 up to the call of the walk: two steps
    /// of the engine generator (the first picks the radius, the second
    /// adds to the start angle) and the lap counter DAT_142ae5d78.
    fn new(rng: &mut Lcg, counter: &mut u32, center: Vec4, near: f32, far: f32) -> Self {
        let first = rng.step();
        let second = rng.step();
        let radius = first as f32 * UNIT * (far - near) + near;
        let turns = (*counter >> 2 & 1 | *counter << 1) as i32;
        let start = turns as f32 * ANGLE_STEP + second as f32 * UNIT * ANGLE_STEP;
        *counter = (*counter + 1) & 7;
        Self {
            center,
            near,
            radius,
            start,
            count: 0,
        }
    }

    /// The next ring point's heading (sin, cos).
    fn next_heading(&mut self) -> (f32, f32) {
        self.count += 1;
        let angle = wrap_angle(self.count as f32 * ANGLE_STEP + self.start);
        (angle.sin(), angle.cos())
    }

    fn point(&self, (s, c): (f32, f32), radius: f32) -> Vec4 {
        [
            self.center[0] + s * radius,
            self.center[1] + 0.0 * radius,
            self.center[2] + c * radius,
            self.center[3] + 0.0 * radius,
        ]
    }

    /// After a full lap the ring shrinks; true while it still reaches `near`.
    fn lap(&mut self) -> bool {
        if self.count > 7 {
            self.radius -= LAP_SHRINK;
            self.count = 0;
        }
        self.near <= self.radius
    }

    fn center3(&self) -> [f32; 3] {
        [self.center[0], self.center[1], self.center[2]]
    }
}

/// FUN_1403f3fa0: a point on the ground around `center`, against the
/// polygons of collision `class`.
pub fn ground_point(
    env: &dyn Env,
    rng: &mut Lcg,
    counter: &mut u32,
    center: Vec4,
    near: f32,
    far: f32,
    class: u32,
) -> Vec4 {
    let mut ring = Ring::new(rng, counter, center, near, far);
    if ring.near <= ring.radius {
        loop {
            let heading = ring.next_heading();
            let at = ring.point(heading, ring.radius);
            let top = [at[0], at[1] + RAY_UP, at[2]];
            let bottom = [at[0], at[1] - RAY_DOWN, at[2]];
            if let Some(p) = env.line_hit_class(top, bottom, class) {
                let found = [p[0], p[1], p[2], 1.0];
                // A clear line from the centre to just past the point.
                let past = ring.point(heading, ring.radius + REACH);
                let end = [past[0], found[1] + LOS_LIFT, past[2]];
                if env.line_hit_class(ring.center3(), end, class).is_none() {
                    return found;
                }
            }
            if !ring.lap() {
                break;
            }
        }
    }
    // FUN_1403f3dd0: the ground straight under the centre, else the centre.
    let c = ring.center;
    let top = [c[0], c[1] + FALLBACK_UP, c[2]];
    let bottom = [c[0], c[1] - FALLBACK_DOWN, c[2]];
    match env.line_hit_class(top, bottom, class) {
        Some(p) => [p[0], p[1], p[2], 1.0],
        None => c,
    }
}

/// FUN_1403f4080: a point in the air around `center`, `low` to `high` above
/// the ring, with a clear line to it; the centre when none is found.
#[allow(clippy::too_many_arguments)]
pub fn air_point(
    env: &dyn Env,
    rng: &mut Lcg,
    counter: &mut u32,
    center: Vec4,
    near: f32,
    far: f32,
    low: f32,
    high: f32,
    class: u32,
) -> Vec4 {
    let mut ring = Ring::new(rng, counter, center, near, far);
    if ring.near <= ring.radius {
        loop {
            let heading = ring.next_heading();
            let mut at = ring.point(heading, ring.radius);
            let draw = rng.step();
            at[1] += draw as f32 * UNIT * (high - low) + low;
            let past = ring.point(heading, ring.radius + REACH);
            let end = [past[0], at[1] + LOS_LIFT, past[2]];
            if env.line_hit_class(ring.center3(), end, class).is_none() {
                return at;
            }
            if !ring.lap() {
                break;
            }
        }
    }
    ring.center
}

/// Both traps hand back the same buffer (DAT_140756b48).
fn finish(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args, v: Vec4) -> Result<(), Fault> {
    let cell = ctx.world.static_cell(1, 247, 0);
    for (k, lane) in native(v).iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 1:247 FUN_14042ead0: `(centre, near, far)`.
fn bg_random_pos(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let center = read_native(ctx, m, args[0].value)?;
    let (near, far) = (args[1].as_float(), args[2].as_float());
    let world = &mut *ctx.world;
    let v = ground_point(
        ctx.env,
        &mut world.engine_rng,
        &mut world.ring_counter,
        center,
        near,
        far,
        GROUND_CLASS,
    );
    finish(ctx, m, args, v)
}

/// 1:248 FUN_14042eb90: `(centre, near, far, low, high)`.
fn bg_random_pos_air(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let center = read_native(ctx, m, args[0].value)?;
    let (near, far) = (args[1].as_float(), args[2].as_float());
    let (low, high) = (args[3].as_float(), args[4].as_float());
    let world = &mut *ctx.world;
    let v = air_point(
        ctx.env,
        &mut world.engine_rng,
        &mut world.ring_counter,
        center,
        near,
        far,
        low,
        high,
        AIR_CLASS,
    );
    finish(ctx, m, args, v)
}

#[cfg(test)]
mod tests;
