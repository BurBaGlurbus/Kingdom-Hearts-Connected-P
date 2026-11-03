//! The route follower on synthetic paths.
use super::*;

fn line(points: &[[f32; 3]], flags: u8) -> Route {
    let path = WalkPath {
        flags,
        points: points.to_vec(),
        ..WalkPath::default()
    };
    let build = Build {
        routine: true,
        start_at_object: true,
        ..Build::default()
    };
    Route::from_path(&path, build, &[0.0, 0.0, 0.0, 1.0], &mut || 0).0
}

fn near(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn a_straight_path_has_straight_tangents_and_a_midpoint() {
    let route = line(&[[0., 0., 0.], [100., 0., 0.], [200., 0., 0.]], 0);
    for k in 0..3 {
        let t = route.node(k).tangent;
        assert!(t[0] > 0.0 && t[1] == 0.0 && t[2] == 0.0);
        assert_eq!(t[3], k as f32);
    }
    let (a, b) = route.pair(0);
    let mid = hermite(0.5, &a, &b);
    assert!(near(mid[0], 50.0, 1.0e-3) && near(mid[1], 0.0, 1.0e-4) && mid[3] == 1.0);
    // The ends of the segment are the nodes.
    assert_eq!(hermite(0.0, &a, &b)[..3], a.position[..3]);
    let end = hermite(1.0, &a, &b);
    assert!(near(end[0], 100.0, 1.0e-3));
}

#[test]
fn the_nearest_point_is_found_on_the_right_segment() {
    let route = line(&[[0., 0., 0.], [100., 0., 0.], [200., 0., 0.]], 0);
    let (index, t, dist) = route.nearest(&[160.0, 0.0, 30.0, 1.0]);
    assert_eq!(index, 1);
    assert!(near(t, 0.6, 0.01), "{t}");
    assert!(near(dist, 30.0, 0.1), "{dist}");
    let (index, t, _) = route.nearest(&[-50.0, 0.0, 0.0, 1.0]);
    assert_eq!((index, t), (0, 0.0));
}

#[test]
fn looking_ahead_walks_forward_and_turns_round_at_the_end() {
    let mut route = line(&[[0., 0., 0.], [100., 0., 0.], [200., 0., 0.]], 0);
    let mut here = [10.0, 0.0, 0.0, 1.0];
    let target = route.look_ahead(&mut here, 30.0).expect("a point ahead");
    assert!(target[0] > 38.0 && target[0] < 60.0, "{target:?}");
    assert!(near(here[0], 10.0, 0.5));
    // From the end of the path the walker turns round (the path is not
    // fixed) and the point ahead lies back along it.
    let mut end = [199.0, 0.0, 0.0, 1.0];
    for _ in 0..4 {
        route.look_ahead(&mut end, 30.0);
    }
    let back = route.look_ahead(&mut end, 30.0).expect("turned round");
    assert!(back[0] < end[0] - 20.0, "{back:?} from {end:?}");
    assert!(!route.forward);
}

#[test]
fn a_fixed_direction_does_not_turn_and_ends_the_walk() {
    let path = WalkPath {
        points: vec![[0., 0., 0.], [100., 0., 0.], [200., 0., 0.]],
        ..WalkPath::default()
    };
    let build = Build {
        routine: true,
        start_at_object: true,
        one_way: true,
        ..Build::default()
    };
    let (mut route, _) = Route::from_path(&path, build, &[0.0; 4], &mut || 0);
    assert!(route.fixed && !route.bounce);
    assert!(route.forward);
    assert!(route.reverse());
    assert!(route.forward, "a fixed direction stays");
    let mut end = [199.5, 0.0, 0.0, 1.0];
    let mut result = Some([0.0; 4]);
    for _ in 0..4 {
        result = route.look_ahead(&mut end, 30.0);
    }
    assert!(result.is_none());
}

#[test]
fn a_closed_path_wraps_round() {
    let mut route = line(
        &[
            [0., 0., 0.],
            [100., 0., 0.],
            [100., 0., 100.],
            [0., 0., 100.],
        ],
        1,
    );
    assert!(route.closed && route.bounce);
    let mut here = [0.0, 0.0, 90.0, 1.0];
    let mut last = here;
    for _ in 0..30 {
        let target = route
            .look_ahead(&mut here, 25.0)
            .expect("a closed path never ends");
        last = target;
    }
    assert!(last.iter().all(|v| v.is_finite()));
}

#[test]
fn reversing_and_setting_the_direction() {
    let mut route = line(&[[0., 0., 0.], [100., 0., 0.]], 0);
    assert!(route.forward);
    assert!(!route.reverse());
    route.set_forward(true);
    assert!(route.forward);
    route.fixed = true;
    route.set_forward(false);
    assert!(route.forward);
}

#[test]
fn leaving_a_way_follows_the_side_away_from_the_other() {
    let mut route = line(&[[0., 0., 0.], [100., 0., 0.], [200., 0., 0.]], 0);
    route.forward = false;
    // The other object is behind (lower x): the walker goes forward.
    route.leave_way(&[100.0, 0.0, 0.0, 1.0], &[40.0, 0.0, 0.0, 1.0]);
    assert!(route.forward);
    route.leave_way(&[100.0, 0.0, 0.0, 1.0], &[160.0, 0.0, 0.0, 1.0]);
    assert!(!route.forward);
}

#[test]
fn an_active_path_collects_points_from_its_target() {
    let object = [0.0, 0.0, 0.0, 1.0];
    let target = [500.0, 0.0, 0.0, 1.0];
    let mut route = Route::active(7, &object, &target);
    assert!(route.active && route.target == Some(7));
    // The object, a point 110 short of the target and the target.
    assert_eq!(route.count, 3);
    assert_eq!(route.point(0)[0], 0.0);
    assert!(near(route.point(1)[0], 390.0, 1.0e-3));
    assert_eq!(route.point(2)[0], 500.0);
    // A target that barely moved adds nothing; one 100 away adds a node.
    route.follow_point(&[510.0, 0.0, 0.0, 1.0]);
    assert_eq!(route.count, 3);
    route.follow_point(&[620.0, 0.0, 0.0, 1.0]);
    assert_eq!(route.count, 4);
    route.clear();
    assert_eq!((route.count, route.target), (0, None));
}

#[test]
fn walking_a_length_moves_the_parameter() {
    let mut route = line(&[[0., 0., 0.], [100., 0., 0.], [200., 0., 0.]], 0);
    route.advance_by_length(1.0);
    let moved = route.position()[0];
    assert!(near(moved, 1.0, 0.2), "{moved}");
}
