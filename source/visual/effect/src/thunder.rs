//! pppKeTh (129): branching lightning bolts. Translated from update
//! FUN_1404be900 (with the bolt spawn FUN_1404de490 / FUN_1404dd010, the
//! per-tick walk FUN_1404de9b0, the bolt growth FUN_1404dd8e0 and the
//! release FUN_1404dd790), draw FUN_1404bef80 → FUN_1404de5c0 →
//! FUN_1404dd100 (strip set-up) with FUN_1404dff80 / FUN_1404df640 /
//! FUN_1404deca0 / FUN_1404df370 (GS fan and strip packets, which
//! FUN_1404dfb20 turns into triangles), create FUN_1404beed0 and restart
//! FUN_1404bef20.
//!
//! A KeThRes block (work var u1) holds a pool of bolts, each a ring of
//! `points` positions. At the key at frame 0 a bolt starts at the part's
//! translation, heading along u2 (pppKeDrct) through the cur matrix. Every
//! tick each bolt re-walks its points from its head: each step turns the
//! direction toward the current target (a pppKeThTp block of another entry's
//! first part, chained through its +0x1c), by a random turn about X, Y or Z,
//! and (for points that already existed) back toward the old direction;
//! new points may fork child bolts. The draw is a 5-wide gouraud strip per
//! segment in view space (transparent edges, colour B inside, colour A on
//! the axis) with a fan at each end.
//!
//! Differences from the exe: bolt nodes live in Rust structs (the exe keeps
//! them in the KeThRes work block and links them by handles); target
//! references are kept as `1 + entry << 16 | var` (−1 = the scratch
//! vector). Killing the forks hanging from a point also forgets them there
//! (the exe leaves a stale list behind).
use crate::math::{self, Vec4};
use crate::programs::{active, byte, float, key, use_val, word};
use crate::{Ctx, Part, Triangle};
use data::object_effect::{Program, i16_at};

pub(crate) const KIND: u32 = 129;

/// A target reference meaning the scratch vector `DAT_140b607e8` (zeros).
pub(crate) const SCRATCH: i32 = -1;

/// The reference a KeThTp / KeTh key stores for entry `entry`'s first part,
/// work variable `var`.
pub(crate) fn encode(entry: i32, var: u32) -> i32 {
    1 + ((entry & 0x7fff) << 16 | (var as i32 & 0xffff))
}

#[derive(Clone, Default)]
struct Point {
    /// +0x00: position (w = 1).
    pos: Vec4,
    /// +0x1c: fade (0..1; the draw dims by `1 − k·fade`).
    fade: f32,
    /// +0x20: heading.
    dir: [f32; 3],
    /// +0x18: forks that start at this point.
    forks: Vec<usize>,
}

#[derive(Clone, Default)]
struct Bolt {
    /// +0x4e: 0x8 dying, 0x10 dead, 0x20 retracting, 0x40 finished, 0x80
    /// retract when full, 0x100 re-aim, 0x200 root follows the emitter,
    /// 0x400 unit headings, 0x800 reached the last target, 0x1000 drawn
    /// from the tip.
    flags: u16,
    /// +0x14: current target.
    target: i32,
    /// +0x18: length in points (fractional growth).
    length: f32,
    /// +0x1c: points dropped from the root so far.
    base: i32,
    /// +0x1e: forks made by this bolt and its parents.
    forks: u8,
    /// +0x40: colour offset of the root (points dropped, plus a fork's
    /// distance along its parent).
    offset: f32,
    /// +0x44: ring index of the root, +0x46: of the tip.
    head: usize,
    tip: usize,
    /// +0x48 / +0x4c: the drawn points as two ring runs (from the head,
    /// then from 0).
    runs: [usize; 2],
    points: Vec<Point>,
    /// +0x0c: the parent bolt point this fork hangs from.
    parent: Option<(usize, usize)>,
}

/// One KeThRes pool: its bolts, the free list and each KeTh's bolt list.
#[derive(Default)]
pub(crate) struct Pool {
    bolts: Vec<Bolt>,
    free: Vec<usize>,
    points: usize,
    lists: std::collections::HashMap<usize, Vec<usize>>,
}

/// The KeTh parameters at u0+0x10.. that the growth reads.
#[derive(Clone, Copy)]
struct Params {
    /// +0x50 / +0x51 / +0x52.
    max_forks: u8,
    fork_mask: u8,
    capacity: u8,
    /// +0x30: step length.
    step: f32,
    /// +0x34: how much a re-walked heading keeps of the new one.
    follow: f32,
    /// +0x38: random turn (radians at ±1).
    turn: f32,
    /// +0x3c: pull toward the target outside its near radius.
    pull: f32,
    /// +0x44: points grown per tick.
    speed: f32,
    /// +0x4c: random fade change per point.
    fade_turn: f32,
}

/// A pppKeThTp block: +0 position, +0xc mode (0: reached inside the
/// radius), +0x10 near radius, +0x14 near pull, +0x18 radius, +0x1c next.
#[derive(Clone, Copy, Default)]
struct Target {
    pos: Vec4,
    mode: f32,
    near: f32,
    near_pull: f32,
    radius: f32,
    next: i32,
}

fn target(ctx: &Ctx, reference: i32) -> Option<Target> {
    if reference == SCRATCH {
        return Some(Target::default());
    }
    if reference <= 0 {
        return None;
    }
    let v = reference - 1;
    let (entry, var) = ((v >> 16) as usize, (v & 0xffff) as usize);
    let vars = ctx.heads.get(entry).filter(|v| !v.is_empty())?;
    let f = |o: usize| {
        vars.get(var + o..var + o + 4)
            .map_or(0., |w| f32::from_le_bytes(w.try_into().unwrap()))
    };
    Some(Target {
        pos: [f(0), f(4), f(8), f(0xc)],
        mode: f(0xc),
        near: f(0x10),
        near_pull: f(0x14),
        radius: f(0x18),
        next: f(0x1c).to_bits() as i32,
    })
}

/// FUN_1404beed0.
pub(crate) fn create(p: &Program, part: &mut Part, ctx: &mut Ctx) {
    let o = use_val(ctx, p, 0);
    part.set_i32(o, 0);
    part.set_u8(o + 4, 0);
    restart(p, part, ctx);
    for pool in part.thunder.values_mut() {
        pool.lists.remove(&o);
    }
}

/// FUN_1404bef20: the colour, width and growth integrators.
pub(crate) fn restart(p: &Program, part: &mut Part, ctx: &mut Ctx) {
    let o = use_val(ctx, p, 0);
    part.zero(o + 0x70, 0x20);
    for at in [0x6c, 0x1c, 0x34, 0x3c] {
        part.set_f32(o + at, 0.);
    }
}

fn sub(a: Vec4, b: Vec4) -> Vec4 {
    std::array::from_fn(|i| a[i] - b[i])
}

fn length3(v: Vec4) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// FUN_140478d80: xyz normalised, w = 0.
fn unit(v: Vec4) -> Vec4 {
    let mut n = math::normalize([v[0], v[1], v[2], 0.]);
    n[3] = 0.;
    n
}

/// FUN_140141ae0 as FUN_140478cb0 calls it: `a·t + b·(1 − t)`.
fn lerp(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    std::array::from_fn(|i| b[i] * (1. - t) + a[i] * t)
}

fn clamp01(v: f32) -> f32 {
    let v = if 0. <= v { v } else { 0. };
    if v <= 1. { v } else { 1. }
}

/// The near-target pull of FUN_1404dd8e0.
fn pull(t: &Target, distance: f32, base: f32) -> f32 {
    if distance < t.near {
        (t.near_pull - ((distance - t.radius) * t.near_pull) / (t.near - t.radius)) + base
    } else {
        base
    }
}

/// A random turn: rand() picks X, Y or Z (table 0x140782710) and an angle
/// of `((r % 1001) − 500) · turn / 500` radians. Returns the rand() value.
fn turn(ctx: &mut Ctx, dir: Vec4, amount: f32) -> (Vec4, u32) {
    let r = ctx.rng.rand();
    let angle = ((r % 1001) as i32 - 500) as f32 * amount / 500.;
    let mut a = [0f32; 3];
    a[(r % 3) as usize] = angle;
    let m = math::rot_radians(&math::IDENTITY, a);
    (math::apply(&m, dir), r)
}

impl Pool {
    fn new(nodes: usize, points: usize) -> Self {
        Pool {
            bolts: vec![Bolt::default(); nodes],
            free: (0..nodes).rev().collect(),
            points,
            lists: Default::default(),
        }
    }

    /// FUN_1404dd010.
    fn init(&mut self, id: usize) {
        let b = &mut self.bolts[id];
        *b = Bolt {
            points: vec![Point::default(); self.points.max(1)],
            ..Bolt::default()
        };
        b.points[0].pos[3] = 1.;
    }

    /// Marks the forks hanging from point `k` of bolt `id` dying.
    fn kill_forks(&mut self, id: usize, k: usize, bits: u16) {
        for f in std::mem::take(&mut self.bolts[id].points[k].forks) {
            let fork = &mut self.bolts[f];
            fork.flags |= bits | 8;
            fork.parent = None;
        }
    }

    /// FUN_1404dd790: the forks on the drawn points die; the bolt leaves
    /// its parent point's list.
    fn release(&mut self, id: usize) {
        let bits = self.bolts[id].flags & 0x20;
        let [first, second] = self.bolts[id].runs;
        let head = self.bolts[id].head;
        let n = self.bolts[id].points.len();
        for k in (0..second).chain(head..head + first) {
            if k < n {
                self.kill_forks(id, k, bits);
            }
        }
        if let Some((parent, k)) = self.bolts[id].parent.take()
            && let Some(point) = self.bolts[parent].points.get_mut(k)
        {
            point.forks.retain(|&f| f != id);
        }
    }

    /// FUN_1404dd8e0: one tick of growth for the bolt at `list[at]`.
    fn grow(&mut self, list: &mut Vec<usize>, at: usize, p: Params, root: i32, ctx: &mut Ctx) {
        let id = list[at];
        let original = self.bolts[id].flags;
        let mut flags = original as u32;
        if flags & 0x800 != 0 {
            flags = (flags & !0x800) | 0x40;
        }
        if flags & 8 != 0 {
            if flags & 0x20 == 0 {
                self.bolts[id].flags = (flags | 0x10) as u16;
                return;
            }
            flags = (flags & !8) | 0x100;
        }
        let mut saved = flags;
        let n = self.bolts[id].points.len();
        let length = self.bolts[id].length;
        let mut base = self.bolts[id].base;
        let mut may_fork = p.fork_mask != 0xff && self.bolts[id].forks < p.max_forks;
        let unit_dirs = original & 0x400 != 0;
        let whole = length as i32;
        let mut new_length = length + p.speed;
        let grown = new_length as i32;
        let mut walk = grown - whole;
        let mut dropped = walk;
        if flags & 2 == 0 {
            if flags & 4 == 0 {
                let capacity = p.capacity as i32;
                if capacity <= grown {
                    if flags & 0x80 != 0 {
                        flags |= 0x20;
                    }
                    dropped = grown - capacity;
                    saved = flags;
                    if flags & 0x20 == 0 {
                        walk -= dropped;
                        new_length = capacity as f32;
                    } else {
                        flags = (flags & !0x200) | 4;
                        saved = flags;
                    }
                }
            }
        } else {
            walk = 0;
            new_length = length;
        }
        let entry_flags = flags;
        let mut shift = dropped;
        let mut reference;
        if flags & 0x20 == 0 {
            shift = 0;
            if flags & 0x100 == 0 {
                reference = self.bolts[id].target;
            } else {
                reference = root;
                self.bolts[id].target = root;
            }
        } else {
            base += dropped;
            reference = self.bolts[id].target;
            if target(ctx, reference).is_none() || whole <= base {
                self.bolts[id].flags |= 0x10;
                return;
            }
        }
        // Retracting: the root moves on, the forks on dropped points die.
        let mut cur = self.bolts[id].head;
        for _ in 0..shift.max(0) {
            self.kill_forks(id, cur, (flags & 0x20) as u16);
            cur = (cur + 1) % n;
        }
        if shift > 0 {
            flags = saved;
        }
        self.bolts[id].head = cur;
        self.bolts[id].offset += shift as f32;
        let mut t = target(ctx, reference).unwrap_or_default();
        let mut aim = t.pos;
        let point = &self.bolts[id].points[cur];
        let mut dir = [point.dir[0], point.dir[1], point.dir[2], 0.];
        let mut pos = point.pos;
        let existing = whole - base;
        walk += existing;
        let mut i = 0;
        if flags & 0x120 == 0x100 {
            let to = sub(aim, pos);
            let k = pull(&t, length3(to), p.pull);
            dir = lerp(unit(to), dir, k);
            if unit_dirs {
                dir = unit(dir);
            }
            dir = turn(ctx, dir, p.turn).0;
            let d = self.bolts[id].points[cur].dir;
            dir = lerp(dir, [d[0], d[1], d[2], 0.], p.follow);
            if unit_dirs {
                dir = unit(dir);
            }
        }
        let finish = |pool: &mut Pool, tip: usize, base: i32, length: f32, flags: u32| {
            let b = &mut pool.bolts[id];
            b.runs = match b.head.cmp(&tip) {
                std::cmp::Ordering::Less => [tip - b.head + 1, 0],
                std::cmp::Ordering::Greater => [n - b.head, tip + 1],
                std::cmp::Ordering::Equal => [0, 0],
            };
            b.base = base;
            b.length = length;
            b.flags = flags as u16;
        };
        loop {
            let here = cur;
            let to = sub(aim, pos);
            let distance = length3(to);
            let inside = if t.mode == 0. {
                t.radius < distance
            } else {
                distance < t.radius
            };
            if !inside && t.radius != distance {
                match target(ctx, t.next) {
                    None => {
                        saved |= 0x800;
                        let done = i + 1;
                        self.bolts[id].tip = cur;
                        let length = (base + i) as f32;
                        flags |= 0x800;
                        if done <= existing {
                            let mut k = cur;
                            for _ in 0..existing - done + 1 {
                                k = (k + 1) % n;
                                self.kill_forks(id, k, (entry_flags & 0x20) as u16);
                            }
                            flags = saved;
                        }
                        if entry_flags & 0x80 != 0 {
                            flags = entry_flags | 0x820;
                        }
                        finish(self, self.bolts[id].tip, base, length, flags);
                        return;
                    }
                    Some(next) => {
                        if i == 0 {
                            self.bolts[id].target = t.next;
                        }
                        reference = t.next;
                        t = next;
                        aim = t.pos;
                    }
                }
            }
            let done = i + 1;
            if walk < done {
                self.bolts[id].tip = cur;
                finish(self, cur, base, new_length, flags);
                return;
            }
            let next = (here + 1) % n;
            cur = next;
            let from = pos;
            let from_dir = dir;
            pos = std::array::from_fn(|k| pos[k] + dir[k] * p.step);
            self.bolts[id].points[next].pos[..3].copy_from_slice(&pos[..3]);
            let k = pull(&t, distance, p.pull);
            dir = lerp(unit(to), dir, k);
            if unit_dirs {
                dir = unit(dir);
            }
            let (turned, r) = turn(ctx, dir, p.turn);
            dir = turned;
            let jitter = (r as f32 * 4.656_613e-10 + r as f32 * 4.656_613e-10) - 1.;
            let previous_fade = self.bolts[id].points[here].fade;
            if existing < done {
                let point = &mut self.bolts[id].points[next];
                point.forks.clear();
                point.pos[3] = 1.;
                point.fade = clamp01(jitter * p.fade_turn + previous_fade);
                if may_fork && (p.fork_mask as u32 & r) == 0 {
                    match self.free.pop() {
                        None => may_fork = false,
                        Some(f) => {
                            self.bolts[id].forks += 1;
                            self.init(f);
                            let parent = &self.bolts[id];
                            let (forks, offset) = (parent.forks, parent.offset);
                            let fork = &mut self.bolts[f];
                            fork.flags |= original & 0x1400;
                            fork.target = reference;
                            fork.forks = forks;
                            fork.offset = (whole as f32 + offset + done as f32) - existing as f32;
                            fork.points[0].pos = [from[0], from[1], from[2], 1.];
                            fork.points[0].dir = [from_dir[0], from_dir[1], from_dir[2]];
                            fork.points[0].fade = previous_fade;
                            fork.parent = Some((id, next));
                            list.insert(at + 1, f);
                            self.bolts[id].points[next].forks.insert(0, f);
                        }
                    }
                    if p.max_forks <= self.bolts[id].forks {
                        may_fork = false;
                    }
                }
            } else {
                let d = self.bolts[id].points[next].dir;
                dir = lerp(dir, [d[0], d[1], d[2], 0.], p.follow);
                if unit_dirs {
                    dir = unit(dir);
                }
                let fade = clamp01(jitter * p.fade_turn + previous_fade);
                let point = &mut self.bolts[id].points[next];
                point.fade += (fade - point.fade) * p.follow;
                let (fade, forks) = (point.fade, point.forks.clone());
                for f in forks {
                    let fork = &mut self.bolts[f];
                    let h = fork.head;
                    if let Some(root) = fork.points.get_mut(h) {
                        root.pos = [from[0], from[1], from[2], root.pos[3]];
                        root.dir = [from_dir[0], from_dir[1], from_dir[2]];
                        root.fade = fade;
                    }
                    fork.target = reference;
                }
            }
            self.bolts[id].points[next].dir = [dir[0], dir[1], dir[2]];
            i = done;
        }
    }
}

/// FUN_1404be900.
pub(crate) fn update(p: &Program, cursor: usize, part: &mut Part, ctx: &mut Ctx) {
    if !active(part) {
        return;
    }
    let (u0, u1, u2) = (use_val(ctx, p, 0), use_val(ctx, p, 1), use_val(ctx, p, 2));
    let m = part.cur;
    let dir = unit(math::apply(&m, part.vec4(u2)));
    let pos = m[3];
    if key(ctx, part, cursor) {
        if word(ctx, cursor) == 0 {
            let entry = word(ctx, cursor + 4) as i32;
            let reference = if entry == -1 {
                SCRATCH
            } else {
                encode(entry, word(ctx, cursor + 8))
            };
            if part.i32(u0 + 0x64) == SCRATCH {
                return;
            }
            part.set_i32(u0 + 0x64, reference);
            part.set_i32(u0 + 0x60, u1 as i32 + 1);
            let mut bits = part.u8(u0 + 4);
            if byte(ctx, cursor + 0x56) == 0 {
                bits |= 8;
            }
            if byte(ctx, cursor + 0x55) != 0 {
                bits |= 4;
            }
            if byte(ctx, cursor + 0x58) != 0 {
                bits |= 0x10;
            }
            if byte(ctx, cursor + 0x57) != 0 {
                bits |= 0x20;
            }
            part.set_u8(u0 + 4, bits);
            spawn(part, u0, u1, pos, dir);
        }
        for i in 0..16 {
            let k = i16_at(&ctx.dpd.bytes, cursor + 0xc + 2 * i).unwrap_or(0);
            let v = part.i16(u0 + 0x70 + 2 * i).wrapping_add(k);
            part.set_i16(u0 + 0x70 + 2 * i, v);
        }
        part.set_i32(u0 + 0x68, word(ctx, cursor + 0x2c) as i32);
        let v = float(ctx, cursor + 0x48) + part.f32(u0 + 0x6c);
        part.set_f32(u0 + 0x6c, v);
        let segments = part.u8(u1 + 1).wrapping_sub(1);
        part.set_u8(u0 + 0x52, segments);
        let width = float(ctx, cursor + 0x30) + part.f32(u0 + 0x1c);
        part.set_f32(u0 + 0x1c, width);
        part.set_i32(u0 + 0x40, word(ctx, cursor + 0x38) as i32);
        part.set_i32(u0 + 0x48, word(ctx, cursor + 0x3c) as i32);
        part.set_i32(u0 + 0x4c, word(ctx, cursor + 0x40) as i32);
        let v = float(ctx, cursor + 0x44) + part.f32(u0 + 0x34);
        part.set_f32(u0 + 0x34, v);
        let v = float(ctx, cursor + 0x4c) + part.f32(u0 + 0x3c);
        part.set_f32(u0 + 0x3c, v);
        let degrees = i16_at(&ctx.dpd.bytes, cursor + 0x50).unwrap_or(0) as u16 as f32;
        part.set_f32(u0 + 0x38, degrees * std::f32::consts::PI * 0.017_453_292);
        part.set_u8(u0 + 0x50, byte(ctx, cursor + 0x52));
        part.set_u8(u0 + 0x51, byte(ctx, cursor + 0x53));
        let taper = (width - width * float(ctx, cursor + 0x34)) / segments as f32;
        part.set_f32(u0 + 0x2c, -taper);
    }
    let step = part.f32(u0 + 0x68) / part.u8(u0 + 0x52) as f32;
    part.set_f32(u0 + 0x30, step);
    part.set_f32(u0 + 0x44, part.f32(u0 + 0x6c) / step);
    for i in 0..8 {
        part.set_u8(u0 + 0x10 + i, (part.i16(u0 + 0x70 + 2 * i) >> 7) as u8);
        part.set_u8(u0 + 0x20 + i, (part.i16(u0 + 0x80 + 2 * i) >> 7) as u8);
    }
    tick(part, u0, u1, pos, dir, ctx);
}

/// The KeThRes pool of work var `res` (built from its header on first use).
fn pool(part: &mut Part, res: usize) -> &mut Pool {
    let (nodes, points) = (part.u8(res) as usize, part.u8(res + 1) as usize);
    part.thunder
        .entry(res)
        .or_insert_with(|| Pool::new(nodes, points))
}

/// FUN_1404de490: a new bolt from the pool, rooted at `pos` heading `dir`,
/// first in the KeTh's list.
fn spawn(part: &mut Part, u0: usize, res: usize, pos: Vec4, dir: Vec4) {
    let bits = part.u8(u0 + 4) as u16;
    let pool = pool(part, res);
    let Some(id) = pool.free.pop() else {
        return;
    };
    pool.init(id);
    let mut flags = ((bits & 4) | 8) << 5;
    if bits & 0x10 != 0 {
        flags |= 0x200;
    }
    if bits & 8 != 0 {
        flags |= 0x400;
    }
    if bits & 0x20 != 0 {
        flags |= 0x1000;
    }
    let b = &mut pool.bolts[id];
    b.flags |= flags;
    b.points[0].pos = [pos[0], pos[1], pos[2], 1.];
    b.points[0].dir = [dir[0], dir[1], dir[2]];
    pool.lists.entry(u0).or_default().insert(0, id);
}

/// FUN_1404de9b0: every bolt of the list grows; dead ones go back to the
/// pool.
fn tick(part: &mut Part, u0: usize, res: usize, pos: Vec4, dir: Vec4, ctx: &mut Ctx) {
    let mut bits = part.u8(u0 + 4);
    let mut dying = 0;
    if bits & 2 != 0 {
        dying = 0x20;
        bits &= !2;
        part.set_u8(u0 + 4, bits);
    }
    let params = Params {
        max_forks: part.u8(u0 + 0x50),
        fork_mask: part.u8(u0 + 0x51),
        capacity: part.u8(u0 + 0x52),
        step: part.f32(u0 + 0x30),
        follow: part.f32(u0 + 0x34),
        turn: part.f32(u0 + 0x38),
        pull: part.f32(u0 + 0x3c),
        speed: part.f32(u0 + 0x44),
        fade_turn: part.f32(u0 + 0x4c),
    };
    let root = part.i32(u0 + 0x64);
    let pool = pool(part, res);
    let mut list = pool.lists.remove(&u0).unwrap_or_default();
    let mut at = 0;
    while at < list.len() {
        let id = list[at];
        let b = &mut pool.bolts[id];
        if b.flags & 0x200 != 0 {
            let h = b.head;
            if let Some(point) = b.points.get_mut(h) {
                point.pos = [pos[0], pos[1], pos[2], point.pos[3]];
                point.dir = [dir[0], dir[1], dir[2]];
            }
        }
        if b.flags & 0x100 != 0 {
            b.flags |= dying;
        }
        pool.grow(&mut list, at, params, root, ctx);
        if pool.bolts[id].flags & 0x10 != 0 {
            list.remove(at);
            pool.release(id);
            pool.free.push(id);
        } else {
            at += 1;
        }
    }
    pool.lists.insert(u0, list);
}

/// FUN_1404bef80 → FUN_1404de5c0: every bolt of the list as gouraud fans
/// and strips. Blend from key +0x54: 0 additive (0x48), 1 subtractive
/// (0x42), else replace (0xa8, FIX 0x80).
pub(crate) fn draw(p: &Program, cursor: usize, part: &mut Part, ctx: &mut Ctx) {
    let (u0, u1) = (use_val(ctx, p, 0), use_val(ctx, p, 1));
    let blend = match byte(ctx, cursor + 0x54) {
        0 => 0x48,
        1 => 0x42,
        _ => 0xa8,
    };
    let Some(pool) = part.thunder.get(&u1) else {
        return;
    };
    let Some(list) = pool.lists.get(&u0) else {
        return;
    };
    for &id in list {
        strip(ctx, part, u0, &pool.bolts[id], blend);
    }
}

/// Colour bytes of the GS vertex (low bytes of the converted ints).
fn bytes(c: Vec4, alpha: Option<u8>) -> [u8; 4] {
    let b = |v: f32| crate::models::cvtt(v) as u8;
    [b(c[0]), b(c[1]), b(c[2]), alpha.unwrap_or_else(|| b(c[3]))]
}

/// FUN_14013ff90 with 0 and 255.
fn clamp255(c: Vec4) -> Vec4 {
    c.map(|v| {
        let v = if 0. <= v { v } else { 0. };
        if 255. <= v { 255. } else { v }
    })
}

fn norm2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l != 0. {
        [v[0] / l, v[1] / l]
    } else {
        [0., 0.]
    }
}

/// The GS screen centre of the exe's view→screen matrix (`ede0`,
/// FUN_14019dc50 arguments DAT_1405b2c08 = 2048).
const SCREEN_CENTRE: f32 = 2048.;

/// A bolt point in the exe's screen space: `ede0 · view · world · p`, kept
/// with its view-space position (the offsets are added to the screen x/y
/// and divided by w = view z, so a screen offset `d` is `d / scale` in view
/// space at any depth).
#[derive(Clone, Copy)]
struct Clip {
    view: Vec4,
    x: f32,
    y: f32,
    inv_w: f32,
}

/// FUN_1404dd100 with its fan/strip builders: the bolt's points (two ring
/// runs from the root, or reversed with flag 0x1000) through the exe's
/// screen matrix as a 5-wide strip: transparent edges (colour B), colour B
/// at `±frac` of the half width, colour A on the axis. The half width (in
/// screen units) starts at +0x1c and changes by +0x2c per point (never below
/// 0); colours change by the signed bytes +0x20 (A) and +0x24 (B) per point;
/// `1 − (+0x48)·fade` dims the width. A fan closes each end. Corners come
/// back to draw space through [`Ctx::screen`].
fn strip(ctx: &mut Ctx, part: &Part, u0: usize, b: &Bolt, blend: u8) {
    let [first, second] = b.runs;
    if first + second < 3 {
        return;
    }
    let colour = |at: usize, signed: bool| -> Vec4 {
        std::array::from_fn(|i| {
            let v = part.u8(u0 + at + i);
            let v = if signed { v as i8 as f32 } else { v as f32 };
            v * ctx.colour[i]
        })
    };
    let (mut a, mut bc) = (colour(0x10, false), colour(0x14, false));
    let (da, db) = (colour(0x20, true), colour(0x24, true));
    let mut width = part.f32(u0 + 0x1c);
    let fade_k = part.f32(u0 + 0x48);
    let frac = part.f32(u0 + 0x40);
    let width_step = part.f32(u0 + 0x2c);
    let (sx, sy) = (ctx.scales[0], ctx.scales[1]);
    let [fx, fy] = ctx.screen;
    let n = b.points.len().max(1);
    let order: Vec<usize> = if b.flags & 0x1000 == 0 {
        let k = b.offset;
        a = clamp255(std::array::from_fn(|i| da[i] * k + a[i]));
        bc = clamp255(std::array::from_fn(|i| db[i] * k + bc[i]));
        let w = k * width_step + width;
        width = if w < 0. { 0. } else { w };
        (b.head..b.head + first).chain(0..second).collect()
    } else {
        let tip_run = (0..second).rev();
        let root_run = (0..first).map(|i| (first + b.head).wrapping_sub(1 + i));
        tip_run.chain(root_run).collect()
    };
    let points: Vec<&Point> = order
        .into_iter()
        .filter_map(|k| b.points.get(k % n))
        .collect();
    let clips: Vec<Clip> = points
        .iter()
        .map(|pt| {
            let view = math::apply(&ctx.ed60, pt.pos);
            Clip {
                view,
                x: fx * view[0] + SCREEN_CENTRE * view[2],
                y: fy * view[1] + SCREEN_CENTRE * view[2],
                inv_w: 1. / view[2],
            }
        })
        .collect();
    // Behind the eye the exe's w is not positive (the GS clips it); no
    // draw-space form exists.
    if points.len() < 3
        || clips
            .iter()
            .any(|c| c.view[2].partial_cmp(&0.) != Some(std::cmp::Ordering::Greater))
    {
        return;
    }
    let ede0 = ctx.ede0;
    let corner = move |c: &Clip, o: [f32; 2], k: f32| -> [f32; 3] {
        let v = math::apply(
            &ede0,
            [
                c.view[0] + o[0] * k / fx,
                c.view[1] + o[1] * k / fy,
                c.view[2],
                1.,
            ],
        );
        [v[0], v[1], v[2]]
    };
    let push = |ctx: &mut Ctx, v: [([f32; 3], [u8; 4]); 3]| {
        ctx.triangles.push(Triangle {
            corners: v.map(|x| x.0),
            colours: v.map(|x| x.1),
            blend,
        });
    };
    let fan = |ctx: &mut Ctx, v: &[([f32; 3], [u8; 4]); 6]| {
        for i in 1..5 {
            push(ctx, [v[0], v[i], v[i + 1]]);
        }
    };
    // FUN_1404dff80: the first two points; the first heading is the raw
    // screen-space difference (before the divide by w).
    let mut p0 = clips[0];
    let mut p1 = clips[1];
    let mut heading = norm2([p1.x - p0.x, p1.y - p0.y]);
    let mut side = [0f32; 2];
    let mut bend = |pn: &Clip, p1: &Clip, fade: f32, width: &mut f32| {
        let nd = norm2([
            pn.x * pn.inv_w - p1.x * p1.inv_w,
            pn.y * pn.inv_w - p1.y * p1.inv_w,
        ]);
        let avg = norm2([heading[0] + nd[0], heading[1] + nd[1]]);
        let w = *width + width_step;
        *width = if w < 0. { 0. } else { w };
        heading = nd;
        [avg[1] * *width * sx * fade, -(avg[0] * *width) * sy * fade]
    };
    let int = |c: Vec4| c.map(|v| crate::models::cvtt(v) as f32);
    let zero = [0f32; 2];
    for (k, (point, pn)) in points.iter().zip(&clips).enumerate().skip(2) {
        let fade = 1. - fade_k * point.fade;
        if k == 2 {
            // FUN_1404df640: the root fan.
            let a_prev = int(a);
            bc = clamp255(std::array::from_fn(|i| bc[i] + db[i]));
            let b_cur = int(bc);
            let perp = bend(pn, &p1, fade, &mut width);
            a = clamp255(std::array::from_fn(|i| a[i] + da[i]));
            let a_cur = int(a);
            fan(
                ctx,
                &[
                    (corner(&p0, zero, 0.), bytes(a_prev, Some(0))),
                    (corner(&p1, perp, 1.), bytes(b_cur, Some(0))),
                    (corner(&p1, perp, frac), bytes(b_cur, None)),
                    (corner(&p1, zero, 0.), bytes(a_cur, None)),
                    (corner(&p1, perp, -frac), bytes(b_cur, None)),
                    (corner(&p1, perp, -1.), bytes(b_cur, Some(0))),
                ],
            );
            side = perp;
        } else {
            // FUN_1404deca0: a segment strip.
            let b_prev = int(bc);
            bc = clamp255(std::array::from_fn(|i| bc[i] + db[i]));
            let b_cur = int(bc);
            let perp = bend(pn, &p1, fade, &mut width);
            let a_prev = int(a);
            a = clamp255(std::array::from_fn(|i| a[i] + da[i]));
            let a_cur = int(a);
            let v = [
                (corner(&p0, side, 1.), bytes(b_prev, Some(0))),
                (corner(&p1, perp, 1.), bytes(b_cur, Some(0))),
                (corner(&p0, side, frac), bytes(b_prev, None)),
                (corner(&p1, perp, frac), bytes(b_cur, None)),
                (corner(&p0, zero, 0.), bytes(a_prev, None)),
                (corner(&p1, zero, 0.), bytes(a_cur, None)),
                (corner(&p0, side, -frac), bytes(b_prev, None)),
                (corner(&p1, perp, -frac), bytes(b_cur, None)),
                (corner(&p0, side, -1.), bytes(b_prev, Some(0))),
                (corner(&p1, perp, -1.), bytes(b_cur, Some(0))),
            ];
            for i in 0..8 {
                push(ctx, [v[i], v[i + 1], v[i + 2]]);
            }
            side = perp;
        }
        p0 = p1;
        p1 = *pn;
    }
    // FUN_1404df370: the tip fan (the tip's alpha byte is the low byte of
    // the unclamped float, as the exe passes it).
    let a_prev = int(a);
    let raw: Vec4 = std::array::from_fn(|i| a[i] + da[i]);
    a = clamp255(raw);
    let tip_alpha = raw[3].to_bits() as u8;
    let b_prev = int(bc);
    fan(
        ctx,
        &[
            (corner(&p1, zero, 0.), bytes(int(a), Some(tip_alpha))),
            (corner(&p0, side, 1.), bytes(b_prev, Some(0))),
            (corner(&p0, side, frac), bytes(b_prev, None)),
            (corner(&p0, zero, 0.), bytes(a_prev, None)),
            (corner(&p0, side, -frac), bytes(b_prev, None)),
            (corner(&p0, side, -1.), bytes(b_prev, Some(0))),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_round_trip() {
        let r = encode(3, 0x40);
        assert!(r > 0);
        let v = r - 1;
        assert_eq!(((v >> 16), v & 0xffff), (3, 0x40));
        assert_eq!(clamp01(-1.), 0.);
        assert_eq!(clamp01(2.), 1.);
        assert_eq!(lerp([1.; 4], [0.; 4], 0.25), [0.25; 4]);
    }
}
