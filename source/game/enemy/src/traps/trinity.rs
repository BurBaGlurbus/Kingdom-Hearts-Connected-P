//! Table 7 traps 28 to 31: Trinity Limit's shots. A shot is a 0x74-byte
//! record of a sprite-particle pool (FUN_140541a00: FUN_1404128d0 with
//! resource 0x5c3, 0x400 records of 0x74 bytes, DAT_142b596d0). The script
//! gets a pointer 0x28 into the record and tunes it (+0x50 the timer, +0x54
//! the speed), so records live in memory scripts can write. Every frame each
//! record runs its callback (FUN_140411ea0): FUN_140541ac0 while it flies,
//! then FUN_140541a70 while it fades. The pool draws them (FUN_140412450);
//! the damage of the limit comes from the scene's own attacks.
//!
//! Record words: +0 position, +0x10 callback, +0x14 sprite, +0x15 alpha,
//! +0x28 the offset to the target, +0x38 the velocity, +0x48 the owner,
//! +0x4c the sprite the callback copies, +0x50 timer, +0x54 speed, +0x58 the
//! turn rate, +0x5c the target record (five words), +0x70 bit 0 homing.
//! Positions are in the script's +Y-down frame.

use super::math::{native, read_sanitized};
use crate::env::Env;
use crate::script::{Fault, Memory, Slot};
use crate::world::{Ctx, TrapFn, World};

type Args = [Slot; 16];
type Vec4 = [f32; 4];

const RECORD: u32 = 0x74;
/// The script's pointer is this far into the record.
const SCRIPT_OFFSET: u32 = 0x28;
/// FUN_140541ac0: homing stops this close to the target (DAT_140623bcc).
const STOP_DISTANCE: f32 = 30.0;
/// FUN_140541ac0: the fade lasts this many frames (8.0).
const FADE: f32 = 8.0;
/// The alpha byte: timer * 0.125 * 128 (DAT_140623a0c, DAT_140585c80).
const ALPHA: f32 = 0.125 * 128.0;

/// Which callback a record runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// FUN_140541ac0.
    Fly,
    /// FUN_140541a70.
    Fade,
}

/// A record in use: its address in static memory and its callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub base: u32,
    pub phase: Phase,
}

/// The pool: records in use and freed blocks to reuse.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pool {
    pub shots: Vec<Shot>,
    free: Vec<u32>,
}

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    if table != 7 {
        return None;
    }
    Some(match index {
        28 => shot_init,
        29 => shot_start,
        30 => shot_ensure,
        31 => shot_set_effect_id,
        _ => return None,
    })
}

fn f32_at(w: &World, at: u32) -> f32 {
    f32::from_bits(w.static_read(at).unwrap_or(0))
}
fn vec_at(w: &World, at: u32) -> Vec4 {
    [0, 4, 8, 12].map(|k| f32_at(w, at + k))
}
fn put_f32(w: &mut World, at: u32, v: f32) {
    w.static_write(at, v.to_bits());
}
fn put_vec(w: &mut World, at: u32, v: Vec4) {
    for (k, x) in v.iter().enumerate() {
        put_f32(w, at + 4 * k as u32, *x);
    }
}

/// 7:28 FUN_14053f8a0 -> FUN_140541a00: the pool.
fn shot_init(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, _: &mut Args) -> Result<(), Fault> {
    ctx.world.shots.get_or_insert_with(Pool::default);
    Ok(())
}

/// 7:30 FUN_14053f990 -> FUN_1405419c0: the pool goes (its records with it).
fn shot_ensure(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, _: &mut Args) -> Result<(), Fault> {
    if let Some(p) = ctx.world.shots.take() {
        ctx.world
            .shots_freed
            .extend(p.shots.iter().map(|s| s.base).chain(p.free));
    }
    Ok(())
}

/// 7:29 FUN_14053f8b0 -> FUN_140541ba0: `(owner, target record, position,
/// direction)`: a record flying from the position along the direction at
/// speed 20 for 60 frames toward the target, sprite 7; the script gets the
/// pointer +0x28 into it. NaN lanes of the vectors are zeroed.
fn shot_start(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let dir = read_sanitized(ctx, m, args[3].value)?;
    let pos = read_sanitized(ctx, m, args[2].value)?;
    let record = args[1].value;
    let target: Vec<u32> = (0..5)
        .map(|k| ctx.read_u32(m, record.wrapping_add(4 * k)).unwrap_or(0))
        .collect();
    let owner = ctx.read_u32(m, args[0].value.wrapping_add(4)).unwrap_or(0);
    if ctx.world.shots.is_none() {
        args[0] = Slot::address(0);
        return Ok(());
    }
    let reuse = ctx.world.shots.as_mut().and_then(|p| p.free.pop());
    let base = reuse
        .or_else(|| ctx.world.shots_freed.pop())
        .unwrap_or_else(|| ctx.world.alloc(RECORD));
    let w = &mut *ctx.world;
    for k in (0..RECORD).step_by(4) {
        w.static_write(base + k, 0);
    }
    put_vec(w, base, pos);
    // FUN_1404118e0: alpha 0x80, frame 0xff (not started), +0x16 the pool's
    // +0xc; FUN_140541ba0 then sets the sprite to 8 and +0x16 to 0xff.
    w.static_write(base + 0x14, u32::from_le_bytes([8, 0x80, 0xff, 0xff]));
    put_vec(w, base + 0x28, [0., 0., 0., 1.]);
    put_vec(w, base + 0x38, dir);
    w.static_write(base + 0x48, owner);
    w.static_write(base + 0x4c, 7);
    put_f32(w, base + 0x50, 60.0);
    put_f32(w, base + 0x54, 20.0);
    put_f32(w, base + 0x58, 0.0);
    for (k, word) in target.iter().enumerate() {
        w.static_write(base + 0x5c + 4 * k as u32, *word);
    }
    w.static_write(base + 0x70, 1);
    if let Some(p) = w.shots.as_mut() {
        p.shots.push(Shot {
            base,
            phase: Phase::Fly,
        });
    }
    args[0] = Slot::address(base + SCRIPT_OFFSET);
    Ok(())
}

/// 7:31 FUN_14053f9a0: the sprite byte (+0x4c) of the record the pointer
/// names.
fn shot_set_effect_id(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let at = args[0].value.wrapping_sub(SCRIPT_OFFSET) + 0x4c;
    let id = args[1].as_int() as u32 & 0xff;
    let old = ctx.world.static_read(at).unwrap_or(0);
    ctx.world.static_write(at, old & !0xff | id);
    Ok(())
}

/// FUN_1403b7200: `v` turned toward `d` by at most `rate`, about the axis
/// across them (the angle itself when it is within the rate).
pub fn turn(v: Vec4, d: Vec4, rate: f32) -> Vec4 {
    let len = |a: Vec4| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let unit = |a: Vec4| {
        let l = len(a);
        if l > 0. { a.map(|x| x / l) } else { a }
    };
    let (a, b) = (unit(v), unit(d));
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let angle = if dot >= 1.0 {
        0.0
    } else if dot <= -1.0 {
        std::f32::consts::PI
    } else {
        dot.acos()
    };
    let step = if angle <= rate && -rate <= angle {
        angle
    } else {
        rate
    };
    let axis = unit([
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
        0.,
    ]);
    if len(axis) == 0. || step == 0. {
        return [v[0], v[1], v[2], 0.];
    }
    // Rodrigues: v cos + (k x v) sin + k (k . v)(1 - cos).
    let (s, c) = step.sin_cos();
    let kv = axis[0] * v[0] + axis[1] * v[1] + axis[2] * v[2];
    let cross = [
        axis[1] * v[2] - axis[2] * v[1],
        axis[2] * v[0] - axis[0] * v[2],
        axis[0] * v[1] - axis[1] * v[0],
    ];
    [
        v[0] * c + cross[0] * s + axis[0] * kv * (1. - c),
        v[1] * c + cross[1] * s + axis[1] * kv * (1. - c),
        v[2] * c + cross[2] * s + axis[2] * kv * (1. - c),
        0.,
    ]
}

/// Where the target record of a shot points (script frame), if it exists.
fn target_point(world: &World, env: &dyn Env, base: u32) -> Option<Vec4> {
    let handle = world.static_read(base + 0x5c)?;
    let p = if handle == crate::world::PLAYER_HANDLE {
        env.player_position()
    } else {
        let id = crate::world::object_from_handle(handle)?;
        let o = world.objects.get(id as usize)?.as_deref()?;
        if o.sys.gone {
            return None;
        }
        o.sys.position
    };
    Some(native([p[0], p[1], p[2], 1.]))
}

/// FUN_140541d30: steer toward the target while homing (+0x28 the offset,
/// +0x38 turned by the rate), then move by velocity * speed * dt.
fn step(world: &mut World, env: &dyn Env, base: u32) {
    let dt = world.frame_time;
    if world.static_read(base + 0x70).unwrap_or(0) & 1 != 0
        && let Some(t) = target_point(world, env, base)
    {
        let pos = vec_at(world, base);
        let d = [t[0] - pos[0], t[1] - pos[1], t[2] - pos[2], 0.];
        put_vec(world, base + 0x28, d);
        let rate = f32_at(world, base + 0x58) * dt;
        let v = turn(vec_at(world, base + 0x38), d, rate);
        put_vec(world, base + 0x38, v);
    }
    let (v, speed) = (vec_at(world, base + 0x38), f32_at(world, base + 0x54));
    let pos = vec_at(world, base);
    let moved = [0, 1, 2, 3].map(|k| pos[k] + v[k] * speed * dt);
    put_vec(world, base, moved);
}

/// One frame of the pool (FUN_140411b90's callbacks); records whose
/// callback ends are freed.
pub fn tick(world: &mut World, env: &dyn Env) {
    let Some(mut pool) = world.shots.take() else {
        return;
    };
    let dt = world.frame_time;
    let mut kept = Vec::new();
    for mut s in pool.shots.drain(..) {
        let b = s.base;
        let alive = match s.phase {
            Phase::Fly => {
                // The sprite follows +0x4c.
                let sprite = world.static_read(b + 0x4c).unwrap_or(0) & 0xff;
                let w14 = world.static_read(b + 0x14).unwrap_or(0);
                world.static_write(b + 0x14, w14 & !0xff | sprite);
                let homing = world.static_read(b + 0x70).unwrap_or(0);
                if homing & 1 != 0 {
                    let pos = vec_at(world, b);
                    let near = target_point(world, env, b).is_none_or(|t| {
                        let d = [t[0] - pos[0], t[1] - pos[1], t[2] - pos[2]];
                        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < STOP_DISTANCE
                    });
                    if near {
                        world.static_write(b + 0x70, homing & !1);
                    }
                }
                step(world, env, b);
                let t = f32_at(world, b + 0x50) - dt;
                put_f32(world, b + 0x50, t);
                if t > 0. {
                    true
                } else {
                    put_f32(world, b + 0x50, FADE);
                    s.phase = Phase::Fade;
                    fade(world, env, b)
                }
            }
            Phase::Fade => fade(world, env, b),
        };
        if alive {
            kept.push(s);
        } else {
            pool.free.push(b);
        }
    }
    pool.shots = kept;
    world.shots = Some(pool);
}

/// One drawn record. FUN_140411b90 writes a 16-byte point per record
/// (FUN_140412450, pool mode 0): the position, then +0x14 (the group of
/// `obj/PRIZE.mdlx`'s sprite program 203 — FUN_140541a00 builds the pool on
/// object 0x5c3, PRIZE), +0x15 (the alpha, 0x80 = 1.0), +0x16 and +0x17
/// (the flip-book frame; 0xff = not started). The draw (FUN_1404d7ed0)
/// steps the frame, and FUN_140411ea0 copies it back to +0x17.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub base: u32,
    /// Engine frame (+Y up).
    pub position: [f32; 3],
    pub group: u8,
    pub alpha: u8,
    pub frame: u8,
}

/// The points of the records in use.
pub fn points(world: &World) -> Vec<Point> {
    let Some(pool) = world.shots.as_ref() else {
        return Vec::new();
    };
    pool.shots
        .iter()
        .map(|s| {
            let p = native(vec_at(world, s.base));
            let b = world.static_read(s.base + 0x14).unwrap_or(0).to_le_bytes();
            Point {
                base: s.base,
                position: [p[0], p[1], p[2]],
                group: b[0],
                alpha: b[1],
                frame: b[3],
            }
        })
        .collect()
}

/// The frame the draw left in a record's point (back to +0x17).
pub fn set_frame(world: &mut World, base: u32, frame: u8) {
    let w = world.static_read(base + 0x14).unwrap_or(0);
    world.static_write(base + 0x14, w & 0x00ff_ffff | u32::from(frame) << 24);
}

/// FUN_140541a70: move, count down, the alpha byte follows the timer; the
/// record ends when the timer goes below 0.
fn fade(world: &mut World, env: &dyn Env, b: u32) -> bool {
    step(world, env, b);
    let t = f32_at(world, b + 0x50) - world.frame_time;
    put_f32(world, b + 0x50, t);
    if t < 0. {
        return false;
    }
    let w14 = world.static_read(b + 0x14).unwrap_or(0);
    let alpha = (t * ALPHA) as u32 & 0xff;
    world.static_write(b + 0x14, w14 & !0xff00 | alpha << 8);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_within_the_rate_lands_on_the_target_direction() {
        let v = turn([10., 0., 0., 0.], [0., 0., 5., 0.], 2.0);
        assert!(v[0].abs() < 1e-4 && (v[2] - 10.).abs() < 1e-4);
        // A rate of zero keeps the velocity.
        assert_eq!(
            turn([1., 0., 0., 0.], [0., 0., 1., 0.], 0.),
            [1., 0., 0., 0.]
        );
        // Past the rate: turned by the rate only.
        let w = turn([1., 0., 0., 0.], [0., 0., 1., 0.], 0.5);
        assert!((w[2].atan2(w[0]) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_shot_flies_then_fades_then_ends() {
        let mut world = World::default();
        world.frame_time = 1.0;
        world.shots = Some(Pool::default());
        let base = world.alloc(RECORD);
        put_vec(&mut world, base + 0x38, [1., 0., 0., 0.]);
        put_f32(&mut world, base + 0x50, 2.0);
        put_f32(&mut world, base + 0x54, 10.0);
        world.shots.as_mut().unwrap().shots.push(Shot {
            base,
            phase: Phase::Fly,
        });
        let env = crate::env::NullEnv;
        tick(&mut world, &env);
        assert_eq!(vec_at(&world, base)[0], 10.);
        tick(&mut world, &env);
        // The timer ran out: the fade took over this frame and moved again.
        assert_eq!(world.shots.as_ref().unwrap().shots[0].phase, Phase::Fade);
        assert_eq!(vec_at(&world, base)[0], 30.);
        for _ in 0..8 {
            tick(&mut world, &env);
        }
        assert!(world.shots.as_ref().unwrap().shots.is_empty());
    }

    #[test]
    fn a_point_carries_the_sprite_alpha_and_frame_in_the_engine_frame() {
        let mut world = World::default();
        world.shots = Some(Pool::default());
        let base = world.alloc(RECORD);
        put_vec(&mut world, base, [1., 2., 3., 1.]);
        world.static_write(base + 0x14, u32::from_le_bytes([7, 0x80, 0xff, 0xff]));
        world.shots.as_mut().unwrap().shots.push(Shot {
            base,
            phase: Phase::Fly,
        });
        let p = points(&world)[0];
        assert_eq!(p.position, [1., -2., -3.]);
        assert_eq!((p.group, p.alpha, p.frame), (7, 0x80, 0xff));
        set_frame(&mut world, base, 3);
        assert_eq!(points(&world)[0].frame, 3);
        assert_eq!(points(&world)[0].alpha, 0x80);
    }
}
