//! Vector traps that turn a direction: `vector_homing` (0:46) and
//! `vector_rotx` (0:68). Operation order follows the executable's float code.

use super::math::read_sanitized;
use crate::script::{Fault, Memory, Slot};
use crate::world::{Ctx, TrapFn};

type Args = [Slot; 16];

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    Some(match (table, index) {
        (0, 46) => vector_homing,
        (0, 68) => vector_rotx,
        (0, 83) => vector_rot,
        _ => return None,
    })
}

/// FUN_140142080: scale xyz to unit length (unchanged when the length is 0).
fn normalize(v: [f32; 4]) -> [f32; 4] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len != 0.0 {
        let s = 1.0 / len;
        [v[0] * s, v[1] * s, v[2] * s, v[3]]
    } else {
        v
    }
}

/// FUN_140142120.
fn cross(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        b[2] * a[1] - b[1] * a[2],
        b[0] * a[2] - a[0] * b[2],
        a[0] * b[1] - b[0] * a[1],
        0.0,
    ]
}

/// FUN_1403b6fb0: the axis-angle matrix, in the executable's element order.
fn axis_angle(axis: [f32; 4], angle: f32) -> [f32; 16] {
    let a = normalize([axis[0], axis[1], axis[2], 1.0]);
    let (x, y, z) = (a[0], a[1], a[2]);
    let (s, c) = (angle.sin(), angle.cos());
    let k = 1.0 - c;
    let mut m = [0.0f32; 16];
    m[15] = 1.0;
    m[0] = x * x * k + c;
    m[5] = y * y * k + c;
    m[10] = z * z * k + c;
    let xy = y * x * k;
    let xz = z * x * k;
    let yz = z * y * k;
    m[1] = s * z + xy;
    m[4] = xy - s * z;
    m[2] = xz - s * y;
    m[8] = s * y + xz;
    m[6] = s * x + yz;
    m[9] = yz - s * x;
    m
}

/// FUN_14013f900: `m` applied to the column `v` as the executable does
/// (`out[i] = sum_j m[4j + i] * v[j]`).
fn apply(m: &[f32; 16], v: [f32; 4]) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for (i, o) in out.iter_mut().enumerate() {
        *o = m[i] * v[0] + m[4 + i] * v[1] + m[8 + i] * v[2] + m[12 + i] * v[3];
    }
    out
}

/// FUN_1403b7200: turn the unit direction of `a` toward the unit direction of
/// `b` by at most `step` radians (already scaled by the frame time). The result
/// is a unit vector with w = 0.
pub fn homing(a: [f32; 4], b: [f32; 4], step: f32) -> [f32; 4] {
    let a = normalize(a);
    let b = normalize(b);
    let dot = a[1] * b[1] + a[0] * b[0] + a[2] * b[2];
    let angle = if dot >= 1.0 {
        0.0
    } else if dot > -1.0 {
        dot.acos()
    } else {
        std::f32::consts::PI
    };
    let turn = if angle <= step && -step <= angle {
        angle
    } else {
        step
    };
    let axis = normalize(cross(a, b));
    let m = axis_angle(axis, turn);
    apply(&m, [a[0], a[1], a[2], 0.0])
}

/// 0:46 FUN_14041d3d0: `vector_homing(a, b, step)`; the result replaces `a`.
fn vector_homing(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let a = read_sanitized(ctx, m, args[0].value)?;
    let b = read_sanitized(ctx, m, args[1].value)?;
    let step = args[2].as_float() * ctx.world.frame_time;
    let r = homing(a, b, step);
    for (k, lane) in r.iter().enumerate() {
        ctx.write_f32(m, args[0].value + 4 * k as u32, *lane);
    }
    Ok(())
}

/// 0:68 FUN_14041d100 -> FUN_1403b75f0: the vector turned about the X axis.
/// [I] The matrix builder is taken to be the usual rotation about X, with the
/// angle wrapped into [-pi, pi) first (FUN_1403b6f20).
fn vector_rotx(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let v = read_sanitized(ctx, m, args[0].value)?;
    let angle = wrap_pi(args[1].as_float());
    let matrix = axis_angle([1.0, 0.0, 0.0, 1.0], angle);
    let r = apply(&matrix, v);
    let cell = ctx.world.static_cell(0, 68, 0);
    for (k, lane) in r.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 0:83 FUN_14041d650 -> FUN_1403b7560: `vector_rot(v, axis, angle)`, the
/// vector turned about the axis by the angle wrapped into [-pi, pi).
fn vector_rot(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let v = read_sanitized(ctx, m, args[0].value)?;
    let axis = read_sanitized(ctx, m, args[1].value)?;
    let matrix = axis_angle(axis, wrap_pi(args[2].as_float()));
    let r = apply(&matrix, v);
    let cell = ctx.world.static_cell(0, 83, 0);
    for (k, lane) in r.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// FUN_1403b6f20.
fn wrap_pi(a: f32) -> f32 {
    use std::f32::consts::PI;
    if 0.0 < a {
        (a + PI) % (2.0 * PI) - PI
    } else {
        PI - (PI - a) % (2.0 * PI)
    }
}

#[cfg(test)]
mod tests;
