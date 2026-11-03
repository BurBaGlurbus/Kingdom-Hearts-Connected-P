//! Table 7 traps 3 to 11: the flare pool Duck Flare's scene keeps
//! (DAT_142b59670, built by FUN_140540810 under the task FUN_1401506b0(1,
//! 0x501e, FUN_140540fa0)). Each flare is a 0x50-byte record: its position
//! (+0), a collision shape (+0x10: kind, radius, height; centre at +0x28),
//! the object it chases (+0x38), the radius arguments (+0x40, +0x44) and
//! the effect riding on it (+0x48). The scene places the flares; every
//! frame the pool pushes overlapping flares apart and out of Sora's push
//! shapes (FUN_140541180). Positions stay in the script's +Y-down frame.

use super::math::read_sanitized;
use crate::script::{self, Fault, Memory, Slot};
use crate::world::{Ctx, Event, TrapFn};

type Args = [Slot; 16];
type Vec4 = [f32; 4];

/// Flare handles are a record (0x50 bytes) apart from here.
pub const FLARE_HANDLES: u32 = script::EXTERN_SEGMENT + 0x0a00_0000;
const RECORD: u32 = 0x50;
/// The pool's records (FUN_140541430's stack of 0x2e indices).
pub const SLOTS: usize = 0x2e;
/// Shape kinds (FUN_1403c9b60): an upright cylinder and a sphere.
pub const CYLINDER: u32 = 0;
pub const SPHERE: u32 = 3;
/// FUN_1403c90b0: the push-out reaches this far past the touching distance.
const PUSH_MARGIN: f32 = 5.0;

/// A collision shape (FUN_1403c9c00's layout): kind, radius, height and
/// centre.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shape {
    pub kind: u32,
    pub radius: f32,
    pub height: f32,
    pub center: Vec4,
}

/// One flare record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flare {
    pub position: Vec4,
    pub shape: Shape,
    /// +0x38: the object of the target record `flare_set_target` gave.
    pub target: u32,
    /// +0x40, +0x44: `flare_set_radius`'s arguments.
    pub radius: [f32; 2],
    /// +0x48: the effect handle that follows the flare.
    pub effect: Option<u32>,
}

/// The pool: its records, the stack of free indices (+0xe60) and their
/// count (+0xf18).
#[derive(Clone, Debug, PartialEq)]
pub struct Pool {
    pub flares: Vec<Flare>,
    free: [usize; SLOTS],
    count: usize,
}

impl Default for Pool {
    /// FUN_140540810 then FUN_140541430.
    fn default() -> Self {
        let mut p = Pool {
            flares: vec![Flare::default(); SLOTS],
            free: [0; SLOTS],
            count: 0,
        };
        p.reset();
        p
    }
}

impl Pool {
    /// FUN_140541430: every record free again (their contents stay).
    pub fn reset(&mut self) {
        for (i, f) in self.free.iter_mut().enumerate() {
            *f = i;
        }
        self.count = SLOTS;
    }

    /// FUN_140540e90: the top of the free stack.
    fn take(&mut self) -> Option<usize> {
        self.count = self.count.checked_sub(1)?;
        Some(self.free[self.count])
    }

    /// FUN_140541090.
    fn give_back(&mut self, index: usize) {
        if self.count < SLOTS {
            self.free[self.count] = index;
            self.count += 1;
        }
    }

    /// FUN_140541160: no flare is out.
    pub fn is_empty(&self) -> bool {
        self.count == SLOTS
    }

    /// FUN_140540ed0: the records in use, in index order.
    pub fn in_use(&self) -> Vec<usize> {
        (0..SLOTS)
            .filter(|i| !self.free[..self.count].contains(i))
            .collect()
    }

    /// FUN_140541180 for each record in use (FUN_140540fa0): a flare that
    /// overlaps another moves by minus half the push-out and the other by
    /// half; then each push shape of Sora that it overlaps pushes it out.
    /// Returns the flares that moved, for their effects.
    pub fn separate(&mut self, pushers: &[Shape]) -> Vec<usize> {
        let mut moved = Vec::new();
        let used = self.in_use();
        for &i in &used {
            for &j in &used {
                if i == j || !overlap(&self.flares[i].shape, &self.flares[j].shape) {
                    continue;
                }
                let p = push_out(&self.flares[i].shape, &self.flares[j].shape);
                let other = add(self.flares[j].position, scale(p, 0.5));
                self.place(j, other);
                let this = add(self.flares[i].position, scale(p, -0.5));
                self.place(i, this);
                moved.extend([i, j]);
            }
            for s in pushers {
                if overlap(&self.flares[i].shape, s) {
                    let p = push_out(&self.flares[i].shape, s);
                    let this = sub(self.flares[i].position, p);
                    self.place(i, this);
                    moved.push(i);
                }
            }
        }
        moved.sort_unstable();
        moved.dedup();
        moved
    }

    /// FUN_140541610's copies: the position and the shape's centre.
    fn place(&mut self, i: usize, v: Vec4) {
        let f = &mut self.flares[i];
        f.position = v;
        f.shape.center = v;
    }
}

fn add(a: Vec4, b: Vec4) -> Vec4 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}
fn sub(a: Vec4, b: Vec4) -> Vec4 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}
fn scale(a: Vec4, s: f32) -> Vec4 {
    a.map(|x| x * s)
}

/// FUN_1403c94c0 for the cylinder and sphere kinds (FUN_1403c9890 when a
/// cylinder takes part): the centres' offset over the summed radii and
/// heights lies inside the unit sphere; two spheres compare the distance
/// with the summed radii. Other kinds do not touch.
pub fn overlap(a: &Shape, b: &Shape) -> bool {
    let d = sub(b.center, a.center);
    match (a.kind, b.kind) {
        (SPHERE, SPHERE) => {
            let r = a.radius + b.radius;
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < r * r
        }
        (CYLINDER | SPHERE, CYLINDER | SPHERE) => {
            let (r, h) = (a.radius + b.radius, a.height + b.height);
            let n = [d[0] / r, d[1] / h, d[2] / r];
            n[0] * n[0] + n[1] * n[1] + n[2] * n[2] < 1.0
        }
        _ => false,
    }
}

/// FUN_1403c90b0 for the cylinder and sphere kinds: the offset that takes
/// the shapes apart along the ground. Coincident centres push along +X.
pub fn push_out(a: &Shape, b: &Shape) -> Vec4 {
    if !matches!(a.kind, CYLINDER | SPHERE) || !matches!(b.kind, CYLINDER | SPHERE) {
        return [0.; 4];
    }
    let d = sub(b.center, a.center);
    let (r, h) = (a.radius + b.radius, b.height + a.height);
    let n = [d[0] / r, d[1] / h, d[2] / r, 0.];
    let reach = r + PUSH_MARGIN;
    let ground = n[2] * n[2] + n[0] * n[0];
    let mut out = if ground <= 0. {
        [reach, 0., 0., 0.]
    } else {
        let s = ((1.0 - n[1] * n[1]) / ground).abs().sqrt();
        scale(n, (s - 1.0) * reach)
    };
    out[1] = 0.;
    out
}

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    if table != 7 {
        return None;
    }
    Some(match index {
        3 => init,
        4 => new,
        5 => free,
        6 => set_pos,
        7 => set_radius,
        8 => set_effect,
        9 => set_target,
        10 => get_pos,
        11 => is_empty,
        _ => return None,
    })
}

fn index_of(handle: u32) -> Option<usize> {
    let o = handle.checked_sub(FLARE_HANDLES)?;
    let i = (o / RECORD) as usize;
    (o % RECORD == 0 && i < SLOTS).then_some(i)
}

fn flare<'a>(ctx: &'a mut Ctx<'_>, handle: u32) -> Option<&'a mut Flare> {
    let i = index_of(handle)?;
    ctx.world.flares.as_mut()?.flares.get_mut(i)
}

/// The effect riding on a flare follows it (FUN_1402be970's position).
pub fn effect_follows(events: &mut Vec<Event>, f: &Flare) {
    if let Some(handle) = f.effect {
        events.push(Event::EffectCommand {
            trap: 38,
            handle,
            words: [0, 0],
            vector: Some([f.position[0], f.position[1], f.position[2]]),
        });
    }
}

/// 7:3 FUN_14053f380 -> FUN_1405410e0: the pool (made once) with every
/// record free.
fn init(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, _: &mut Args) -> Result<(), Fault> {
    ctx.world.flares.get_or_insert_with(Pool::default).reset();
    Ok(())
}

/// 7:4 FUN_14053f390 -> FUN_140540e90: a free record's handle. [I] An
/// exhausted pool answers 0 (the executable reads before its stack).
fn new(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let i = ctx.world.flares.as_mut().and_then(Pool::take);
    args[0] = Slot::address(i.map_or(0, |i| FLARE_HANDLES + RECORD * i as u32));
    Ok(())
}

/// 7:5 FUN_14053f3c0 -> FUN_140541090.
fn free(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    if let (Some(i), Some(p)) = (index_of(args[0].value), ctx.world.flares.as_mut()) {
        p.give_back(i);
    }
    Ok(())
}

/// 7:6 FUN_14053f3e0 -> FUN_140541610: the position (NaN lanes zeroed), the
/// shape's centre and the effect's position.
fn set_pos(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let v = read_sanitized(ctx, m, args[1].value)?;
    let mut moved = None;
    if let Some(f) = flare(ctx, args[0].value) {
        f.position = v;
        f.shape.center = v;
        moved = Some(f.clone());
    }
    if let Some(f) = moved {
        effect_follows(&mut ctx.world.events, &f);
    }
    Ok(())
}

/// 7:7 FUN_14053f460 -> FUN_140541670: `(radius, height)`; a height above
/// 0 makes a cylinder, otherwise a sphere of the radius.
fn set_radius(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let (radius, height) = (args[1].as_float(), args[2].as_float());
    if let Some(f) = flare(ctx, args[0].value) {
        f.radius = [radius, height];
        f.shape.kind = if height <= 0. { SPHERE } else { CYLINDER };
        f.shape.radius = radius;
        f.shape.height = if height > 0. { height } else { radius };
    }
    Ok(())
}

/// 7:8 FUN_14053f4a0: +0x48, the effect that follows the flare.
fn set_effect(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let effect = args[1].value;
    if let Some(f) = flare(ctx, args[0].value) {
        f.effect = (effect != 0).then_some(effect);
    }
    Ok(())
}

/// 7:9 FUN_14053f4d0: +0x38, the object of a target record. [U] Nothing
/// in the pool's update reads it; the record's handle is kept.
fn set_target(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let target = args[1].value;
    if let Some(f) = flare(ctx, args[0].value) {
        f.target = target;
    }
    Ok(())
}

/// 7:10 FUN_14053f510: a copy of the position in a static vector
/// (DAT_140788710).
fn get_pos(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let v = flare(ctx, args[0].value).map_or([0.; 4], |f| f.position);
    let cell = ctx.world.static_cell(7, 10, 0);
    for (k, lane) in v.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 7:11 FUN_14053f560 -> FUN_140541160.
fn is_empty(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let empty = ctx.world.flares.as_ref().is_none_or(Pool::is_empty);
    args[0] = Slot::int(empty as i32);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cyl(x: f32) -> Shape {
        Shape {
            kind: CYLINDER,
            radius: 60.,
            height: 60.,
            center: [x, 0., 0., 1.],
        }
    }

    #[test]
    fn the_free_stack_hands_out_the_last_index_first() {
        let mut p = Pool::default();
        assert!(p.is_empty());
        assert_eq!(p.take(), Some(SLOTS - 1));
        assert_eq!(p.take(), Some(SLOTS - 2));
        assert!(!p.is_empty());
        assert_eq!(p.in_use(), vec![SLOTS - 2, SLOTS - 1]);
        p.give_back(SLOTS - 1);
        assert_eq!(p.take(), Some(SLOTS - 1));
        p.reset();
        assert!(p.is_empty());
    }

    #[test]
    fn overlapping_flares_part_along_the_ground() {
        let (a, b) = (cyl(0.), cyl(60.));
        assert!(overlap(&a, &b) && !overlap(&a, &cyl(130.)));
        let p = push_out(&a, &b);
        // n = 0.5 along x: s = 2, out = 0.5 * (2 - 1) * 125.
        assert!((p[0] - 62.5).abs() < 1e-4 && p[1] == 0. && p[2] == 0.);
        let mut pool = Pool::default();
        let (i, j) = (pool.take().unwrap(), pool.take().unwrap());
        pool.place(i, a.center);
        pool.place(j, b.center);
        for k in [i, j] {
            pool.flares[k].shape.kind = CYLINDER;
            pool.flares[k].shape.radius = 60.;
            pool.flares[k].shape.height = 60.;
        }
        assert_eq!(pool.separate(&[]), vec![j, i]);
        let gap = pool.flares[i].position[0] - pool.flares[j].position[0];
        assert!(gap.abs() > 60.);
    }
}
