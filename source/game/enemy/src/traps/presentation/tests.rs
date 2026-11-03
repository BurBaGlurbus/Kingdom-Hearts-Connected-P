//! Head look, host reads and the light-cycle steering on synthetic objects.
use super::*;
use crate::env::{Clips, Env, NullEnv};
use crate::sysobj::Vec3;
use crate::traps::test_support::{ai_object, call, frame, world_with};
use crate::world::Object;

#[test]
fn a_model_without_a_look_shape_never_looks() {
    let mut world = world_with(&[0., 50.]);
    let mut me = Object::bare(0);
    let mut f = frame();
    let a = ai_object(&mut f, 0, 1);
    let b = ai_object(&mut f, 16, 2);
    call(
        &mut world,
        &mut me,
        &mut f,
        look_start,
        &[Slot::address(a), Slot::address(b), Slot::float(20.0)],
    );
    let look = world.objects[1].as_ref().unwrap().sys.head_look;
    assert_eq!((look.state, look.target), (0, None));
}

#[test]
fn looking_blends_in_and_out() {
    let mut world = world_with(&[0., 50.]);
    world.objects[1].as_mut().unwrap().sys.head_look.bone = Some(4);
    let mut me = Object::bare(0);
    let mut f = frame();
    let a = ai_object(&mut f, 0, 1);
    let b = ai_object(&mut f, 16, 2);
    call(
        &mut world,
        &mut me,
        &mut f,
        look_start,
        &[Slot::address(a), Slot::address(b), Slot::float(20.0)],
    );
    let look = world.objects[1].as_ref().unwrap().sys.head_look;
    assert_eq!((look.state, look.target, look.kind), (2, Some(2), 1));
    assert_eq!(look.blend.value, 0.0);
    assert!((look.blend.rate - 0.05).abs() < 1.0e-6);
    world.objects[1].as_mut().unwrap().sys.head_look.blend.value = 0.6;
    call(
        &mut world,
        &mut me,
        &mut f,
        look_end,
        &[Slot::address(a), Slot::float(30.0)],
    );
    let blend = world.objects[1].as_ref().unwrap().sys.head_look.blend;
    assert!((blend.rate + 0.02).abs() < 1.0e-6, "{blend:?}");
}

/// A host that knows the camera and a bone.
struct Host;
impl Env for Host {
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
    fn camera_eye(&self) -> Option<Vec3> {
        Some([1., 2., 3.])
    }
    fn bone_matrix(&self, _: u32, bone: u32) -> Option<[f32; 16]> {
        // A translation by (10 * bone, 0, 0).
        let mut m = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        m[12] = 10.0 * bone as f32;
        Some(m)
    }
    fn mission_gauge_ratio(&self, index: i32) -> f32 {
        index as f32 / 4.0
    }
}

fn run_with(
    env: &dyn Env,
    world: &mut crate::world::World,
    f: &mut crate::traps::test_support::Frame,
    trap: TrapFn,
    args: &[Slot],
) -> Slot {
    let mut me = Object::bare(0);
    let mut slots = [Slot::default(); 16];
    slots[..args.len()].copy_from_slice(args);
    let mut memory = Memory::new(&mut f.inst, &mut f.th);
    let mut ctx = Ctx {
        world,
        me: &mut me,
        env,
    };
    trap(&mut ctx, &mut memory, &mut slots).unwrap();
    slots[0]
}

#[test]
fn the_host_answers_the_camera_the_bones_and_the_gauge() {
    let mut world = world_with(&[0.]);
    let mut f = frame();
    let a = ai_object(&mut f, 0, 1);
    let eye = run_with(&Host, &mut world, &mut f, camera_eye, &[]);
    let lanes: Vec<f32> = (0..4)
        .map(|k| f32::from_bits(world.static_read(eye.value + 4 * k).unwrap()))
        .collect();
    assert_eq!(lanes, vec![1.0, -2.0, -3.0, 1.0]);
    // The null host has no camera: the zero vector with w 1.
    let none = run_with(&NullEnv, &mut world, &mut f, camera_at, &[]);
    assert_eq!(
        f32::from_bits(world.static_read(none.value + 12).unwrap()),
        1.0
    );
    let ratio = run_with(
        &Host,
        &mut world,
        &mut f,
        mission_gauge_ratio,
        &[Slot::int(3)],
    );
    assert_eq!(ratio.as_float(), 0.75);
    let base = f.inst.work_base() + 64;
    {
        let mut memory = Memory::new(&mut f.inst, &mut f.th);
        for (k, v) in [1.0f32, 2.0, 3.0, 1.0].iter().enumerate() {
            memory.write_f32(base + 4 * k as u32, *v);
        }
    }
    let moved = run_with(
        &Host,
        &mut world,
        &mut f,
        apply_bone_matrix,
        &[Slot::address(a), Slot::int(2), Slot::address(base)],
    );
    let x = f32::from_bits(world.static_read(moved.value).unwrap());
    assert_eq!(x, 21.0);
}

#[test]
fn the_light_cycle_steers_towards_the_other_and_reports_the_distance() {
    // Both cycles face +z; the other is 100 ahead and a little to one side.
    let own = Cycle {
        position: [0., 0., 0., 1.],
        facing: [0., 0., 1., 0.],
    };
    let other = Cycle {
        position: [40., 0., 100., 1.],
        facing: [0., 0., 1., 0.],
    };
    let v = lightcycle_steer(&own, &other, 2.0, 0.0, 1.0);
    // With no speed it stays put: the distance is to the other, normalised.
    let expected = (40.0f32 * 40.0 + 100.0 * 100.0).sqrt();
    assert!((v[3] - expected).abs() < 1.0e-3, "{v:?}");
    assert!(v[0] == 1.0 || v[0] == -1.0);
    assert_eq!(v[1], 0.0);
    // Mirroring the other across the cycle's line flips the turn side.
    let mirrored = Cycle {
        position: [-40., 0., 100., 1.],
        ..other
    };
    let w = lightcycle_steer(&own, &mirrored, 2.0, 0.0, 1.0);
    assert_eq!(v[0], -w[0]);
    assert!((w[3] - v[3]).abs() < 1.0e-3);
}

#[test]
fn a_captured_motion_hands_out_a_token_until_the_object_moves_on() {
    let mut world = world_with(&[0., 50.]);
    let mut me = Object::bare(0);
    let mut f = frame();
    let captor = ai_object(&mut f, 0, 1);
    let victim = ai_object(&mut f, 16, 2);
    let capture = crate::traps::lookup(1, 29).unwrap();
    let capture_id = crate::traps::lookup(1, 238).unwrap();
    let token = call(
        &mut world,
        &mut me,
        &mut f,
        capture,
        &[
            Slot::address(captor),
            Slot::address(victim),
            Slot::int(7),
            Slot::float(10.0),
        ],
    );
    assert_eq!(token.as_int(), 1);
    let read = call(
        &mut world,
        &mut me,
        &mut f,
        capture_id,
        &[Slot::address(victim)],
    );
    assert_eq!(read.as_int(), 1);
    let sys = &mut world.objects[2].as_mut().unwrap().sys;
    assert_eq!(sys.motion.flags.word >> 14 & 1, 1);
    assert_eq!(sys.capture.map(|c| (c.captor, c.motion)), Some((1, 7)));
    assert!(world.events.iter().any(|e| matches!(
        e,
        crate::world::Event::MotionCapture {
            victim: 2,
            motion: 7,
            ..
        }
    )));
}

/// A model whose motion 7 is three ticks long.
struct ShortClip;
struct ThreeTicks;
impl Clips for ThreeTicks {
    fn length(&self, slot: usize) -> u32 {
        if slot == 7 { 3 } else { 0 }
    }
    fn loop_start(&self, _: usize) -> u32 {
        0
    }
    fn root_delta(&self, _: usize, _: u32) -> Vec3 {
        [0.; 3]
    }
    fn events(&self, _: usize) -> Option<&crate::motion::MotionEvents> {
        None
    }
}
impl Env for ShortClip {
    fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        NullEnv.floor(x, z, top)
    }
    fn clips(&self, _: u32) -> &dyn Clips {
        &ThreeTicks
    }
    fn player_position(&self) -> Vec3 {
        [0.; 3]
    }
    fn target_position(&self, _: u32) -> Option<Vec3> {
        None
    }
}

#[test]
fn a_captured_motion_ends_when_the_borrowed_clip_has_played() {
    let mut world = world_with(&[0., 50.]);
    let mut f = frame();
    let captor = ai_object(&mut f, 0, 1);
    let victim = ai_object(&mut f, 16, 2);
    let capture = crate::traps::lookup(1, 29).unwrap();
    let capture_id = crate::traps::lookup(1, 238).unwrap();
    let token = run_with(
        &ShortClip,
        &mut world,
        &mut f,
        capture,
        &[
            Slot::address(captor),
            Slot::address(victim),
            Slot::int(7),
            Slot::float(0.0),
        ],
    );
    assert_eq!(token.as_int(), 1);
    for _ in 0..2 {
        world.tick_captures(None);
        let id = run_with(
            &ShortClip,
            &mut world,
            &mut f,
            capture_id,
            &[Slot::address(victim)],
        );
        assert_eq!(id.as_int(), 1, "still captured");
    }
    world.tick_captures(None);
    let id = run_with(
        &ShortClip,
        &mut world,
        &mut f,
        capture_id,
        &[Slot::address(victim)],
    );
    assert_eq!(id.as_int(), 0);
    let sys = &world.objects[2].as_ref().unwrap().sys;
    assert!(sys.motion_ended());
    assert_eq!(sys.motion.flags.word >> 14 & 1, 0);
    // Sora's stand-in is stepped as well.
    let proxy_token = {
        let proxy = world.player_proxy.as_mut().unwrap();
        proxy.sys.capture = Some(crate::sysobj::Capture {
            captor: 1,
            motion: 7,
            token: 9,
            remaining: 1,
            length: 1,
            loop_start: None,
            next: None,
        });
        proxy.sys.capture.map(|c| c.token)
    };
    assert_eq!(proxy_token, Some(9));
    world.tick_captures(None);
    assert!(world.player_proxy.as_ref().unwrap().sys.capture.is_none());
}
