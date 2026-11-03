//! Traps whose answer or effect belongs to the presentation layer: the head
//! look (the NPC's head turning to a target), the camera's eye and target,
//! bone matrices, the mission gauge and the light-cycle steering of the
//! Space Paranoids enemies. Translated from the executable (Steam build
//! 15194255).
//!
//! What the camera, the skeleton and the HUD know comes from the host's
//! [`Env`](crate::env::Env); without an answer the script gets the zero
//! vector (w = 1), the identity matrix or 0, which is what the executable
//! returns when the subsystem has nothing.

use super::math::{native, native_yaw, read_native, read_sanitized, rotate_y};
use crate::route::{add, normalize, scale, sub};
use crate::script::{Fault, Memory, Slot};
use crate::world::{Ctx, TrapFn};

type Args = [Slot; 16];
type V4 = [f32; 4];

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    Some(match (table, index) {
        (1, 83) => apply_bone_matrix,
        (1, 174) => look_start,
        (1, 176) => look_end,
        (1, 252) => camera_apply_pos,
        (1, 269) => lightcycle_movement,
        (1, 305) => camera_eye,
        (1, 306) => camera_at,
        (4, 12) => mission_gauge_ratio,
        _ => return None,
    })
}

/// The buffer the position-returning traps share (DAT_140756b48).
fn return_shared(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args, v: V4) {
    let cell = ctx.world.static_cell(1, 247, 0);
    for (k, lane) in native(v).iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
}

/// 1:174 FUN_14042d7b0 -> FUN_1403e5650: start looking at another object,
/// blending in over `ticks`. Only a model with a LOOK shape does.
fn look_start(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let target = ctx.sys_of_ai(m, args[1].value)?;
    let ticks = args[2].as_float();
    let sys = &ctx.object(target)?.sys;
    let f = sys.forward();
    let facing = [f[0], 0.0, f[2], sys.yaw];
    let look = &mut ctx.object(id)?.sys.head_look;
    if look.bone.is_some() {
        look.state = 2;
        look.target = Some(target);
        look.target_facing = facing;
        look.blend.start(0.0, 1.0, ticks);
        look.kind = 1;
    }
    Ok(())
}

/// 1:176 FUN_14042d8a0 -> FUN_1403e4f70: blend the look out over `ticks`.
fn look_end(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let ticks = args[1].as_float();
    let blend = &mut ctx.object(id)?.sys.head_look.blend;
    let from = blend.value;
    blend.start(from, 0.0, ticks);
    Ok(())
}

/// 1:305 FUN_140430da0: where the camera is.
fn camera_eye(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let v = ctx
        .env
        .camera_eye()
        .map_or([0., 0., 0., 1.], |p| [p[0], p[1], p[2], 1.]);
    return_shared(ctx, m, args, v);
    Ok(())
}

/// 1:306 FUN_140430de0: what the camera looks at.
fn camera_at(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let v = ctx
        .env
        .camera_at()
        .map_or([0., 0., 0., 1.], |p| [p[0], p[1], p[2], 1.]);
    return_shared(ctx, m, args, v);
    Ok(())
}

/// 1:252 FUN_140430bf0: a point (w = 1) through the inverse of the camera's
/// matrix.
fn camera_apply_pos(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let p = read_native(ctx, m, args[0].value)?;
    let v = ctx
        .env
        .camera_apply_pos([p[0], p[1], p[2], 1.0])
        .unwrap_or([0., 0., 0., 1.]);
    return_shared(ctx, m, args, v);
    Ok(())
}

/// 1:83 FUN_14042f6f0: a point through the matrix of one of the object's
/// bones (FUN_1403b50f0): `out[k] = m[k]*x + m[4+k]*y + m[8+k]*z + m[12+k]*w`.
/// Without a skeleton the matrix is the identity.
fn apply_bone_matrix(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let bone = args[1].value;
    let v = read_sanitized(ctx, m, args[2].value)?;
    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let matrix = ctx.env.bone_matrix(id, bone).unwrap_or(IDENTITY);
    let out: V4 = std::array::from_fn(|k| {
        matrix[k] * v[0] + matrix[4 + k] * v[1] + matrix[8 + k] * v[2] + matrix[12 + k] * v[3]
    });
    let cell = ctx.world.static_cell(1, 247, 0);
    for (k, lane) in out.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 4:12 FUN_140433770 -> FUN_1403fb690: the mission gauge's fill (0 to 1).
fn mission_gauge_ratio(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let index = args[0].as_int();
    args[0] = Slot::float(ctx.env.mission_gauge_ratio(index));
    Ok(())
}

/// Everything FUN_140421940 reads of the two light cycles.
#[derive(Clone, Copy, Debug)]
pub struct Cycle {
    pub position: V4,
    pub facing: V4,
}

fn cross(a: &V4, b: &V4) -> V4 {
    // FUN_140142120.
    [
        b[2] * a[1] - b[1] * a[2],
        b[0] * a[2] - a[0] * b[2],
        a[0] * b[1] - b[0] * a[1],
        0.0,
    ]
}

/// FUN_140421940: how the light cycle `own` should steer to meet `other`:
/// `[side, 0, turn, distance]`. `lead` scales how far ahead of itself it
/// aims, `speed` is the cycles' speed parameter (DAT_142af7bc8 + 4) and
/// `frame_time` the frame length. In the executable's +Y-down frame.
pub fn lightcycle_steer(own: &Cycle, other: &Cycle, lead: f32, speed: f32, frame_time: f32) -> V4 {
    let step = scale(&scale(&own.facing, speed), frame_time);
    let next = add(&own.position, &step);
    let mut out = [0.0f32; 4];
    let right = rotate_y(&other.facing, std::f32::consts::FRAC_PI_2);
    let ahead = scale(&step, lead);
    let aim = sub(&sub(&other.position, &ahead), &next);
    let side = cross(&right, &aim);
    if 0.0 <= side[1] {
        let shifted = add(&other.position, &scale(&side, lead));
        let aim = sub(&shifted, &next);
        let side = cross(&right, &aim);
        if 0.0 < side[1] {
            out[2] = -1.0;
        }
    } else {
        out[2] = 1.0;
    }
    let mut toward = sub(&other.position, &next);
    out[3] = normalize(&mut toward);
    let turn = cross(&toward, &own.facing);
    out[0] = if 0.0 <= turn[1] { -1.0 } else { 1.0 };
    out
}

/// 1:269 FUN_14042c8e0 -> FUN_140421940.
fn lightcycle_movement(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let own = ctx.sys_of_ai(m, args[0].value)?;
    let other = ctx.sys_of_ai(m, args[1].value)?;
    let lead = args[2].as_float();
    let cycle = |ctx: &mut Ctx<'_>, id| -> Result<Cycle, Fault> {
        let sys = &ctx.object(id)?.sys;
        let f = sys.forward();
        let p = sys.position;
        let facing = native([f[0], 0.0, f[2], 0.0]);
        Ok(Cycle {
            position: native([p[0], p[1], p[2], 1.0]),
            facing: [facing[0], 0.0, facing[2], native_yaw(sys.yaw)],
        })
    };
    let (a, b) = (cycle(ctx, own)?, cycle(ctx, other)?);
    let v = lightcycle_steer(
        &a,
        &b,
        lead,
        ctx.env.lightcycle_speed(),
        ctx.world.frame_time,
    );
    let cell = ctx.world.static_cell(1, 247, 0);
    for (k, lane) in v.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

#[cfg(test)]
mod tests;
