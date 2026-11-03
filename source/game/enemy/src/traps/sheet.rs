//! Traps on the battle sheet (`sysobj + 0x5c0`): the HP pools and damage
//! tuning scripts read and change. A pool is `{hp, max_hp, min_hp}` (twelve
//! bytes); pool 0 is the object's own HP.
//!
//! [U] Pools above 0 exist only when the host fills `Sysobj::extra_pools` from
//! the sheet data; the element-rate setter's index mapping is not confirmed.

use crate::script::{Fault, Memory, Slot};
use crate::sysobj::Pool;
use crate::world::{Ctx, OBJECT_HANDLES, PLAYER_HANDLE, PLAYER_OBJECT, TrapFn, object_handle};

type Args = [Slot; 16];

/// The sheet handle range inside an object's handle range.
pub const SHEET_OFFSET: u32 = 0xc00;

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    if table == 10 && index == 45 {
        return Some(drive_time);
    }
    if table != 1 {
        return None;
    }
    Some(match index {
        84 => sheet_of,
        228 => pool_hp,
        229 => pool_max,
        230 => pool_rate,
        231 => set_min_hp,
        232 => pool_min,
        233 => set_hp,
        311 => set_element_rate,
        329 => set_prize_range,
        348 => set_hitback_addition,
        _ => return None,
    })
}

fn owner(handle: u32) -> Result<u32, Fault> {
    if handle == PLAYER_HANDLE + SHEET_OFFSET {
        return Ok(PLAYER_OBJECT);
    }
    let o = handle
        .checked_sub(OBJECT_HANDLES)
        .filter(|o| o % 0x1000 == SHEET_OFFSET)
        .ok_or(Fault::Memory(handle))?;
    Ok(o / 0x1000)
}

fn pool_of(ctx: &mut Ctx<'_>, handle: u32, index: i32) -> Result<Pool, Fault> {
    let id = owner(handle)?;
    let sys = &ctx.object(id)?.sys;
    sys.pool(usize::try_from(index).map_err(|_| Fault::Memory(handle))?)
        .ok_or(Fault::Memory(handle))
}

/// 10:45 FUN_140536f80: the sheet's +0x1b4, the form or summon time left
/// (Sora's; other sheets keep none here).
fn drive_time(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = owner(args[0].value)?;
    let t = if id == PLAYER_OBJECT {
        ctx.env.drive_time()
    } else {
        0.
    };
    args[0] = Slot::float(t);
    Ok(())
}

/// 1:84 FUN_14042f7d0: the sheet of the AI object's engine object.
fn sheet_of(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    args[0] = Slot::address(object_handle(id) + SHEET_OFFSET);
    Ok(())
}

/// 1:228 FUN_140431030.
fn pool_hp(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let p = pool_of(ctx, args[0].value, args[1].as_int())?;
    args[0] = Slot::int(p.hp);
    Ok(())
}

/// 1:229 FUN_140431070.
fn pool_max(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let p = pool_of(ctx, args[0].value, args[1].as_int())?;
    args[0] = Slot::int(p.max);
    Ok(())
}

/// 1:230 FUN_1404310f0: `(float)hp / (float)max` (no zero check).
fn pool_rate(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let p = pool_of(ctx, args[0].value, args[1].as_int())?;
    args[0] = Slot::float(p.hp as f32 / p.max as f32);
    Ok(())
}

/// 1:232 FUN_1404310b0.
fn pool_min(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let p = pool_of(ctx, args[0].value, args[1].as_int())?;
    args[0] = Slot::int(p.min);
    Ok(())
}

/// 1:231 FUN_140431190: the HP floor of a pool.
fn set_min_hp(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = owner(args[0].value)?;
    let (value, index) = (args[1].as_int(), args[2].as_int());
    let sys = &mut ctx.object(id)?.sys;
    sys.set_pool_min(usize::try_from(index).unwrap_or(usize::MAX), value);
    Ok(())
}

/// 1:233 FUN_140431140: set a pool's HP, clamped to `[min, max]` (a value
/// below the floor lands on the floor).
fn set_hp(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = owner(args[0].value)?;
    let (value, index) = (args[1].as_int(), args[2].as_int());
    let sys = &mut ctx.object(id)?.sys;
    sys.set_pool_hp(usize::try_from(index).unwrap_or(usize::MAX), value);
    Ok(())
}

/// 1:311 FUN_140431200 -> FUN_1403c2650: a damage-taken percentage by element.
fn set_element_rate(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = owner(args[0].value)?;
    let (element, rate) = (args[1].as_int(), args[2].as_int());
    let sys = &mut ctx.object(id)?.sys;
    if let Some(w) = usize::try_from(element)
        .ok()
        .and_then(|e| sys.stats.weakness.get_mut(e))
    {
        *w = rate;
    }
    Ok(())
}

/// 1:329 FUN_140431230: +0x1c4.
fn set_prize_range(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = owner(args[0].value)?;
    ctx.object(id)?.sys.prize_range = args[1].as_int();
    Ok(())
}

/// 1:348 FUN_140431260: +0x1c8.
fn set_hitback_addition(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = owner(args[0].value)?;
    ctx.object(id)?.sys.hitback_addition = args[1].as_int();
    Ok(())
}
