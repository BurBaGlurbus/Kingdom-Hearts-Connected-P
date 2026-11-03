//! Hand-computed values.
use super::*;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

fn close(a: [f32; 4], b: [f32; 4]) {
    for k in 0..4 {
        assert!((a[k] - b[k]).abs() < 1e-5, "{a:?} vs {b:?}");
    }
}

#[test]
fn homing_turns_by_the_step_toward_the_target() {
    let r = homing([2., 0., 0., 1.], [0., 5., 0., 1.], FRAC_PI_4);
    let h = FRAC_PI_4.cos();
    close(r, [h, h, 0., 0.]);
}

#[test]
fn homing_stops_on_the_target_when_the_step_is_larger() {
    let r = homing([1., 0., 0., 1.], [0., 0., 3., 1.], 3.0);
    close(r, [0., 0., 1., 0.]);
}

#[test]
fn homing_a_parallel_vector_returns_the_unit_vector() {
    let r = homing([0., 4., 0., 1.], [0., 1., 0., 1.], 1.0);
    close(r, [0., 1., 0., 0.]);
}

#[test]
fn homing_keeps_a_zero_vector_zero() {
    let r = homing([0.; 4], [0., 1., 0., 1.], 1.0);
    close(r, [0.; 4]);
}

#[test]
fn rotx_turns_y_toward_z() {
    let m = axis_angle([1., 0., 0., 1.], FRAC_PI_2);
    close(apply(&m, [0., 1., 0., 0.]), [0., 0., 1., 0.]);
    close(apply(&m, [1., 0., 0., 0.]), [1., 0., 0., 0.]);
}

#[test]
fn angles_wrap_into_a_half_turn_either_side() {
    use std::f32::consts::PI;
    assert!((wrap_pi(3. * PI / 2.) + PI / 2.).abs() < 1e-5);
    assert!((wrap_pi(-3. * PI / 2.) - PI / 2.).abs() < 1e-5);
    assert!((wrap_pi(0.5) - 0.5).abs() < 1e-6);
}
