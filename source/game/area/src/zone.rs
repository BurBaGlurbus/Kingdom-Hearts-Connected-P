//! Spawn-point zones (YS::ZONE_BOX / ZONE_CYLINDER / ZONE_INFINITY), built
//! from event-activator records by FUN_140421010 / FUN_140420de0 and tested
//! by vtbl slot 1 (0x140421180 / 0x140421250 / 0x140421320).
use data::ard::Activator;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Box,
    Cylinder,
    Infinity,
}

/// A zone in engine coordinates (Y down).
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub shape: Shape,
    pub option: u16,
    pub center: [f32; 3],
    pub half: [f32; 3],
    /// Rotation Y; X and Z are ignored by the ctor.
    pub rot_y: f32,
    /// World → local rows (row-vector convention), as the ctor's inverted
    /// RotY(−rotY)·T matrix.
    local: [[f32; 3]; 4],
    pub activator: usize,
}

impl Zone {
    /// FUN_140421010: shapes other than 0..2 make no zone.
    pub fn new(a: &Activator, activator: usize) -> Option<Self> {
        let shape = match a.shape {
            0 => Shape::Box,
            1 => Shape::Cylinder,
            2 => Shape::Infinity,
            _ => return None,
        };
        // M = RotY(−r) with row 3 = centre; FUN_140141270 rows
        // (c, 0, −s), (0, 1, 0), (s, 0, c) for angle −r. The inverse
        // (FUN_14013f3a0) transposes the rotation and sets t' = −t·Rᵀ.
        let r = -a.rotation[1];
        let (s, c) = r.sin_cos();
        let rot = [[c, 0., -s], [0., 1., 0.], [s, 0., c]];
        // (t·Rᵀ)_j = row j of R · t.
        let t = |j: usize| rot[j].iter().zip(a.center).map(|(r, p)| r * p).sum::<f32>();
        // Transposed rotation rows: row i of Rᵀ = column i of R.
        let rt = [
            [rot[0][0], rot[1][0], rot[2][0]],
            [rot[0][1], rot[1][1], rot[2][1]],
            [rot[0][2], rot[1][2], rot[2][2]],
        ];
        let translation = [-t(0), -t(1), -t(2)];
        Some(Self {
            shape,
            option: a.option,
            center: a.center,
            half: a.extent,
            rot_y: a.rotation[1],
            local: [rt[0], rt[1], rt[2], translation],
            activator,
        })
    }

    /// The point in zone-local space (`v·M` with w = 1).
    pub fn to_local(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.local;
        std::array::from_fn(|j| p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j])
    }

    /// vtbl slot 1. Comparisons are inclusive and fail on NaN.
    pub fn contains(&self, p: [f32; 3]) -> bool {
        let [lx, ly, lz] = self.to_local(p);
        let [hx, hy, hz] = self.half;
        match self.shape {
            Shape::Box => -hx <= lx && lx <= hx && -hy <= ly && ly <= hy && -hz <= lz && lz <= hz,
            Shape::Cylinder => {
                let (z, x) = (lz / hz, lx / hx);
                -hy <= ly && ly <= hy && z * z + x * x <= 1.0
            }
            Shape::Infinity => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activator(shape: u16, center: [f32; 3], extent: [f32; 3], rot_y: f32) -> Activator {
        Activator {
            shape,
            option: 0,
            center,
            extent,
            rotation: [0., rot_y, 0.],
            flags: 0,
            group_on: 0,
            group_off: 0,
            raw: [0; 0x40],
        }
    }

    #[test]
    fn local_space_matches_the_closed_form() {
        // exit-01 §1.2: lx = c·dx + s·dz, ly = dy, lz = −s·dx + c·dz.
        let r = 0.3491f32;
        let z = Zone::new(
            &activator(0, [-2520., -175., 130.], [100., 275., 400.], r),
            0,
        )
        .unwrap();
        let p = [-2400., -100., 300.];
        let (dx, dy, dz) = (p[0] + 2520., p[1] + 175., p[2] - 130.);
        let (s, c) = r.sin_cos();
        let l = z.to_local(p);
        let want = [c * dx + s * dz, dy, -s * dx + c * dz];
        for i in 0..3 {
            assert!((l[i] - want[i]).abs() < 1e-3, "{l:?} {want:?}");
        }
    }

    #[test]
    fn box_and_cylinder_bounds_are_inclusive() {
        let b = Zone::new(&activator(0, [0.; 3], [100., 50., 10.], 0.), 0).unwrap();
        assert!(b.contains([100., -50., 10.]));
        assert!(!b.contains([100.5, 0., 0.]));
        assert!(!b.contains([f32::NAN, 0., 0.]));
        let c = Zone::new(&activator(1, [0.; 3], [60., 100., 60.], 0.), 0).unwrap();
        assert!(c.contains([60., 0., 0.]));
        assert!(!c.contains([43., 0., 43.]));
        assert!(!c.contains([0., 101., 0.]));
        let i = Zone::new(&activator(2, [0.; 3], [0.; 3], 0.), 0).unwrap();
        assert!(i.contains([1e9, 0., 0.]));
        assert!(Zone::new(&activator(3, [0.; 3], [1.; 3], 0.), 0).is_none());
    }
}
