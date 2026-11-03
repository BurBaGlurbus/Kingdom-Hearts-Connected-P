//! pppKeShpTail3X (157): a tail of shape sprites strung along the part's
//! recent path. Translated from update FUN_1404ceab0, draw FUN_1404ceef0,
//! create FUN_1404cee40 and restart FUN_1404ceeb0.
//!
//! Work block u0: +0x00 head RGBA and +0x08 tail RGBA (i16, value >> 7 =
//! colour byte), +0x10/+0x18 their rates, +0x20/+0x28 the rates'
//! accelerations; +0x30 a ring of 28 positions (vec4), +0x1f0 the
//! flip-book clock, +0x1f8 a 16-bit random seed, +0x1fa the ring head.
//!
//! Each update writes the part's translation (or its world position when
//! key byte +0x53 is set) one slot back in the ring. The draw walks the ring
//! from the newest point, placing up to `+0x1c` sprites a shrinking spacing
//! apart (`+0x18`, scaled with the size `+0xc` → `+0x10`), each a random
//! flip-book frame, size and (with `+0x1e`) roll, coloured from head to
//! tail colour.
use crate::math::{self, Mat, Vec4};
use crate::programs::{active, byte, emit_sprites, float, key, use_val, word};
use crate::{Ctx, Part};
use data::object_effect::{Program, i16_at};

pub(crate) const KIND: u32 = 157;

const RING: usize = 0x1c;
const RING_AT: usize = 0x30;
const CLOCK: usize = 0x1f0;
const SEED: usize = 0x1f8;
const HEAD: usize = 0x1fa;

/// FUN_1404cee40.
pub(crate) fn create(p: &Program, part: &mut Part, ctx: &mut Ctx) {
    let o = use_val(ctx, p, 0);
    part.set_u8(o + HEAD, 0);
    part.set_i16(o + 0x1f4, 0);
    part.set_i32(o + CLOCK, 0);
    let seed = ctx.rng.rand() as i16;
    part.set_i16(o + SEED, seed);
    part.zero(o, 0x30);
}

/// FUN_1404ceeb0: the colour state only.
pub(crate) fn restart(p: &Program, part: &mut Part, ctx: &mut Ctx) {
    let o = use_val(ctx, p, 0);
    part.zero(o, 0x30);
}

fn ring_at(o: usize, i: usize) -> usize {
    o + RING_AT + 16 * i
}

/// The point written into the ring: the cur translation, or (+0x53) the
/// instance world matrix applied to it with w = 1.
fn sample(ctx: &Ctx, cursor: usize, part: &Part) -> Vec4 {
    if byte(ctx, cursor + 0x53) == 0 {
        part.cur[3]
    } else {
        let mut v = math::apply(&ctx.world, part.cur[3]);
        v[3] = 1.;
        v
    }
}

/// FUN_1404ceab0.
pub(crate) fn update(p: &Program, cursor: usize, part: &mut Part, ctx: &mut Ctx) {
    if !active(part) {
        return;
    }
    let o = use_val(ctx, p, 0);
    if part.frame == 0 && !part.restarted {
        let mut fill = sample(ctx, cursor, part);
        fill[3] = 1.;
        for i in 0..RING {
            part.set_vec4(ring_at(o, i), fill);
        }
    }
    let head = match part.u8(o + HEAD) {
        0 => RING as u8,
        h => h,
    } - 1;
    part.set_u8(o + HEAD, head);
    let v = sample(ctx, cursor, part);
    part.set_vec4(ring_at(o, head as usize), v);
    // Colour integrators: rate += acceleration, value += rate.
    let s = |part: &Part, i: usize| part.i16(o + 2 * i);
    for i in 0..4 {
        part.set_i16(
            o + 2 * (8 + i),
            s(part, 8 + i).wrapping_add(s(part, 12 + i)),
        );
        part.set_i16(o + 2 * i, s(part, i).wrapping_add(s(part, 8 + i)));
        part.set_i16(
            o + 2 * (16 + i),
            s(part, 16 + i).wrapping_add(s(part, 20 + i)),
        );
        part.set_i16(
            o + 2 * (4 + i),
            s(part, 4 + i).wrapping_add(s(part, 16 + i)),
        );
    }
    if key(ctx, part, cursor) {
        // Head colour +0x20, tail colour +0x28, head rate +0x30, head
        // acceleration +0x38, tail rate +0x40, tail acceleration +0x48.
        for (var, at) in [
            (0, 0x20),
            (4, 0x28),
            (8, 0x30),
            (12, 0x38),
            (16, 0x40),
            (20, 0x48),
        ] {
            for i in 0..4 {
                let k = i16_at(&ctx.dpd.bytes, cursor + at + 2 * i).unwrap_or(0);
                part.set_i16(o + 2 * (var + i), s(part, var + i).wrapping_add(k));
            }
        }
    }
}

fn sub(a: Vec4, b: Vec4) -> Vec4 {
    std::array::from_fn(|i| a[i] - b[i])
}

fn length(v: Vec4) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn next(i: usize) -> usize {
    if i != RING - 1 { i + 1 } else { 0 }
}

/// FUN_1404ceef0.
pub(crate) fn draw(p: &Program, cursor: usize, part: &mut Part, ctx: &mut Ctx) {
    let shape = word(ctx, cursor + 4);
    if shape == 0xffff {
        return;
    }
    let Some(anim) = ctx.dpd.shapes.get(&(shape as i16)).map(|s| s.anim) else {
        return;
    };
    let (o, colour_var) = (use_val(ctx, p, 0), use_val(ctx, p, 1));
    let n = byte(ctx, cursor + 0x1c) as u32;
    let steps = n as i32 - 1;
    // Head and tail colour (bytes; alpha × the part alpha / 128) and the
    // per-sprite step between them.
    let fade = part.u8(colour_var + 0xb) as f32 * (1. / 128.);
    let read = |base: usize| -> Vec4 {
        let mut c: Vec4 = std::array::from_fn(|i| (part.i16(o + 2 * (base + i)) >> 7) as f32);
        c[3] *= fade;
        c
    };
    let mut colour = read(0);
    let tail = read(4);
    let divisor = if steps == 0 { 1e-6 } else { steps as f32 };
    let colour_step = sub(colour, tail).map(|c| c * (1. / divisor));
    let mut size = float(ctx, cursor + 0xc);
    let mut spacing = float(ctx, cursor + 0x18);
    let size_step = (size - float(ctx, cursor + 0x10)) / steps as f32;
    let spacing_step = size_step / size * spacing;
    let mut count = if spacing == 0. { 0 } else { n };
    // The path: the newest point, the segment to the next older one.
    let head = part.u8(o + HEAD) as usize;
    let ring = |i: usize| part.vec4(ring_at(o, i));
    let mut point = ring(head);
    let mut start = ring(head);
    let mut end_index = next(head);
    let mut end = ring(end_index);
    let mut segment = sub(end, start);
    let mut reach = length(segment);
    let mut seg_len = reach;
    let mut along = spacing;
    let clock = part.i32(o + CLOCK) as u32;
    let mut seed = part.i16(o + SEED) as u16 as u32;
    let wait = i16_at(&ctx.dpd.bytes, anim + 0x12).unwrap_or(0) as i32 as u32;
    let frames = i16_at(&ctx.dpd.bytes, anim + 6).unwrap_or(0) as i32 as u32;
    let to_view = if byte(ctx, cursor + 0x53) == 0 {
        ctx.ed60
    } else {
        ctx.view
    };
    let axis_x = math::scale(part.cur[0], ctx.scales[0]);
    let axis_y = math::scale(part.cur[1], ctx.scales[1]);
    let random_size = float(ctx, cursor + 0x14);
    let roll = i16_at(&ctx.dpd.bytes, cursor + 0x1e).unwrap_or(0) != 0;
    let mut first = byte(ctx, cursor + 0x1d) != 0;
    'sprites: loop {
        if !first {
            count = count.wrapping_sub(1);
            if count == 0 {
                break;
            }
            colour = sub(colour, colour_step);
            spacing -= spacing_step;
            size -= size_step;
            if 0. >= spacing {
                break;
            }
            if reach < spacing {
                loop {
                    let k = next(end_index);
                    if k == head {
                        break 'sprites;
                    }
                    start = end;
                    end = ring(k);
                    segment = sub(end, start);
                    along -= seg_len;
                    seg_len = length(segment);
                    reach += seg_len;
                    end_index = k;
                    if reach >= spacing {
                        break;
                    }
                }
            }
            let t = along / seg_len;
            point = std::array::from_fn(|i| start[i] + segment[i] * t);
            along += spacing;
            reach -= spacing;
        }
        first = false;
        seed = (seed * 0x80d + 7) & 0xffff;
        let Some(index) = clock
            .wrapping_add(seed)
            .checked_div(wait)
            .and_then(|v| v.checked_rem(frames))
        else {
            break;
        };
        let rgba = colour.map(|c| c as i64 as u8);
        let scale = (1. - (seed as f32 / 65535.) * random_size) * size;
        let keep_w = |v: Vec4| [v[0] * scale, v[1] * scale, v[2] * scale, v[3]];
        let mut w: Mat = [
            keep_w(axis_x),
            keep_w(axis_y),
            [0.; 4],
            math::apply(&to_view, [point[0], point[1], point[2], 1.]),
        ];
        if roll {
            let (s, c) = math::sin_cos(seed as i32);
            let (c0, c1) = (w[0], w[1]);
            w[0] = std::array::from_fn(|i| c1[i] * s + c0[i] * c);
            w[1] = std::array::from_fn(|i| c1[i] * c - c0[i] * s);
        }
        let w = math::mul(&ctx.ede0, &w);
        emit_sprites(ctx, anim, index as usize, &w, rgba);
    }
    if active(part) {
        let v = part
            .i32(o + CLOCK)
            .wrapping_add(word(ctx, cursor + 8) as i32);
        part.set_i32(o + CLOCK, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_walk_wraps() {
        assert_eq!(next(RING - 1), 0);
        assert_eq!(next(3), 4);
    }
}
