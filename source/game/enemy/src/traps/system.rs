//! Table 0 traps outside the math group: scratch memory, game progress flags
//! and the effect queries scripts make on handles `trap_obj_effect_start_bind`
//! returned.
//!
//! [U] Effects and progress belong to the host; these traps ask the
//! [`Env`](crate::env::Env) and otherwise record the request as an event.

use crate::script::{Fault, Memory, Slot};
use crate::world::{Ctx, Event, TrapFn};

type Args = [Slot; 16];

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    if table != 0 {
        return None;
    }
    Some(match index {
        9 => thread_start,
        15 => progress_check_flag,
        47 => memory_alloc,
        48 => memory_free,
        50 => effect_is_active,
        53 => effect_pos,
        38 => effect_command::<38>,
        39 => effect_command::<39>,
        40 => effect_command::<40>,
        41 => effect_command::<41>,
        44 => effect_command::<44>,
        51 => effect_command::<51>,
        52 => effect_command::<52>,
        66 => effect_command::<66>,
        67 => effect_command::<67>,
        85 => effect_command::<85>,
        79 => effect_add_dead_block,
        _ => return None,
    })
}

/// 0:9 FUN_14041da20: start a child thread of the running thread. `kind`
/// nonzero first ends the children already carrying those kind bits.
fn thread_start(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let _ = ctx;
    let (kind, entry, tag) = (args[0].value, args[1].as_int(), args[2].value);
    if kind != 0 {
        m.kill_children(kind, kind);
    }
    let argument = args[3];
    let list: &[Slot] = if argument.value != 0 {
        std::slice::from_ref(&argument)
    } else {
        &[]
    };
    let handle = m.spawn_child(kind, tag, entry, list);
    args[0] = Slot::address(handle);
    Ok(())
}

/// Effect commands on a handle `effect_start_bind` returned (the trap index
/// `T`): 38 position, 39 rotation, 40 scale, 41 direction, 44 loop end, 51
/// kill, 52 fade out, 66/67 pause and colour, 85 stop. The effect system is
/// the host's; the command and its operands are recorded.
fn effect_command<const T: u16>(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let handle = args[0].value;
    let operand = [args[1], args[2]];
    let vector = ctx.read_f32(m, operand[0].value).and_then(|x| {
        Some([
            x,
            ctx.read_f32(m, operand[0].value + 4)?,
            ctx.read_f32(m, operand[0].value + 8)?,
        ])
    });
    ctx.world.events.push(Event::EffectCommand {
        trap: T,
        handle,
        words: [operand[0].value, operand[1].value],
        vector,
    });
    Ok(())
}

/// 0:15 FUN_14041db60 -> FUN_14039a980: a game progress flag.
fn progress_check_flag(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let set = ctx.env.progress_flag(args[0].value);
    args[0] = Slot::int(set as i32);
    Ok(())
}

/// 0:47 FUN_14041e630 -> FUN_140152430: a zeroed block of `size` bytes.
fn memory_alloc(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let handle = ctx.world.alloc(args[0].value);
    args[0] = Slot::address(handle);
    Ok(())
}

/// 0:48 FUN_14041e660 -> FUN_140152570.
fn memory_free(_: &mut Ctx<'_>, _: &mut Memory<'_>, _: &mut Args) -> Result<(), Fault> {
    Ok(())
}

/// 0:50 FUN_14041e220 -> FUN_1402be780: the effect still exists.
fn effect_is_active(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let alive = ctx.env.effect_active(args[0].value);
    args[0] = Slot::int(alive as i32);
    Ok(())
}

/// 0:53 FUN_14041e2c0 -> FUN_1402be970: an effect position (own result cell).
fn effect_pos(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let p = ctx.env.effect_position(args[0].value).unwrap_or([0.; 3]);
    let cell = ctx.world.static_cell(0, 53, 0);
    for (k, lane) in [p[0], p[1], p[2], 1.0].iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 0:79 FUN_14041e5a0 -> FUN_1403f63a0: tie an effect to the object's death.
fn effect_add_dead_block(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    ctx.world.events.push(Event::EffectDeadBlock {
        object: ctx.me.id,
        handle: args[0].value,
    });
    Ok(())
}
