//! Synthetic objects only.
use super::*;
use crate::env::NullEnv;
use crate::script::{Instance, Thread};
use crate::world::{Object, World, object_handle};

/// An AI reference whose word at +4 names the engine object.
fn reference(world: &mut World, id: ObjId) -> u32 {
    world.ai_reference(object_handle(id))
}

fn run(
    world: &mut World,
    me: &mut Object,
    trap: (u16, u16),
    args: &mut [Slot; 16],
) -> Result<(), Fault> {
    let mut ctx = Ctx {
        world,
        me,
        env: &NullEnv,
    };
    let mut inst = Instance::new(0, &{
        let mut h = vec![0u8; 44];
        h[..4].copy_from_slice(b"test");
        h[16..20].copy_from_slice(&64u32.to_le_bytes());
        h[20..24].copy_from_slice(&64u32.to_le_bytes());
        h[24..28].copy_from_slice(&64u32.to_le_bytes());
        h
    })
    .unwrap();
    let mut th = Thread::new(&inst, 0, 0, &[]);
    let mut memory = Memory::new(&mut inst, &mut th);
    // Some traps the shield uses (attach, detach, motion sync) live in other
    // trap modules: the whole table answers.
    let f = crate::traps::lookup(trap.0, trap.1).expect("the trap is translated");
    f(&mut ctx, &mut memory, args)
}

fn world_with_shield() -> (World, Object) {
    let mut world = World::default();
    let mut goofy = Object::bare(0);
    goofy.sys.weapons = vec![1];
    let mut shield = Object::bare(1);
    shield.sys.attached = Some(crate::sysobj::Attach {
        to: 0,
        bone: 0,
        flags: 0,
        callback: None,
    });
    shield.sys.synced = true;
    world.objects = vec![None, Some(Box::new(shield))];
    (world, goofy)
}

#[test]
fn the_weapon_slot_names_the_shield_and_an_empty_slot_names_nothing() {
    let (mut world, mut goofy) = world_with_shield();
    let owner = reference(&mut world, 0);
    let mut args = [Slot::default(); 16];
    args[0] = Slot::address(owner);
    args[1] = Slot::int(0);
    run(&mut world, &mut goofy, (1, 234), &mut args).unwrap();
    assert_eq!(args[0].value, object_handle(1));
    args[0] = Slot::address(owner);
    args[1] = Slot::int(3);
    run(&mut world, &mut goofy, (1, 234), &mut args).unwrap();
    assert_eq!(args[0].value, 0);
}

#[test]
fn detaching_and_unsyncing_free_the_shield_and_attaching_takes_it_back() {
    let (mut world, mut goofy) = world_with_shield();
    let owner = reference(&mut world, 0);
    let shield = reference(&mut world, 1);
    let mut args = [Slot::default(); 16];
    // Attached and synced to begin with.
    args[0] = Slot::address(shield);
    run(&mut world, &mut goofy, (1, 161), &mut args).unwrap();
    assert_eq!(args[0].as_int(), 1);
    args[0] = Slot::address(owner);
    args[1] = Slot::address(shield);
    run(&mut world, &mut goofy, (10, 40), &mut args).unwrap();
    assert_eq!(args[0].as_int(), 1);
    // The throw: detach, then leave the owner's motion list.
    args[0] = Slot::address(shield);
    run(&mut world, &mut goofy, (1, 102), &mut args).unwrap();
    args[0] = Slot::address(owner);
    args[1] = Slot::address(shield);
    run(&mut world, &mut goofy, (1, 236), &mut args).unwrap();
    let sys = &world.objects[1].as_ref().unwrap().sys;
    assert!(sys.attached.is_none() && !sys.synced);
    // The return: follow the owner's motion again and attach to the hand.
    args[0] = Slot::address(owner);
    args[1] = Slot::address(shield);
    run(&mut world, &mut goofy, (1, 126), &mut args).unwrap();
    args[0] = Slot::address(shield);
    args[1] = Slot::address(owner);
    args[2] = Slot::int(5);
    run(&mut world, &mut goofy, (1, 18), &mut args).unwrap();
    let sys = &world.objects[1].as_ref().unwrap().sys;
    assert_eq!(sys.attached.map(|a| a.to), Some(0));
    assert!(sys.synced);
}

#[test]
fn the_bone_point_falls_back_to_the_object_and_nothing_is_a_summon() {
    let (mut world, mut goofy) = world_with_shield();
    goofy.sys.position = [10., 20., 30.];
    let me = reference(&mut world, 0);
    let local = world.alloc(16);
    for (k, v) in [1.0f32, 2.0, 3.0, 1.0].iter().enumerate() {
        assert!(world.static_write(local + 4 * k as u32, v.to_bits()));
    }
    let mut args = [Slot::default(); 16];
    args[0] = Slot::address(me);
    args[1] = Slot::int(3);
    args[2] = Slot::address(local);
    run(&mut world, &mut goofy, (1, 83), &mut args).unwrap();
    // The null host has no bone data: the object's own place comes back
    // (the script's vectors are the engine's: (x, -y, -z) of the scene).
    let cell = args[0].value;
    let read = |k: u32| f32::from_bits(world.static_read(cell + 4 * k).unwrap());
    assert_eq!([read(0), read(1), read(2), read(3)], [10., -20., -30., 1.]);
    // `sysobj_is_summon` takes the object again.
    args[0] = Slot::address(me);
    run(&mut world, &mut goofy, (10, 31), &mut args).unwrap();
    assert_eq!(args[0].as_int(), 0);
}
