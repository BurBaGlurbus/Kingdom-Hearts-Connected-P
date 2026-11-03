//! The route follower the `5:*` traps drive (the object's route at sysobj
//! +0x630), translated from the executable's 0xb0-byte class: FUN_14054df70
//! (construction), FUN_14054e130 (from a spawn point's walk path),
//! FUN_14054e3f0 (an "active" path that follows another object),
//! FUN_1405503e0 (the Hermite tangents), FUN_14054ea80 (the Hermite point),
//! FUN_14054ec00 (its derivative), FUN_14054ef80 / FUN_14054f210 /
//! FUN_1405510d0 / FUN_1405513a0 (nearest point on the path) and
//! FUN_14054f9d0 (the point a given distance ahead).
//!
//! Positions are in the scripts' frame (+Y down, four lanes, w = 1), the way
//! the walk paths of the area archives hold them. The 0x20-byte nodes hold a
//! position and a tangent; the tangent's w lane carries the node's index.
//!
//! [U] FUN_14054f9d0 compares the new projection with the old curve point;
//! the decompilation's first distance operand is unclear and this reads it as
//! the old curve point's distance. FUN_1405513a0 / FUN_1405515c0 return the
//! best distance in the executable (the decompiler shows no return value).

pub type V4 = [f32; 4];

/// One walk path of a spawn point (the area archive's record): its id, its
/// flags byte (bit 0 closed, bit 1 no smoothing), its group byte and the
/// points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WalkPath {
    pub id: u16,
    pub flags: u8,
    pub group: u8,
    pub points: Vec<[f32; 3]>,
}

/// How a path is asked for: by its id (below zero: the first one) or by its
/// group byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKey {
    Id(i32),
    Group(u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Node {
    pub position: V4,
    pub tangent: V4,
}

/// What the building routine reads of the object (FUN_14054e130).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Build {
    /// The object follows routes: +0x6c8 bit 3.
    pub routine: bool,
    /// Routine flag 3 on such an object: every point takes the first one's
    /// height.
    pub flatten: bool,
    /// Routine flags 1 and 2.
    pub start_at_object: bool,
    pub one_way: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    /// +4 and +8: the first live node and how many there are; +0xc the
    /// ring's capacity.
    start: i32,
    count: i32,
    capacity: i32,
    /// +0x10: the path closes on itself.
    closed: bool,
    /// +0x18: the parameter of the position on the path (node index plus
    /// fraction); +0x1c the progress of the look-ahead.
    pub t: f32,
    pub progress: f32,
    /// +0x20: the tangent scale.
    tension: f32,
    /// +0x24: an active path adds points once the target moved this far.
    radius: f32,
    /// +0x28: the length of the segment being walked.
    chord: f32,
    /// +0x2c / +0x4c: the segment being walked (its tangents hold the node
    /// indices in w).
    seg_start: Node,
    seg_end: Node,
    /// +0x6c: the last position asked about.
    last_position: V4,
    nodes: Vec<Node>,
    /// +0x88: walking towards higher nodes.
    pub forward: bool,
    /// +0x89: the walker turns round at the end of an open path.
    pub bounce: bool,
    /// +0x8a: the direction is fixed (it cannot be reversed).
    pub fixed: bool,
    /// +0x8b: an active path, built from the positions of `target`.
    pub active: bool,
    /// +0x90: the object an active path follows.
    pub target: Option<u32>,
    /// +0x98: the node index at which `forward` flips to follow the path
    /// back (set by `set_path_target_point`), -1 for none.
    pub stop_index: i32,
}

// --- The executable's vector helpers (xyzw lanes) -----------------------------

pub fn sub(a: &V4, b: &V4) -> V4 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}
pub fn add(a: &V4, b: &V4) -> V4 {
    [b[0] + a[0], b[1] + a[1], b[2] + a[2], b[3] + a[3]]
}
pub fn scale(a: &V4, s: f32) -> V4 {
    [s * a[0], s * a[1], s * a[2], s * a[3]]
}
/// FUN_140140200.
fn dot3(a: &V4, b: &V4) -> f32 {
    a[1] * b[1] + a[0] * b[0] + a[2] * b[2]
}
/// FUN_140140f50.
pub fn len3(a: &V4) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
/// FUN_1401a8ef0.
fn len_sq(a: &V4) -> f32 {
    a[0] * a[0] + a[1] * a[1] + a[2] * a[2]
}
/// FUN_140142080: normalise xyz in place, returning the length.
pub fn normalize(a: &mut V4) -> f32 {
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if len != 0.0 {
        let inv = 1.0 / len;
        a[0] *= inv;
        a[1] *= inv;
        a[2] *= inv;
    }
    len
}

/// FUN_14054ea80: the point at fraction `t` of the Hermite segment `a`..`b`.
pub fn hermite(t: f32, a: &Node, b: &Node) -> V4 {
    let t2 = t * t;
    let t3 = t2 * t;
    let bt = scale(&b.tangent, t3 - t2);
    let at = scale(&a.tangent, (t3 - (t2 + t2)) + t);
    let t2_3 = t2 * 3.0;
    let bp = scale(&b.position, t2_3 - (t3 + t3));
    let ap = scale(&a.position, ((t3 + t3) - t2_3) + 1.0);
    let sum = add(&add(&add(&ap, &bp), &at), &bt);
    [sum[0], sum[1], sum[2], 1.0]
}

/// FUN_14054ec00: the derivative of the segment `a`..`b` at fraction `u`.
fn hermite_slope(u: f32, a: &Node, b: &Node) -> V4 {
    let u3 = u * u * 3.0;
    let bt = scale(&b.tangent, u3 - (u + u));
    let at = scale(&a.tangent, (u3 - u * 4.0) + 1.0);
    let u6 = u * 6.0;
    let u2_6 = u * u * 6.0;
    let bp = scale(&b.position, u6 - u2_6);
    let ap = scale(&a.position, u2_6 - u6);
    add(&add(&add(&ap, &bp), &at), &bt)
}

/// FUN_14054ef80: the fraction along the segment `a`..`b` of the point
/// nearest to `p`, and that point's distance from `p`.
fn segment_distance(p: &V4, a: &V4, b: &V4) -> (f32, f32) {
    let mut d = sub(b, a);
    let mut e = sub(p, a);
    let along = dot3(&d, &e);
    if along < 0.0 {
        return (0.0, len3(&e));
    }
    let len = normalize(&mut d);
    e[3] = 0.0;
    // The matrix FUN_14013f900 applies has the rows d*d.x, d*d.y, d*d.z and
    // (0, 0, 0, 1): the projection of e on d.
    let rows = [
        scale(&d, d[0]),
        scale(&d, d[1]),
        scale(&d, d[2]),
        [0.0, 0.0, 0.0, 1.0],
    ];
    let proj: V4 = std::array::from_fn(|k| {
        rows[0][k] * e[0] + rows[1][k] * e[1] + rows[2][k] * e[2] + rows[3][k] * e[3]
    });
    let along_len = len3(&proj);
    if along_len <= len {
        let off = len_sq(&e) - len_sq(&proj);
        let dist = if off > 0.0 { off.abs().sqrt() } else { 0.0 };
        let t = if len == 0.0 { 0.0 } else { along_len / len };
        (t, dist)
    } else {
        (1.0, len3(&sub(p, b)))
    }
}

impl Route {
    /// FUN_14054df70: an empty route.
    fn empty() -> Self {
        let unit = [0.0, 0.0, 0.0, 1.0];
        let node = Node {
            position: unit,
            tangent: unit,
        };
        Self {
            start: 0,
            count: 0,
            capacity: 0,
            closed: false,
            t: 0.0,
            progress: -2.0,
            tension: 0.5,
            radius: 20.0,
            chord: 0.0,
            seg_start: node,
            seg_end: node,
            last_position: unit,
            nodes: Vec::new(),
            forward: true,
            bounce: false,
            fixed: false,
            active: false,
            target: None,
            stop_index: -1,
        }
    }

    /// FUN_14054e130 and the work FUN_1403b9de0 does around it: the route of
    /// a walk path. `rand` is the C library's `rand()`. Returns the route and,
    /// when the object starts at a random place on it, that place.
    pub fn from_path(
        path: &WalkPath,
        build: Build,
        object_position: &V4,
        rand: &mut dyn FnMut() -> i32,
    ) -> (Self, Option<V4>) {
        let mut route = Self::empty();
        let n = path.points.len() as i32;
        route.count = n;
        route.capacity = n;
        let first_y = path.points.first().map_or(0.0, |p| p[1]);
        route.nodes = path
            .points
            .iter()
            .map(|p| Node {
                position: [p[0], if build.flatten { first_y } else { p[1] }, p[2], 1.0],
                tangent: [0.0; 4],
            })
            .collect();
        route.closed = path.flags & 1 != 0;
        route.tension = if path.flags & 2 == 0 { 0.5 } else { 0.0 };
        route.compute_tangents();
        let mut place = None;
        if !build.routine {
            route.fixed = false;
            route.reset_to(object_position);
        } else {
            route.fixed = build.one_way;
            if !build.start_at_object && n >= 2 {
                // A random place and direction on the path.
                let spread = if route.closed { n } else { n - 1 } as f32;
                let at = rand() as f32 * spread * 3.051_757_8e-5;
                let whole = at as i32;
                route.forward = (rand() as f32 * 2.0 * 3.051_757_8e-5) as i64 != 0;
                let a = route.node(whole);
                let b = route.node((whole + 1) % route.count);
                place = Some(hermite(at - whole as f32, &a, &b));
                route.t = at;
            } else {
                route.reset_to(object_position);
            }
        }
        route.bounce = !route.fixed;
        (route, place)
    }

    /// FUN_14054e3f0: an active path that follows `target`, from `object`'s
    /// position (nothing is added when it is already near).
    pub fn active(target: u32, object_position: &V4, target_position: &V4) -> Self {
        let mut route = Self::empty();
        route.fixed = false;
        route.capacity = 0x20;
        route.nodes = vec![Node::default(); 0x20];
        route.active = true;
        route.set_target(Some(target), object_position, target_position);
        route
    }

    fn node(&self, i: i32) -> Node {
        if self.capacity <= 0 {
            return Node::default();
        }
        self.nodes[i.rem_euclid(self.capacity) as usize]
    }

    /// The nodes of the segment that starts at node `i`: the node and the
    /// next one on the loop.
    fn pair(&self, i: i32) -> (Node, Node) {
        let next = if self.count > 0 {
            (i + 1) % self.count
        } else {
            0
        };
        (self.node(i), self.node(next))
    }

    /// FUN_1405503e0: the tangents of every node.
    fn compute_tangents(&mut self) {
        let n = self.count;
        if n < 3 {
            if let [a, b] = &mut self.nodes[..] {
                a.tangent = sub(&b.position, &a.position);
                b.tangent = a.tangent;
            }
            self.closed = false;
            return;
        }
        // The end tangent of an open path: a parabola through three points.
        let end = |p0: &V4, p1: &V4, p2: &V4, tension: f32| -> (V4, f32, V4) {
            let a = len3(&sub(p1, p0));
            let b = len3(&sub(p2, p1));
            let s = a / (b + a);
            let c = 1.0 - s;
            let q = scale(
                &sub(&sub(p1, &scale(&scale(p0, c), c)), &scale(&scale(p2, s), s)),
                1.0 / ((s + s) * c),
            );
            let v = add(
                &add(&scale(p0, c * -2.0), &scale(&q, 2.0 - s * 4.0)),
                &scale(&scale(p2, 2.0), s),
            );
            let ratio = len3(&scale(&sub(p2, p0), tension)) / len3(&v);
            (q, ratio, v)
        };
        let nodes = self.nodes.clone();
        for i in 0..n {
            let iu = i as usize;
            let tangent = if !self.closed && i == 0 {
                let (q, ratio, _) = end(
                    &nodes[0].position,
                    &nodes[1].position,
                    &nodes[2].position,
                    self.tension,
                );
                scale(&scale(&sub(&q, &nodes[0].position), 2.0), ratio)
            } else if !self.closed && i == n - 1 {
                let (q, ratio, _) = end(
                    &nodes[iu - 2].position,
                    &nodes[iu - 1].position,
                    &nodes[iu].position,
                    self.tension,
                );
                scale(&scale(&sub(&nodes[iu].position, &q), 2.0), ratio)
            } else {
                let prev = if i - 1 < 0 { n - 1 + i } else { i - 1 };
                let next = if i + 1 < n { i + 1 } else { i + 1 - n };
                scale(
                    &sub(
                        &nodes[next as usize].position,
                        &nodes[prev as usize].position,
                    ),
                    self.tension,
                )
            };
            self.nodes[iu].tangent = [tangent[0], tangent[1], tangent[2], i as f32];
        }
    }

    /// FUN_1405510d0: the segment and fraction nearest to `p`, and the
    /// distance.
    fn nearest(&self, p: &V4) -> (i32, f32, f32) {
        let mut segments = self.count;
        if segments == 2 {
            let (a, b) = self.pair(0);
            let (t, dist) = segment_distance(p, &a.position, &b.position);
            return (0, t, dist);
        }
        if !self.closed {
            segments -= 1;
        }
        let mut best = (0, 0.0, 0.0);
        for i in 0..segments {
            let (a, b) = self.pair(i);
            let (t, dist) = segment_distance(p, &a.position, &b.position);
            if i == 0 || (0.0 <= dist && dist < best.2) {
                best = (i, t, dist);
            }
        }
        let (index, mut t, dist) = best;
        if t > 1.0e-4 && t < 0.9999 {
            // Refine along the curve by halving.
            let (a, b) = self.pair(index);
            let (mut lo_point, mut hi_point) = (a.position, b.position);
            let (mut hi, mut lo) = (1.0f32, 0.0f32);
            let mut mid = 0.0;
            for _ in 0..0x21 {
                mid = (hi + lo) * 0.5;
                let m = hermite(mid, &a, &b);
                let (_, d_lo) = segment_distance(p, &lo_point, &m);
                let (_, d_hi) = segment_distance(p, &m, &hi_point);
                if d_hi <= d_lo {
                    lo_point = m;
                    lo = mid;
                } else {
                    hi_point = m;
                    hi = mid;
                    mid = lo;
                }
            }
            t = (hi + mid) * 0.5;
        }
        (index, t, dist)
    }

    /// FUN_14054f210: refine the fraction in `lo..hi` of segment `a`..`b`.
    fn refine(&self, p: &V4, mut lo: f32, mut hi: f32, a: &Node, b: &Node) -> (f32, f32) {
        let (mut lo_point, mut hi_point) = (a.position, b.position);
        let mut last = 0.0;
        for _ in 0..0x21 {
            let mid = (lo + hi) * 0.5;
            let m = hermite(mid, a, b);
            let (_, d_lo) = segment_distance(p, &lo_point, &m);
            let (_, d_hi) = segment_distance(p, &m, &hi_point);
            if d_hi <= d_lo {
                lo_point = m;
                lo = mid;
                last = d_lo;
            } else {
                hi_point = m;
                hi = mid;
                last = d_hi;
            }
        }
        ((lo + hi) * 0.5, last)
    }

    /// FUN_1405513a0 / FUN_1405515c0: the segment and fraction nearest to `p`
    /// around the current parameter (the walking direction decides which
    /// side is searched first), and the distance.
    fn nearest_around(&self, p: &V4) -> (i32, f32, f32) {
        let whole = self.t as i32;
        let mut index = whole;
        let mut t = self.t - whole as f32;
        let mut frac = t;
        if !self.forward || t <= 0.9999 {
            if !self.forward && t < 1.0e-4 {
                index = if self.closed {
                    (index - 1 + self.count) % self.count
                } else {
                    index - 1
                };
                t = 1.0;
                frac = 1.0;
            }
        } else {
            index = if self.closed {
                (index + 1) % self.count
            } else {
                index + 1
            };
            t = 0.0;
            frac = 0.0;
        }
        if self.closed {
            let (lo, hi) = if self.forward {
                (frac, 1.0)
            } else {
                (0.0, frac)
            };
            let (a, b) = self.pair(index);
            let (t1, d1) = self.refine(p, lo, hi, &a, &b);
            let other = if self.forward {
                (index + 1) % self.count
            } else {
                (index - 1 + self.count) % self.count
            };
            let (a2, b2) = self.pair(other);
            let (t2, d2) = self.refine(p, 0.0, 1.0, &a2, &b2);
            return if d2 <= d1 {
                (other, t2, d2)
            } else {
                (index, t1, d1)
            };
        }
        let last = self.count - 1;
        if index >= last {
            return (last, t, len3(&sub(p, &self.node(last).position)));
        }
        if index < 0 {
            return (0, 0.0, len3(&sub(p, &self.node(0).position)));
        }
        let mut best = (index, frac, -1.0f32);
        let mut at = index;
        let mut lo = frac;
        let mut hi = 1.0f32;
        if !self.forward {
            lo = 0.0;
            hi = frac;
        }
        let mut passes = 0;
        loop {
            let (a, b) = self.pair(at);
            let (tt, dist) = self.refine(p, lo, hi, &a, &b);
            let better = best.2 < 0.0 || dist < best.2;
            if better {
                best = (at, tt, dist);
            }
            at += if self.forward { 1 } else { -1 };
            if at < 0 || last <= at {
                break;
            }
            passes += 1;
            lo = 0.0;
            hi = 1.0;
            if passes >= 2 {
                break;
            }
        }
        best
    }

    /// FUN_140550d80: put the walker at the place on the path nearest to
    /// `p`.
    pub fn reset_to(&mut self, p: &V4) {
        if self.count < 2 {
            return;
        }
        let (index, t, _) = self.nearest(p);
        self.place_on(index, t);
    }

    fn place_on(&mut self, index: i32, t: f32) {
        let (a, b) = self.pair(index);
        self.seg_start = a;
        self.seg_end = b;
        self.t = index as f32 + t;
        self.chord = len3(&sub(&b.position, &a.position));
    }

    /// FUN_14054f720: where the walker is.
    pub fn position(&self) -> V4 {
        let i = self.t as i32;
        let (a, b) = self.pair(i);
        hermite(self.t - i as f32, &a, &b)
    }

    /// FUN_14054f6c0: the direction the path runs in at the walker.
    pub fn direction(&self) -> V4 {
        let i = self.t.floor();
        let (a, b) = self.pair(i as i32);
        let mut d = hermite_slope(self.t - i, &a, &b);
        normalize(&mut d);
        d
    }

    /// FUN_14054f470: the first node's place.
    pub fn first_point(&self) -> V4 {
        self.node(self.start).position
    }

    /// FUN_140550c00: the last node's place.
    pub fn last_point(&self) -> V4 {
        self.node(self.start + self.count - 1).position
    }

    /// FUN_140550240: node `i`'s position and tangent lanes.
    pub fn point(&self, i: i32) -> V4 {
        self.node(i).position
    }

    /// FUN_1405510a0: reverse the walking direction unless it is fixed;
    /// returns the direction.
    pub fn reverse(&mut self) -> bool {
        if !self.fixed {
            self.forward = !self.forward;
        }
        self.forward
    }

    /// FUN_140551850: set the walking direction unless it is fixed.
    pub fn set_forward(&mut self, forward: bool) {
        if !self.fixed && self.forward != forward {
            self.forward = forward;
        }
    }

    /// FUN_140550e90: walk away from `other` along the path (not when fixed
    /// or when the two are nearly together).
    pub fn leave_way(&mut self, own: &V4, other: &V4) {
        if self.fixed {
            return;
        }
        let mut away = sub(own, other);
        if normalize(&mut away) < 0.01 {
            return;
        }
        let (index, t, _) = self.nearest(own);
        let (a, b) = self.pair(index);
        let mut slope = hermite_slope(t, &a, &b);
        if normalize(&mut slope) < 0.01 {
            return;
        }
        let along = dot3(&away, &slope);
        let against = dot3(&away, &scale(&slope, -1.0));
        self.forward = against <= along;
    }

    /// FUN_14054f4b0: move the parameter along the curve until the walked
    /// distance is `length` (in steps of 5e-5 of a segment, at most 1024),
    /// stopping at the nearer side of the step that passes it.
    pub fn advance_by_length(&mut self, length: f32) {
        let curve = |route: &Self, t: f32| {
            let whole = t as i32;
            let (a, b) = route.pair(whole);
            hermite(t - whole as f32, &a, &b)
        };
        let mut previous = curve(self, self.t);
        let mut before = self.t;
        let mut walked = 0.0f32;
        let mut steps = 0;
        loop {
            if length <= walked {
                return;
            }
            let here = curve(self, self.t);
            let reached = walked + len3(&sub(&here, &previous));
            if length <= reached {
                if (walked - length).abs() < (reached - length).abs() {
                    self.t = before;
                }
                return;
            }
            previous = here;
            before = self.t;
            self.t += 5.0e-5;
            let wrap = (self.start + self.count) as f32;
            if wrap <= self.t {
                self.t -= wrap;
            }
            steps += 1;
            walked = reached;
            if steps > 0x3ff {
                return;
            }
        }
    }

    /// FUN_14054e7d0: append a position, dropping the oldest when full.
    fn push(&mut self, p: &V4) -> usize {
        let (count, start) = (self.count, self.start);
        if count < self.capacity {
            self.count += 1;
        } else {
            self.start += 1;
        }
        let at = ((start + count) % self.capacity) as usize;
        self.nodes[at].position = *p;
        at
    }

    /// FUN_14054e6a0: an active path takes a new point once it is 100
    /// units from the last one, and the last node's tangent runs to it.
    pub fn follow_point(&mut self, p: &V4) {
        if self.count == 0 {
            self.push(p);
            return;
        }
        let last = ((self.start + self.count - 1) % self.capacity) as usize;
        if len3(&sub(&self.nodes[last].position, p)) >= 100.0 {
            let at = self.push(p);
            let tangent = sub(&self.nodes[at].position, &self.nodes[last].position);
            self.nodes[last].tangent = tangent;
            self.nodes[last].tangent[3] = (self.count - 2 + self.start) as f32;
        }
    }

    /// FUN_140550c30 / FUN_14054e3f0: aim an active path at `target` (or
    /// keep the old one for `None`), starting it over; when the target is
    /// more than twice the radius from `object` the path begins with the
    /// object, a point 110 short of the target, and the target.
    pub fn set_target(&mut self, target: Option<u32>, object: &V4, target_position: &V4) {
        if !self.active {
            return;
        }
        let new = match (target, self.target) {
            (Some(t), old) if old != Some(t) => t,
            (None, Some(old)) => old,
            _ => return,
        };
        self.target = Some(new);
        self.start = 0;
        self.count = 0;
        self.t = 0.0;
        self.progress = -2.0;
        let apart = sub(target_position, object);
        let dist = len3(&apart);
        if self.radius + self.radius < dist {
            let near = add(&scale(&apart, (dist - 110.0) / dist), object);
            self.follow_point(object);
            self.follow_point(&near);
            self.follow_point(target_position);
        }
    }

    /// FUN_14054f3a0: forget an active path's points.
    pub fn clear(&mut self) {
        if self.active {
            self.progress = -2.0;
            self.target = None;
            self.start = 0;
            self.count = 0;
            self.t = 0.0;
        }
    }

    /// FUN_14054f7c0: the direction an object at `position` heading `facing`
    /// should take to follow the path, looking `ahead` units on (at most
    /// 50); `blend` weights the direction along the path against the one
    /// straight to the point ahead.
    pub fn follow_direction(&mut self, position: &V4, facing: &V4, ahead: f32, blend: f32) -> V4 {
        let ahead = ahead.min(50.0);
        let mut reference = *position;
        let Some(target) = self.look_ahead(&mut reference, ahead) else {
            return [facing[0], facing[1], facing[2], 0.0];
        };
        let mut to_target = sub(&target, position);
        let mut from_path = sub(&target, &reference);
        let distance = normalize(&mut to_target);
        normalize(&mut from_path);
        let mut direction = to_target;
        if distance <= ahead * 10.0 {
            let mixed = add(&scale(&to_target, 1.0 - blend), &scale(&from_path, blend));
            direction = mixed;
            normalize(&mut direction);
        }
        [direction[0], direction[1], direction[2], 1.0]
    }

    /// FUN_14054f9d0: the point `ahead` units further along the path from
    /// the place nearest `position`, which is replaced by that nearest place.
    /// `None` when the end of a path that does not turn is reached.
    pub fn look_ahead(&mut self, position: &mut V4, ahead: f32) -> Option<V4> {
        if !self.active {
            let whole = self.t as i32;
            let (a, b) = self.pair(whole);
            let current = hermite(self.t - whole as f32, &a, &b);
            let old = *position;
            let search = ahead * 10.0;
            let (index, t, dist) = if self.count < 3 {
                self.nearest(position)
            } else {
                let found = self.nearest_around(position);
                if found.2 < 0.0 || search <= found.2 {
                    self.nearest(position)
                } else {
                    found
                }
            };
            let old_t = self.t;
            self.place_on(index, t);
            let _ = dist;
            let projected = hermite(t, &self.seg_start, &self.seg_end);
            *position = projected;
            let d_old = len3(&sub(&current, &old));
            let d_new = len3(&sub(&projected, &old));
            if (d_new - d_old).abs() < (d_new + d_old) * 1.0e-4 {
                self.t = old_t;
                *position = current;
            }
            self.progress = self.t;
            if self.stop_index >= 0 && !self.fixed {
                self.forward = (self.t as i32) < self.stop_index;
            }
        } else if self.count < 2 || ((self.count - 1 + self.start) as f32) - 1.0 < self.progress {
            return None;
        }
        if self.progress < -1.0 {
            let p = *position;
            self.reset_to(&p);
            self.progress = self.t;
        }
        if self.active && self.start <= (self.progress + 1.0) as i32 {
            let node = self.node((self.progress + 1.0) as i32);
            self.seg_end = node;
            self.chord = len3(&sub(&self.seg_end.position, &self.seg_start.position));
        }
        self.last_position = *position;
        let mut pass = 0;
        let mut restart = true;
        while restart {
            restart = false;
            let mut found = None;
            for _ in 0..0x14 {
                let u = self.progress - self.seg_start.tangent[3];
                let candidate = hermite(u, &self.seg_start, &self.seg_end);
                let mut off = sub(&candidate, position);
                off[1] = 0.0;
                if ahead < len3(&off) {
                    found = Some(candidate);
                    break;
                }
                let index = self.seg_start.tangent[3];
                let step = (ahead / self.chord) * 0.5;
                if !self.forward {
                    self.progress -= step;
                    if self.progress <= index {
                        let first = self.start;
                        let mut previous = (index + 0.5) as i32 - 1;
                        if previous < first {
                            previous += self.count;
                        }
                        if !self.active {
                            if (first as f32) < self.progress {
                                self.progress = index;
                            } else if !self.closed {
                                if !self.bounce {
                                    return None;
                                }
                                if !self.fixed {
                                    self.forward = true;
                                }
                                restart = true;
                                break;
                            } else {
                                self.progress = (first + self.count) as f32;
                            }
                        } else if self.progress < first as f32 {
                            return None;
                        }
                        self.seg_end = self.seg_start;
                        self.seg_start = self.node(previous);
                        self.chord = len3(&sub(&self.seg_end.position, &self.seg_start.position));
                    }
                } else {
                    self.progress += step;
                    if index + 1.0 <= self.progress {
                        let last = self.start - 1 + self.count;
                        let mut next = (self.progress + 1.0) as i32;
                        if !self.active {
                            if last < next {
                                if !self.closed {
                                    if !self.bounce {
                                        return None;
                                    }
                                    if !self.fixed {
                                        self.forward = !self.forward;
                                    }
                                    restart = true;
                                    break;
                                }
                                next -= self.count;
                            }
                        } else {
                            if last <= next {
                                self.progress = self.seg_end.tangent[3];
                                self.seg_start = self.seg_end;
                                return None;
                            }
                            if next < self.start {
                                let p = *position;
                                self.reset_to(&p);
                                self.progress = self.t;
                                restart = true;
                                break;
                            }
                        }
                        self.progress = self.seg_end.tangent[3];
                        self.seg_start = self.seg_end;
                        self.seg_end = self.node(next);
                        self.chord = len3(&sub(&self.seg_end.position, &self.seg_start.position));
                    }
                }
            }
            if let Some(found) = found {
                return Some(found);
            }
            if !restart {
                break;
            }
            pass += 1;
            if pass > 1 {
                break;
            }
        }
        Some(*position)
    }
}

#[cfg(test)]
mod tests;
