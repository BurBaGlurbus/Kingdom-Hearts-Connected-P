//! The route traps on a synthetic walk path.
use super::*;
use crate::env::{Clips, Env, NullEnv};
use crate::route::WalkPath;
use crate::script::Slot;
use crate::sysobj::Vec3;
use crate::traps::test_support::{Frame, ai_object, frame, world_with};
use crate::world::{Object, World};

/// A map with one walk path along +x for unit 1, in the script frame.
struct Paths;

impl Env for Paths {
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
    fn walk_path(&self, unit: Option<u32>, key: PathKey) -> Option<WalkPath> {
        let by_id = matches!(key, PathKey::Id(_));
        (unit == Some(1) && by_id).then(|| WalkPath {
            id: 3,
            flags: 0,
            group: 2,
            points: vec![[0., 0., 0.], [100., 0., 0.], [200., 0., 0.]],
        })
    }
    fn walk_unit(&self, id: u32) -> Option<u32> {
        (id == 9).then_some(1)
    }
    fn event_path(&self) -> Vec<(i32, i32)> {
        vec![(10, 11), (20, 21)]
    }
}

fn run(
    env: &dyn Env,
    world: &mut World,
    me: &mut Object,
    f: &mut Frame,
    trap: TrapFn,
    args: &[Slot],
) -> Slot {
    let mut slots = [Slot::default(); 16];
    slots[..args.len()].copy_from_slice(args);
    let mut memory = Memory::new(&mut f.inst, &mut f.th);
    let mut ctx = Ctx { world, me, env };
    trap(&mut ctx, &mut memory, &mut slots).unwrap();
    slots[0]
}

fn lane(world: &World, slot: Slot, k: u32) -> f32 {
    f32::from_bits(world.static_read(slot.value + 4 * k).unwrap())
}

#[test]
fn a_unit_path_becomes_the_objects_route() {
    let mut world = world_with(&[0., 10.]);
    world.objects[1].as_mut().unwrap().sys.unit = Some(1);
    let mut me = Object::bare(0);
    let mut f = frame();
    let ai = ai_object(&mut f, 0, 1);
    let ok = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        obj_set_path,
        &[Slot::address(ai), Slot::int(3)],
    );
    assert_eq!(ok.as_int(), 1);
    let route = world.objects[1]
        .as_ref()
        .unwrap()
        .sys
        .route
        .as_ref()
        .unwrap();
    assert!(!route.active && route.forward);
    // The object stood at the first point, so the walker is there.
    let first = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        get_path_first_point_pos,
        &[Slot::address(ai)],
    );
    assert_eq!(lane(&world, first, 0), 0.0);
    let last = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        get_path_last_point_pos,
        &[Slot::address(ai)],
    );
    assert_eq!(lane(&world, last, 0), 200.0);
    let now = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        get_path_current_pos,
        &[Slot::address(ai)],
    );
    assert!(lane(&world, now, 0).abs() < 1.0);
}

#[test]
fn an_object_without_a_unit_gets_no_route() {
    let mut world = world_with(&[0.]);
    let mut me = Object::bare(0);
    let mut f = frame();
    let ai = ai_object(&mut f, 0, 1);
    let ok = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        obj_set_path,
        &[Slot::address(ai), Slot::int(3)],
    );
    assert_eq!(ok.as_int(), 0);
    assert!(world.objects[1].as_ref().unwrap().sys.route.is_none());
    // The path direction of a routeless object leaves the slot alone.
    let kept = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        get_path_dir,
        &[Slot::address(ai), Slot::float(30.0)],
    );
    assert_eq!(kept.value, ai);
}

#[test]
fn the_path_direction_points_along_the_path() {
    let mut world = world_with(&[20.]);
    world.objects[1].as_mut().unwrap().sys.unit = Some(1);
    let mut me = Object::bare(0);
    let mut f = frame();
    let ai = ai_object(&mut f, 0, 1);
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        obj_set_path,
        &[Slot::address(ai), Slot::int(3)],
    );
    let dir = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        get_path_dir,
        &[Slot::address(ai), Slot::float(30.0)],
    );
    assert!(lane(&world, dir, 0) > 0.99, "{}", lane(&world, dir, 0));
    assert_eq!(lane(&world, dir, 3), 1.0);
    let way = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        reverse_path_way,
        &[Slot::address(ai)],
    );
    assert_eq!(way.as_int(), 0);
}

#[test]
fn a_path_by_spawn_point_id_uses_that_units_first_path() {
    let mut world = world_with(&[0.]);
    let mut me = Object::bare(0);
    let mut f = frame();
    let ai = ai_object(&mut f, 0, 1);
    let miss = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        set_path_by_group,
        &[Slot::address(ai), Slot::int(4)],
    );
    assert_eq!(miss.as_int(), 0);
    let hit = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        set_path_by_group,
        &[Slot::address(ai), Slot::int(9)],
    );
    assert_eq!(hit.as_int(), 1);
    assert!(world.objects[1].as_ref().unwrap().sys.route.is_some());
}

#[test]
fn an_active_path_follows_another_object() {
    let mut world = world_with(&[0., 500.]);
    let mut me = Object::bare(0);
    let mut f = frame();
    let a = ai_object(&mut f, 0, 1);
    let b = ai_object(&mut f, 16, 2);
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        create_active_path,
        &[Slot::address(a), Slot::address(b)],
    );
    let route = world.objects[1]
        .as_ref()
        .unwrap()
        .sys
        .route
        .as_ref()
        .unwrap();
    assert!(route.active && route.target == Some(2));
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        clear_active_path,
        &[Slot::address(a)],
    );
    let route = world.objects[1]
        .as_ref()
        .unwrap()
        .sys
        .route
        .as_ref()
        .unwrap();
    assert_eq!(route.target, None);
}

#[test]
fn ending_an_action_clears_it_and_starts_the_wait() {
    let mut world = world_with(&[0.]);
    {
        let sys = &mut world.objects[1].as_mut().unwrap().sys;
        sys.rtn_time[8] = 180;
        sys.rtn.action.kind = 4;
        sys.rtn.action.dir_kind = 2;
        sys.rtn.action.started = true;
        sys.rtn.action.timer = 5.0;
    }
    let mut me = Object::bare(0);
    let mut f = frame();
    let ai = ai_object(&mut f, 0, 1);
    let words = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        get_rtn_action,
        &[Slot::address(ai)],
    );
    assert_eq!(world.static_read(words.value).unwrap(), 4);
    assert_eq!(world.static_read(words.value + 4).unwrap(), 2);
    // A zone action with Sora out of its sphere is over; one with her in it
    // is not.
    let over = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        is_end_rtn_action,
        &[Slot::address(ai)],
    );
    assert_eq!(over.as_int(), 1);
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        end_rtn_action,
        &[Slot::address(ai)],
    );
    let sys = &world.objects[1].as_ref().unwrap().sys;
    assert_eq!(sys.rtn.cooldown, 180);
    assert_eq!(sys.rtn.action.kind, 0);
    assert_eq!(sys.rtn.saved.kind, 4);
    let over = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        is_end_rtn_action,
        &[Slot::address(ai)],
    );
    assert_eq!(over.as_int(), 1, "no action running is over");
}

#[test]
fn the_event_path_walks_the_list_and_wraps() {
    let mut world = world_with(&[0.]);
    let mut me = Object::bare(0);
    let mut f = frame();
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_move_next,
        &[],
    );
    assert_eq!(world.eh22.moving, 1);
    assert_eq!(
        (world.eh22.current, world.eh22.next, world.eh22.value),
        (0, 1, 10.0)
    );
    let busy = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_is_moving,
        &[],
    );
    assert_eq!(busy.as_int(), 1);
    // A move in progress is not interrupted.
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_move_next,
        &[],
    );
    assert_eq!(world.eh22.next, 1);
    world.eh22.moving = 0;
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_move_before,
        &[],
    );
    assert_eq!(world.eh22.moving, 2);
    assert_eq!(
        (world.eh22.current, world.eh22.next, world.eh22.value),
        (0, 0, 11.0)
    );
    world.eh22.moving = 0;
    // Before the first point is the last.
    run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_move_before,
        &[],
    );
    assert_eq!(world.eh22.current, 1);
    let point = run(
        &Paths,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_get_point,
        &[],
    );
    assert_eq!(point.as_int(), 1);
}

#[test]
fn without_an_event_path_nothing_starts_moving() {
    let mut world = world_with(&[0.]);
    let mut me = Object::bare(0);
    let mut f = frame();
    run(
        &NullEnv,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_move_next,
        &[],
    );
    run(
        &NullEnv,
        &mut world,
        &mut me,
        &mut f,
        eh22_path_move_before,
        &[],
    );
    assert_eq!(world.eh22.moving, 0);
}
