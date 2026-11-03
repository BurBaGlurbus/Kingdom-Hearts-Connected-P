//! worldmap::Player's actions and commands, translated from the exe: the
//! action table 0x140750950 (FUN_140409d20) gives ActionShip (0x2f,
//! vtable 0x1405cd6c8), ActionWarp (0x3a, 0x1405cd720) and ActionDash
//! (0x3b, 0x1405cd778); the command deck kind 10 holds one row
//! (FUN_1403947a0); the manual lock-on (FUN_1403dcd70 state 2 with the
//! search FUN_1403bea30 and the stick switch FUN_1403bd750); the engine
//! effects by floor (FUN_140394900). Engine coordinates (Y down), 60 Hz
//! ticks.
type V3 = [f32; 3];

/// Action ids (FUN_1403b4da0).
pub const SHIP: u16 = 0x2f;
pub const WARP: u16 = 0x3a;
pub const DASH: u16 = 0x3b;

/// The cursor's motions: ActionShip's idle and fast (FUN_140563880),
/// ActionDash's (FUN_140563a70) and ActionWarp's (FUN_140563950).
pub const MOTION_IDLE: u16 = 0;
pub const MOTION_FAST: u16 = 0x99;
pub const MOTION_DASH: u16 = 0x9a;
pub const MOTION_WARP: u16 = 0xa1;
/// FUN_1403c6dc0's blend for ActionShip's motions and ActionDash's exit
/// (DAT_140623bac).
pub const BLEND: f32 = 8.;
/// ActionDash's steering (FUN_1405639b0): inertia and drag 0.95
/// (DAT_1405b1c74).
pub const DASH_STEER: f32 = 0.95;

/// The commands the cursor's deck (kind 10) offers: 0x246 warps to the
/// locked target, 0x247 dashes (FUN_140394fc0). Their 03system 'cmd' texts.
pub const CMD_WARP: u16 = 0x246;
pub const CMD_DASH: u16 = 0x247;
pub const MSG_WARP: u32 = 0x46a4;
pub const MSG_DASH: u32 = 0x46a3;
/// The row's selector (record +6 = 1): the back button, logical bit 24.
pub const SELECTOR: u8 = 1;
/// FUN_14015df90 for deck kind 10: selector 1 draws icon 0xdb for messages
/// 0x46a3, 0x46a4 and 0x4925, else 0xda.
pub fn row_icon(message: u32) -> u8 {
    if (0x46a3..=0x46a4).contains(&message) || message == 0x4925 {
        0xdb
    } else {
        0xda
    }
}

/// FUN_1403947a0: each tick the deck is rebuilt with the warp command while
/// a target is held, else the dash.
pub fn command(locked: bool) -> (u16, u32) {
    if locked {
        (CMD_WARP, MSG_WARP)
    } else {
        (CMD_DASH, MSG_DASH)
    }
}

/// FUN_140563880: the fast motion once the requested speed reaches the mean
/// of the walk and run speeds.
pub fn ship_motion(speed: f32, walk: f32, run: f32) -> u16 {
    if 0. < speed && (walk + run) * 0.5 <= speed {
        MOTION_FAST
    } else {
        MOTION_IDLE
    }
}

/// FUN_1403b84e0 → FUN_140394900: which engine effect the floor under the
/// cursor wants: element 5 (slot +0xe68) on material 1, element 6 (slot
/// +0xe70) on material 2, none elsewhere. The others are released
/// (FUN_1403f1f00).
pub fn engine_effect(material: u8) -> Option<(usize, u16)> {
    match material {
        1 => Some((0, 5)),
        2 => Some((1, 6)),
        _ => None,
    }
}
/// ActionDash's element (FUN_140395950, slot +0xe60).
pub const DASH_EFFECT: u16 = 2;

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// FUN_140141ae0(out, a, b, t) = `b * (1 − t) + a * t`.
fn mix(a: V3, b: V3, t: f32) -> V3 {
    [0, 1, 2].map(|i| b[i] * (1. - t) + a[i] * t)
}

fn length(v: V3) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// ActionWarp's path (FUN_1403978c0, run by FUN_140397820 through
/// FUN_1403b6d70): a cubic Bézier from the cursor to the ground under the
/// top of the target's sphere over 30 ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpPath {
    points: [V3; 4],
    /// +0x50 / +0x54.
    time: f32,
    length: f32,
}

/// FUN_1403978c0's ground search (FUN_1403f3dd0(p, p, 5000, 1000)): a ray
/// from 1000 above the point to 6000 below it.
pub const GROUND_ABOVE: f32 = 1000.;
pub const GROUND_BELOW: f32 = 5000.;

impl WarpPath {
    /// `from` the cursor; `centre` and `radius` the target sphere's (the
    /// holder's FUN_1403be0f0); `ground(top, bottom)` the map's first hit on
    /// the segment, if any.
    pub fn new(from: V3, centre: V3, radius: f32, ground: impl Fn(V3, V3) -> Option<V3>) -> Self {
        let mut end = centre;
        end[1] -= radius;
        let top = [end[0], end[1] - GROUND_ABOVE, end[2]];
        let bottom = [end[0], end[1] + GROUND_BELOW + GROUND_ABOVE, end[2]];
        if let Some(hit) = ground(top, bottom) {
            end = hit;
        }
        // DAT_1406239f8 0.1 and DAT_1405aaa40 0.9, raised by 1000
        // (DAT_140623c08) and 500 (DAT_1405b4fe8).
        let mut p1 = mix(end, from, 0.1);
        p1[1] -= 1000.;
        let mut p2 = mix(end, from, 0.9);
        p2[1] -= 500.;
        WarpPath {
            points: [from, p1, p2, end],
            time: 0.,
            length: 30.,
        }
    }

    /// FUN_140397820: one tick; the new position. At the end the position
    /// is the last point and the path is spent.
    pub fn step(&mut self) -> V3 {
        self.time += 1.;
        if self.time < self.length {
            bezier(&self.points, self.time / self.length)
        } else {
            self.length = 0.;
            self.points[3]
        }
    }

    /// ActionWarp's tick (FUN_1405638f0) leaves for ActionShip once +0x54
    /// is 0.
    pub fn done(&self) -> bool {
        self.length <= 0.
    }

    pub fn end(&self) -> V3 {
        self.points[3]
    }
}

/// FUN_1403b6d70: (1−t)³ p0 + 3(1−t)² t p1 + 3(1−t) t² p2 + t³ p3.
fn bezier(p: &[V3; 4], t: f32) -> V3 {
    let u = 1. - t;
    [0, 1, 2].map(|i| {
        p[1][i] * 3. * u.powf(2.) * t
            + u.powf(3.) * p[0][i]
            + p[2][i] * 3. * u * t.powf(2.)
            + t.powf(3.) * p[3][i]
    })
}

/// A lock-on candidate: an object's TARGET shape (kind 3) as FUN_1403c9c00
/// places it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub object: usize,
    pub centre: V3,
    pub radius: f32,
}

/// worldmap::Player's look point (vtable +0x140, FUN_140394d10): its
/// position 3500 further along +Z (DAT_1405c20b8).
pub fn look_point(position: V3) -> V3 {
    [position[0], position[1], position[2] + 3500.]
}

/// FUN_1403bea30 (FUN_1403be2b0 mode 4, the manual lock's search): of the
/// candidates whose sphere comes within `range` (SSTM +0x4c times
/// FUN_1403aba10's scale) of the look point, the one nearest the cursor.
pub fn lock_search(position: V3, range: f32, candidates: &[Candidate]) -> Option<Candidate> {
    let look = look_point(position);
    let mut best: Option<(f32, Candidate)> = None;
    for c in candidates {
        if range < length(sub(c.centre, look)) - c.radius {
            continue;
        }
        let d = length(sub(c.centre, position)) - c.radius;
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, *c));
        }
    }
    best.map(|(_, c)| c)
}

/// FUN_1403bd750: a right-stick flick while locked moves the lock to the
/// nearest other candidate within range of the look point that lies in the
/// stick's direction from the current target (cosine at least 0.5, on the
/// ground plane with Z flipped to the screen's up). `stick` is (x right,
/// y down).
pub fn switch_target(
    position: V3,
    range: f32,
    current: &Candidate,
    stick: [f32; 2],
    candidates: &[Candidate],
) -> Option<Candidate> {
    let look = look_point(position);
    let s = stick[0].hypot(stick[1]);
    if s == 0. {
        return None;
    }
    let stick = [stick[0] / s, stick[1] / s];
    let mut best: Option<(f32, Candidate)> = None;
    for c in candidates {
        if c.object == current.object {
            continue;
        }
        if range < length(sub(c.centre, look)) - c.radius {
            continue;
        }
        let v = [
            c.centre[0] - current.centre[0],
            -(c.centre[2] - current.centre[2]),
        ];
        let d = v[0].hypot(v[1]);
        let score = d - c.radius;
        let cos = if d > 0. {
            (v[0] * stick[0] + v[1] * stick[1]) / d
        } else {
            0.
        };
        if 0.5 <= cos && best.is_none_or(|(b, _)| score < b) {
            best = Some((score, *c));
        }
    }
    best.map(|(_, c)| c)
}

/// worldmap::LockOnCamera (FUN_1405635d0, update FUN_1405636c0): it frames a
/// point 15% of the way from the cursor to the target, its eye offset
/// moving from near to far as the two part from 1000 to 6000 units.
pub mod lock_camera {
    /// DAT_14078a6a0 / 6b0 / 690.
    pub const NEAR_EYE: super::V3 = [0., -1840., -2400.];
    pub const FAR_EYE: super::V3 = [0., -3450., -3000.];
    pub const TARGET: super::V3 = [0., -500., 0.];
    /// DAT_1405b5328.
    pub const SHARE: f32 = 0.15;
    /// DAT_140623c08, DAT_1405c2b88 and DAT_1405b26e4.
    pub const NEAR: f32 = 1000.;
    pub const FAR: f32 = 6000.;
    /// +0x54 = 30.0: the blend from the previous camera.
    pub const BLEND_TICKS: f32 = 30.;

    /// The eye and target this tick (before FUN_14038ebc0's blend).
    pub fn goal(cursor: super::V3, target: super::V3) -> (super::V3, super::V3) {
        let mid = super::mix(target, cursor, SHARE);
        let d = super::length(super::sub(target, cursor));
        let t = if d <= NEAR {
            0.
        } else if d < FAR {
            (d - NEAR) / (FAR - NEAR)
        } else {
            1.
        };
        let eye = super::mix(FAR_EYE, NEAR_EYE, t);
        (super::add(mid, eye), super::add(mid, TARGET))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(object: usize, centre: V3, radius: f32) -> Candidate {
        Candidate {
            object,
            centre,
            radius,
        }
    }

    #[test]
    fn the_deck_row_follows_the_lock() {
        assert_eq!(command(false), (CMD_DASH, MSG_DASH));
        assert_eq!(command(true), (CMD_WARP, MSG_WARP));
        assert_eq!(row_icon(MSG_DASH), 0xdb);
        assert_eq!(row_icon(0x4926), 0xda);
    }

    #[test]
    fn the_ship_runs_fast_above_the_mean_speed() {
        assert_eq!(ship_motion(30., 15., 30.), MOTION_FAST);
        assert_eq!(ship_motion(15., 15., 30.), MOTION_IDLE);
        assert_eq!(ship_motion(0., 0., 0.), MOTION_IDLE);
    }

    #[test]
    fn engine_effects_follow_the_floor_material() {
        assert_eq!(engine_effect(1), Some((0, 5)));
        assert_eq!(engine_effect(2), Some((1, 6)));
        assert_eq!(engine_effect(0), None);
        assert_eq!(engine_effect(0x12), None);
    }

    #[test]
    fn the_warp_lands_on_the_ground_under_the_target() {
        let ground = |top: V3, bottom: V3| {
            assert!(top[1] < bottom[1]);
            Some([top[0], 0., top[2]])
        };
        let mut p = WarpPath::new([0., 0., 0.], [3000., -800., 0.], 500., ground);
        assert_eq!(p.end(), [3000., 0., 0.]);
        let first = p.step();
        // The first control point lifts the path off the floor.
        assert!(first[1] < 0. && first[0] > 0. && first[0] < 300.);
        let mut last = first;
        while !p.done() {
            last = p.step();
        }
        assert_eq!(last, [3000., 0., 0.]);
        // 30 ticks in all.
        let mut q = WarpPath::new([0.; 3], [10., 0., 0.], 0., |_, _| None);
        let n = std::iter::from_fn(|| (!q.done()).then(|| q.step())).count();
        assert_eq!(n, 30);
        assert_eq!(q.end(), [10., 0., 0.]);
    }

    #[test]
    fn the_manual_lock_takes_the_nearest_in_range_of_the_look_point() {
        let cands = [
            candidate(1, [0., 0., 4000.], 500.),
            candidate(2, [0., 0., 9000.], 500.),
            candidate(3, [0., 0., -1000.], 500.),
        ];
        // Look point (0, 0, 3500): range 1000 reaches 1 and 3 is 4500 off.
        let got = lock_search([0.; 3], 1000., &cands).unwrap();
        assert_eq!(got.object, 1);
        assert_eq!(lock_search([0.; 3], 10., &cands[1..]), None);
        // Of two in range, the nearer to the cursor.
        let got = lock_search([0.; 3], 10_000., &cands).unwrap();
        assert_eq!(got.object, 3);
    }

    #[test]
    fn a_stick_flick_switches_to_the_target_that_way() {
        let cur = candidate(1, [0., 0., 0.], 100.);
        let cands = [
            cur,
            candidate(2, [2000., 0., 0.], 100.),
            candidate(3, [0., 0., 2000.], 100.),
        ];
        let right = switch_target([0., 0., -3500.], 1e6, &cur, [1., 0.], &cands);
        assert_eq!(right.map(|c| c.object), Some(2));
        // Screen up (y −1) is +Z on the map.
        let up = switch_target([0., 0., -3500.], 1e6, &cur, [0., -1.], &cands);
        assert_eq!(up.map(|c| c.object), Some(3));
        let left = switch_target([0., 0., -3500.], 1e6, &cur, [-1., 0.], &cands);
        assert_eq!(left, None);
    }

    #[test]
    fn the_lock_camera_pulls_back_with_distance() {
        let (eye, target) = lock_camera::goal([0.; 3], [500., 0., 0.]);
        assert_eq!(target, [75., -500., 0.]);
        assert_eq!(eye, [75., -1840., -2400.]);
        let (far, _) = lock_camera::goal([0.; 3], [10000., 0., 0.]);
        assert_eq!(far, [1500., -3450., -3000.]);
        let (mid, _) = lock_camera::goal([0.; 3], [3500., 0., 0.]);
        assert!((mid[1] - (-1840. * 0.5 - 3450. * 0.5)).abs() < 1e-2);
    }
}
