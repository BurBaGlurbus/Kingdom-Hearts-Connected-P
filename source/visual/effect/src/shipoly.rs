//! pppRyjDrawShipolyBone (200): a swing trail. The instance is handed two
//! world points per frame (the exe's `DAT_140b60268`, a pair of positions
//! on a bone from the element's BonePos); the program keeps a ring of the
//! last `N` pairs, runs a natural cubic spline through them for each
//! coordinate and edge, and lays the model's quad strip along the result,
//! fading along the strip. Translated from draw FUN_1404d1960, the strip
//! set-up FUN_1404d11e0, the rebuild FUN_1404d1d00, the spline solver
//! FUN_1404d1480, create FUN_1404d1c60 and destroy FUN_1404d1cb0 (update
//! FUN_1404d1c50 is empty).
//!
//! Key fields (the cursor): +4 model, +0xd freeze (the ring does not
//! advance), +0xe the ring length, +0xf taper, +0x10/+0x11/+0x12 the
//! texture window (width, height, quarter).
//!
//! The exe edits the model's own vertex colours and texture coordinates in
//! place; here the quads are generated with the same values, one corner
//! colour each. Uncertain: the draw-state bits the exe builds from key
//! bytes +8..+0xb (depth and write masks) are not modelled, and the
//! element's spawn and end are the caller's.
use crate::math::{self, Vec4};
use crate::models::cvtt;
use crate::programs::{byte, use_val, word};
use crate::{Ctx, Part, Quad};
use data::object_effect::{self, Program};

pub(crate) const KIND: u32 = 200;
/// The exe's stack arrays hold 128 samples.
const MAX_RING: usize = 128;

#[derive(Debug, Clone, Default)]
pub(crate) struct Trail {
    /// One `[A, B]` pair of world points per ring slot.
    ring: Vec<[Vec4; 2]>,
    head: usize,
    active: bool,
    /// Strip vertices relative to the newest A, as the model's shorts.
    vertices: Vec<[i16; 3]>,
    /// Per quad: alpha at its first and second pair, and the u range.
    quads: Vec<QuadState>,
}

#[derive(Debug, Clone, Copy, Default)]
struct QuadState {
    alpha: [u8; 2],
    u: [i32; 2],
}

/// FUN_1404d1c60: head 0, length 0, active.
pub(crate) fn create(p: &Program, part: &mut Part) {
    part.shipoly.insert(
        p.use_val,
        Trail {
            active: true,
            ..Trail::default()
        },
    );
}

/// FUN_1404d1cb0.
pub(crate) fn destroy(p: &Program, part: &mut Part) {
    part.shipoly.remove(&p.use_val);
}

/// FUN_1404d1480: the second differences `m` (a sixth of the natural
/// spline's second derivatives) of unit-spaced samples `y`.
fn spline(y: &[f32]) -> Vec<f32> {
    let n = y.len();
    let mut m = vec![0f32; n];
    if n < 3 {
        return m;
    }
    let d: Vec<f32> = y.windows(2).map(|w| w[1] - w[0]).collect();
    let mut diag = vec![0f32; n];
    diag[1] = 4.;
    m[1] = d[1] - d[0];
    for i in 1..n - 2 {
        m[i + 1] = (d[i + 1] - d[i]) - (1. / diag[i]) * m[i];
        diag[i + 1] = 4. - 1. / diag[i];
    }
    m[n - 2] -= m[n - 1];
    for j in (1..n - 1).rev() {
        m[j] = (m[j] - m[j + 1]) / diag[j];
    }
    m
}

/// FUN_1404d1d00 on the ring and the strip.
fn rebuild(trail: &mut Trail, key_taper: bool, key_freeze: bool, quads: usize, a: Vec4, ctx: &Ctx) {
    let n = trail.ring.len();
    let segments = quads;
    let samples = segments + 1;
    trail.vertices.resize(samples * 2, [0; 3]);
    let head = trail.head;
    for (c, origin) in a.iter().take(3).enumerate() {
        for e in 0..2 {
            let y: Vec<f32> = (0..n)
                .map(|j| trail.ring[(head + n - j % n) % n][e][c] - origin)
                .collect();
            let m = spline(&y);
            for i in 0..samples {
                let x = i as f32 / segments as f32 * (n - 1) as f32;
                let v = evaluate_hermite(&y, &m, x);
                trail.vertices[2 * i + e][c] = cvtt(v) as i16;
            }
        }
    }
    let _ = (key_taper, key_freeze, ctx);
}

/// The exe's evaluation: with `k = ⌊x⌋`, `f = x − k`,
/// `((y[k+1] − y[k]) − (2m[k] + m[k+1]) + ((m[k+1] − m[k])·f + 3m[k])·f)·f + y[k]`.
fn evaluate_hermite(y: &[f32], m: &[f32], x: f32) -> f32 {
    if y.len() < 2 {
        return y.first().copied().unwrap_or(0.);
    }
    let k = (x as usize).min(y.len() - 2);
    let f = x - k as f32;
    let a = m[k];
    ((y[k + 1] - y[k]) - (a + a + m[k + 1]) + ((m[k + 1] - a) * f + a * 3.) * f) * f + y[k]
}

pub(crate) fn draw(p: &Program, cursor: usize, part: &mut Part, ctx: &mut Ctx) {
    let model = word(ctx, cursor + 4);
    let Some([a3, b3]) = ctx.trail else {
        return;
    };
    if model == 0xffff {
        return;
    }
    let Some(model_ref) = ctx.dpd.models.get(&(model as i16)).copied() else {
        return;
    };
    let key = p.use_val;
    let Some(mut trail) = part.shipoly.remove(&key) else {
        return;
    };
    let a: Vec4 = [a3[0], a3[1], a3[2], 1.];
    let b: Vec4 = [b3[0], b3[1], b3[2], 1.];
    let n = (byte(ctx, cursor + 0xe) as usize).min(MAX_RING);
    // FUN_1404d11e0: the ring starts filled with the first pair.
    if trail.ring.is_empty() && n > 0 {
        trail.ring = vec![[a, b]; n];
    }
    let meshes = object_effect::omd_meshes(&ctx.dpd.bytes, model_ref.hdr).unwrap_or_default();
    let header = object_effect::omd_header(&ctx.dpd.bytes, model_ref.hdr).ok();
    let quad_count: usize = meshes.iter().map(|m| m.triangles.len() / 2).sum();
    // The strip needs 2·quads + 2 vertices (FUN_1404d11e0's check).
    if n == 0 || quad_count == 0 || model_ref.vertex_count != quad_count * 2 + 2 {
        part.shipoly.insert(key, trail);
        return;
    }
    let n = trail.ring.len();
    trail.head %= n;
    let head = trail.head;
    trail.ring[head] = [a, b];
    rebuild(
        &mut trail,
        byte(ctx, cursor + 0xf) != 0,
        byte(ctx, cursor + 0xd) != 0,
        quad_count,
        a,
        ctx,
    );
    let segments = quad_count;
    // Trailing segments whose edge-0 vertices did not move.
    let mut stalled = 0usize;
    if trail.active {
        let mut i = segments;
        while i > 0 && trail.vertices[2 * i] == trail.vertices[2 * i - 2] {
            stalled += 1;
            i -= 1;
        }
    }
    // The taper pulls edge 0 toward edge 1 along the strip.
    if byte(ctx, cursor + 0xf) != 0 {
        let live = if stalled > 0 {
            segments - stalled
        } else {
            segments
        };
        let step = if live > 0 { 1. / live as f32 } else { 1. };
        let mut f = 1f32;
        for i in 0..=segments {
            let v0: [f32; 3] = trail.vertices[2 * i].map(|c| c as f32);
            let v1: [f32; 3] = trail.vertices[2 * i + 1].map(|c| c as f32);
            // sceVu0InterVector: v1 · r + v2 · (1 − r).
            for c in 0..3 {
                trail.vertices[2 * i][c] = cvtt(v0[c] * f + v1[c] * (1. - f)) as i16;
            }
            f = (f - step).max(0.);
        }
    }
    // Texture window and the fade along the strip (FUN_1404d1d00 tail).
    if trail.active {
        let width = match byte(ctx, cursor + 0x10) {
            1 => 0x800,
            2 => 0x400,
            _ => 0x1000,
        };
        let height = match byte(ctx, cursor + 0x11) {
            1 => 0x800,
            2 => 0x400,
            _ => 0x1000,
        };
        let (mut u_start, mut u_end) = (0i32, width);
        match byte(ctx, cursor + 0x12) {
            1 => {
                u_start = 0x400;
                u_end += 0x400;
            }
            2 => {
                u_start = 0x800;
                u_end += 0x800;
            }
            3 => {
                u_start = 0xc00;
                u_end += 0xc00;
            }
            _ => {}
        }
        let _ = height;
        u_start += 0x10;
        u_end -= 0x10;
        let live = segments - stalled;
        let (mut u, step, mut alpha, alpha_step) = if live < 1 {
            (0, 0, 0f32, 0f32)
        } else {
            (
                (u_start),
                (u_end - u_start) / live as i32,
                128.,
                128. / live as f32,
            )
        };
        if live < 1 {
            u = 0;
        }
        trail.quads.resize(segments, QuadState::default());
        for (i, q) in trail.quads.iter_mut().enumerate() {
            let (first, second) = if i < live {
                (
                    cvtt(alpha) as u8,
                    ((cvtt(alpha) & 0xff) as f32 - alpha_step) as i32 as u8,
                )
            } else {
                (0, 0)
            };
            let next = u + step;
            *q = QuadState {
                alpha: [first, second],
                u: [u, next - 1],
            };
            alpha = (alpha - alpha_step).max(0.);
            u = next.min(u_end);
        }
    }
    // Draw: key +0xd == 0 advances the ring on whole ticks.
    let rgba: [u8; 4] = {
        let u0 = use_val(ctx, p, 0);
        std::array::from_fn(|i| part.u8(u0 + 8 + i))
    };
    if rgba[3] != 0 && !trail.quads.is_empty() {
        emit(
            ctx,
            &trail,
            &meshes,
            header.map_or(0x44, |h| h.alpha),
            rgba,
            a,
            cursor,
        );
    }
    if byte(ctx, cursor + 0xd) == 0 && !ctx.skip && ctx.flags[1] == 0 {
        trail.head += 1;
        if trail.head >= n {
            trail.active = false;
            trail.head = 0;
        }
    }
    part.shipoly.insert(key, trail);
}

/// The strip as textured quads in view space.
fn emit(
    ctx: &mut Ctx,
    trail: &Trail,
    meshes: &[object_effect::OmdMesh],
    alpha_register: u8,
    rgba: [u8; 4],
    origin: Vec4,
    cursor: usize,
) {
    let part = rgba.map(|c| c as f32 / 128.);
    let height = match byte(ctx, cursor + 0x11) {
        1 => 0x800,
        2 => 0x400,
        _ => 0x1000,
    };
    let (v_bottom, v_top) = (height - 0x10, 0x10);
    let mut index = 0usize;
    for mesh in meshes {
        let (tw, th) = if mesh.textured {
            (
                (1u32 << ((mesh.tex0 >> 26) & 0xf).min(10)) as f32,
                (1u32 << ((mesh.tex0 >> 30) & 0xf).min(10)) as f32,
            )
        } else {
            (1., 1.)
        };
        for _ in 0..mesh.triangles.len() / 2 {
            let Some(q) = trail.quads.get(index) else {
                break;
            };
            let at = |v: usize| -> [f32; 3] {
                let s = trail.vertices[v];
                let w = [
                    s[0] as f32 + origin[0],
                    s[1] as f32 + origin[1],
                    s[2] as f32 + origin[2],
                    1.,
                ];
                let p = math::apply(&ctx.view, w);
                [p[0], p[1], p[2]]
            };
            // Slots (0, 1, 2, 3) of the strip, drawn as 0, 1, 3, 2.
            let (r0, r1, r2, r3) = (2 * index, 2 * index + 1, 2 * index + 2, 2 * index + 3);
            let uv = |u: i32, v: i32| [u as f32 / 4096. * tw, v as f32 / 4096. * th];
            let colour = |alpha: u8| -> [f32; 4] {
                let a = (alpha as f32 / 255. * part[3]).min(1.) * (255. / 128.);
                [
                    part[0].min(1.) * (255. / 128.),
                    part[1].min(1.) * (255. / 128.),
                    part[2].min(1.) * (255. / 128.),
                    a.min(1.),
                ]
            };
            let (ca, cb) = (colour(q.alpha[0]), colour(q.alpha[1]));
            ctx.quads.push(Quad {
                corners: [at(r0), at(r1), at(r3), at(r2)],
                uv: [
                    uv(q.u[0], v_bottom),
                    uv(q.u[0], v_top),
                    uv(q.u[1], v_top),
                    uv(q.u[1], v_bottom),
                ],
                colour: ca,
                corner_colours: Some([ca, ca, cb, cb]),
                blend: alpha_register,
                tbp: (mesh.tex0 & 0x3fff) as u16,
                tex0: mesh.tex0,
            });
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_reproduces_a_line() {
        // A straight run has no curvature.
        let y: Vec<f32> = (0..8).map(|i| 2. * i as f32 + 1.).collect();
        let m = spline(&y);
        assert!(m.iter().all(|v| v.abs() < 1e-4));
        for x in [0., 0.5, 3.25, 6.9] {
            assert!((evaluate_hermite(&y, &m, x) - (2. * x + 1.)).abs() < 1e-3);
        }
    }

    #[test]
    fn spline_passes_through_the_samples() {
        let y = [0f32, 3., 1., 4., 1., 5.];
        let m = spline(&y);
        for (i, v) in y.iter().enumerate() {
            let x = (i as f32).min((y.len() - 1) as f32 - 1e-4);
            assert!((evaluate_hermite(&y, &m, x) - v).abs() < 1e-2, "{i}");
        }
    }
}
