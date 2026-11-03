//! The ring search against a synthetic map: a floor at y = 0, and optionally a
//! wall that blocks the lines through it.
use super::*;
use crate::env::{Clips, NullEnv};
use crate::sysobj::Vec3;

struct Map {
    wall_x: Option<f32>,
    ground: bool,
}

impl Env for Map {
    fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        NullEnv.floor(x, z, top)
    }
    fn clips(&self, model: u32) -> &dyn Clips {
        NullEnv.clips(model)
    }
    fn player_position(&self) -> Vec3 {
        [0.; 3]
    }
    fn target_position(&self, _: u32) -> Option<Vec3> {
        None
    }
    fn line_hit(&self, from: Vec3, to: Vec3) -> Option<Vec3> {
        if let Some(w) = self.wall_x
            && (from[0] - w) * (to[0] - w) < 0.0
        {
            return Some([w, from[1], from[2]]);
        }
        let vertical = from[0] == to[0] && from[2] == to[2];
        (self.ground && vertical && from[1] >= 0.0 && to[1] <= 0.0)
            .then_some([from[0], 0.0, from[2]])
    }
}

fn flat() -> Map {
    Map {
        wall_x: None,
        ground: true,
    }
}

fn dist(a: &Vec4, b: &Vec4) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn a_ground_point_lies_on_the_floor_within_the_ring() {
    let map = flat();
    let mut rng = Lcg::default();
    let mut counter = 0;
    let center = [500.0, 30.0, -200.0, 1.0];
    for _ in 0..20 {
        let p = ground_point(&map, &mut rng, &mut counter, center, 100.0, 400.0, 1);
        assert_eq!(p[1], 0.0);
        let d = dist(&p, &center);
        assert!((100.0 - 0.01..=400.0 + 0.01).contains(&d), "{d}");
    }
    // The lap counter went round once per call.
    assert_eq!(counter, 20 & 7);
}

#[test]
fn the_ring_start_follows_the_documented_expression() {
    let mut rng = Lcg::default();
    let mut counter = 5;
    let ring = Ring::new(&mut rng, &mut counter, [0.; 4], 10.0, 20.0);
    assert_eq!(counter, 6);
    let first = 0x10dcd_u32;
    let second = first.wrapping_mul(0x10dcd);
    assert_eq!(ring.radius, first as f32 * UNIT * 10.0 + 10.0);
    // Counter 5 = 0b101: (5 >> 2 & 1) | (5 << 1) = 1 | 10 = 11.
    assert_eq!(
        ring.start,
        11.0 * ANGLE_STEP + second as f32 * UNIT * ANGLE_STEP
    );
    // Two steps of the generator equal the executable's merged multiplier.
    assert_eq!(rng.0, 1u32.wrapping_mul(0x1c58_7629));
}

#[test]
fn without_ground_the_centre_comes_back() {
    let map = Map {
        wall_x: None,
        ground: false,
    };
    let mut rng = Lcg::default();
    let mut counter = 0;
    let center = [1.0, 2.0, 3.0, 1.0];
    assert_eq!(
        ground_point(&map, &mut rng, &mut counter, center, 50.0, 60.0, 1),
        center
    );
}

#[test]
fn an_air_point_is_raised_and_a_blocked_line_rejects_the_ring() {
    let mut rng = Lcg::default();
    let mut counter = 0;
    let center = [0.0, 0.0, 0.0, 1.0];
    let p = air_point(
        &flat(),
        &mut rng,
        &mut counter,
        center,
        100.0,
        200.0,
        50.0,
        90.0,
        2,
    );
    assert!((50.0..=90.0).contains(&p[1]), "{}", p[1]);
    // A wall just beside the centre blocks every line that crosses it, so a
    // point on its far side is refused: whatever comes back is on the near
    // side (or is the centre).
    let walled = Map {
        wall_x: Some(1.0),
        ground: true,
    };
    let mut rng = Lcg::default();
    let mut counter = 0;
    for _ in 0..10 {
        let p = air_point(
            &walled,
            &mut rng,
            &mut counter,
            center,
            100.0,
            200.0,
            50.0,
            90.0,
            2,
        );
        assert!(p[0] < 1.0);
    }
}
