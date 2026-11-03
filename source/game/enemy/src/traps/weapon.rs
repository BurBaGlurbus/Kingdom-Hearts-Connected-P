//! Weapon objects and the script threads they run on: Goofy's thrown shield
//! (`blade`, the Bash) detaches his shield, flies it as an object of its own
//! and puts it back in his hand. Argument conventions follow the executable's
//! handlers: an object argument is an AI reference whose word at +4 is the
//! engine object.

use super::math::{native, native_local, read_sanitized};
use crate::script::{Fault, Memory, Slot};
use crate::world::{Ctx, ObjId, TrapFn, object_handle};

type Args = [Slot; 16];

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    Some(match (table, index) {
        (1, 82) => obj_thread_start,
        (1, 83) => apply_bone_matrix,
        (1, 126) => motion_sync,
        (1, 234) => obj_weapon,
        (1, 235) => party_hand_to_bone,
        (1, 236) => motion_unsync,
        (1, 276) => obj_use_mp,
        (10, 40) => is_motion_sync,
        _ => return None,
    })
}

fn object_arg(ctx: &Ctx<'_>, m: &Memory<'_>, slot: Slot) -> Result<ObjId, Fault> {
    ctx.sys_of_ai(m, slot.value)
}

/// 1:82 FUN_14042f660 `trap_obj_thread_start(obj, kind, entry, tag, argument)`:
/// a thread of the object's program; a non-zero `kind` first ends the
/// threads of that kind. The caller is the object itself in every use.
fn obj_thread_start(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let _ = ctx;
    let (kind, entry, tag) = (args[1].value, args[2].as_int(), args[3].value);
    if kind != 0 {
        m.kill_children(kind, kind);
    }
    let argument = args[4];
    let list: &[Slot] = if argument.value != 0 {
        std::slice::from_ref(&argument)
    } else {
        &[]
    };
    let handle = m.spawn_child(kind, tag, entry, list);
    args[0] = Slot::address(handle);
    Ok(())
}

/// 1:83 FUN_14042f6f0 `trap_obj_apply_bone_matrix(obj, bone, point)`: the
/// point of the bone's model in the world (the executable's Y-down vector
/// back out); the object's own position when the host has no bone data.
fn apply_bone_matrix(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = object_arg(ctx, m, args[0])?;
    let bone = args[1].as_int();
    let local = native_local(read_sanitized(ctx, m, args[2].value)?);
    let (own, yaw) = {
        let sys = &ctx.object(id)?.sys;
        (sys.position, sys.yaw)
    };
    let world = ctx
        .env
        .bone_point(id, bone, [local[0], local[1], local[2]], own, yaw)
        .unwrap_or(own);
    let out = native([world[0], world[1], world[2], 1.0]);
    let cell = ctx.world.static_cell(1, 83, 0);
    for (k, lane) in out.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 1:126 `trap_obj_motion_sync(owner, weapon)`: the weapon's motion follows the
/// owner's again.
fn motion_sync(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let weapon = object_arg(ctx, m, args[1])?;
    ctx.object(weapon)?.sys.synced = true;
    Ok(())
}

/// 1:236 FUN_14042d310 (FUN_1403c8f20): the weapon leaves the owner's motion
/// list.
fn motion_unsync(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let weapon = object_arg(ctx, m, args[1])?;
    ctx.object(weapon)?.sys.synced = false;
    Ok(())
}

/// 10:40 `trap_obj_is_motion_sync(owner, weapon)` (FUN_1403c7fb0).
fn is_motion_sync(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let weapon = object_arg(ctx, m, args[1])?;
    let synced = ctx.object(weapon)?.sys.synced;
    args[0] = Slot::int(synced as i32);
    Ok(())
}

/// 1:234 FUN_140431310: the object in weapon slot `args[1]` (+0xd60).
fn obj_weapon(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = object_arg(ctx, m, args[0])?;
    let slot = args[1].as_int().max(0) as usize;
    let weapon = ctx.object(id)?.sys.weapons.get(slot).copied();
    args[0] = Slot::address(weapon.map_or(0, object_handle));
    Ok(())
}

/// 1:235 FUN_140431360 (FUN_1403d7620): the bone a party member's weapon
/// slot sits on. The host places the weapon, so the slot number stands in.
fn party_hand_to_bone(_: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    args[0] = Slot::int(args[1].as_int());
    Ok(())
}

/// 1:276 FUN_14042be40: spending the object's MP is the host's.
fn obj_use_mp(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, _: &mut Args) -> Result<(), Fault> {
    ctx.world
        .events
        .push(crate::world::Event::Unsupported("obj_use_mp"));
    Ok(())
}

#[cfg(test)]
mod tests;
