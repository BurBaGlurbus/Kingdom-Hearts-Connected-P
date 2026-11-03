//! Table 10: the kn::Friend traps the summon scripts call. "Self" is the
//! owner of the script's AI object (the friend); the personality traps
//! act on the [`Friend`](crate::friend::Friend) it carries.

use super::math::{native, read_sanitized};
use crate::friend::{self, flag};
use crate::script::{Fault, Memory, Slot};
use crate::sysobj::class_flag;
use crate::world::{Ctx, ObjId, PLAYER_OBJECT, TrapFn};

type Args = [Slot; 16];

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    if table == 6 && index == 36 {
        return Some(obj_detach_camera);
    }
    if table == 6 && index == 40 {
        return Some(obj_limit_peterpan_idle);
    }
    if table == 6 && index == 26 {
        return Some(obj_limit_hover);
    }
    if table == 7 {
        return Some(match index {
            19 => update_target,
            24 => enable_system_wishdir,
            25 => disable_system_wishdir,
            26 => call,
            27 => limit_start_command,
            21 => obj_limit_hover_set_spec,
            _ => return None,
        });
    }
    if table != 10 {
        return None;
    }
    Some(match index {
        1 => stitch_set_screen_position,
        2 => stitch_get_screen_position,
        12 => stitch_effect_start,
        13 => stitch_shot_effect,
        16 => stitch_effect_kill,
        32 => stitch_move_request,
        33 => get_player_attacker,
        34 => remove_player_attacker,
        3 => start_limit,
        4 => end_limit,
        5 => chickenlittle_get_shoot_target,
        8 => target_last_position,
        9 => genie_change_form,
        10 => genie_get_limit_command,
        18 => is_boss,
        20 => follow_player,
        21 => follow_enemy,
        51 => chickenlittle_get_nearest_target,
        23 => enemy_is_attacked_from,
        25 => peterpan_receive_notify,
        26 => peterpan_accept_notify,
        27 => enable_inertia,
        29 => friend_use_item,
        28 => disable_inertia,
        30 => is_finish_blow,
        36 => is_control,
        37 => is_moveonly,
        39 => action_clear,
        41 => start_leave,
        42 => is_start_leave,
        44 => is_tornado,
        46 => disable_enemy_follow,
        47 => enable_enemy_follow,
        48 => disable_player_follow,
        49 => enable_player_follow,
        53 => add_watch_effect,
        54 => is_effect_exist,
        59 => chickenlittle_set_shoot_target,
        _ => return None,
    })
}

/// The friend that owns the script's AI object (arg 0).
fn me(ctx: &mut Ctx<'_>, m: &Memory<'_>, ai: u32) -> Result<ObjId, Fault> {
    ctx.sys_of_ai(m, ai)
}

fn with_friend<R>(
    ctx: &mut Ctx<'_>,
    id: ObjId,
    f: impl FnOnce(&mut friend::Friend) -> R,
) -> Result<Option<R>, Fault> {
    Ok(ctx.object(id)?.friend.as_deref_mut().map(f))
}

fn int(args: &mut Args, v: i32) {
    args[0] = Slot::int(v);
}

fn set_flag(
    ctx: &mut Ctx<'_>,
    m: &Memory<'_>,
    args: &Args,
    bit: u32,
    on: bool,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    with_friend(ctx, id, |f| {
        if on {
            f.flags |= bit;
        } else {
            f.flags &= !bit;
        }
    })?;
    Ok(())
}

fn vector(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, index: u16, v: [f32; 3], args: &mut Args) {
    let cell = ctx.world.static_cell(10, index, 0);
    for (k, lane) in native([v[0], v[1], v[2], 1.]).iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
}

fn with_stitch<R>(
    ctx: &mut Ctx<'_>,
    id: ObjId,
    f: impl FnOnce(&mut friend::stitch::Stitch) -> R,
) -> Result<Option<R>, Fault> {
    Ok(ctx
        .object(id)?
        .friend
        .as_deref_mut()
        .and_then(|fr| fr.stitch.as_deref_mut())
        .map(f))
}

/// 10:1 FUN_1405363a0 -> FUN_1401c9620: Stitch's screen position (x, y,
/// depth, heading; NaN lanes 0).
fn stitch_set_screen_position(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = read_sanitized(ctx, m, args[1].value)?;
    with_stitch(ctx, id, |s| s.set_screen(v))?;
    Ok(())
}

/// 10:2 FUN_140536420 -> FUN_1401c7f30: a copy of the screen position in
/// the trap's own cell (DAT_140787500).
fn stitch_get_screen_position(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = with_stitch(ctx, id, |s| s.screen)?.unwrap_or_default();
    let cell = ctx.world.static_cell(10, 2, 0);
    for (k, lane) in v.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 10:12 FUN_140536680 -> FUN_1401c9c10: an effect slot (effect, bound
/// object's AI or 0, bone, motion, stay, start flags, argument); returns
/// the slot.
fn stitch_effect_start(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let on = if args[2].value == 0 {
        None
    } else {
        match ctx.sys_ref_ai(m, args[2].value) {
            Ok(crate::world::SysRef::Player) => Some(PLAYER_OBJECT),
            Ok(crate::world::SysRef::Object(o)) => Some(o),
            Err(_) => None,
        }
    };
    let (effect, bone, link) = (args[1].as_int(), args[3].as_int(), args[4].as_int());
    let stay = args[5].value != 0;
    let start = [args[6].as_int(), args[7].as_int()];
    let k = with_stitch(ctx, id, |s| {
        s.effect(effect, on, id, bone, link, stay, start)
    })?;
    int(args, k.map_or(0, |k| k as i32));
    Ok(())
}

/// 10:13 FUN_140536720 -> FUN_1401c9920: [`friend::stitch::shot`].
fn stitch_shot_effect(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let n = args[1].as_int();
    with_object_friend(ctx, id, |world, o, f, env| {
        friend::stitch::shot(world, o, f, env, n)
    })
}

/// 10:16 FUN_1405367e0 -> FUN_1401c80f0: end an effect slot.
fn stitch_effect_kill(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let slot = args[1].as_int();
    if slot >= 0 {
        with_stitch(ctx, id, |s| s.kill(slot as usize))?;
    }
    Ok(())
}

/// 10:32 FUN_140536c70 -> FUN_1401c8cf0: a goal of `kind`
/// (FUN_1401c75e0), motion 0xa1 over 4 frames, and the screen move as the
/// current action, continuing.
fn stitch_move_request(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let kind = args[1].as_int();
    let env = ctx.env;
    let o = ctx.object(id)?;
    let Some(fr) = o.friend.as_deref_mut() else {
        return Ok(());
    };
    if let Some(mut s) = fr.stitch.take() {
        s.pick_goal(kind, &mut friend::stitch::FriendRng(fr));
        fr.stitch = Some(s);
    }
    fr.current = Some(friend::stitch::MOVE);
    fr.flags |= flag::CONTINUES;
    o.sys.motion_change(0xa1, 4., env.clips(o.sys.model));
    Ok(())
}

/// 6:36 FUN_14053e870 -> FUN_1403cc3b0: the object leaves camera space
/// (+0x124 bit 6, +0x8c8 bit 12 and the matrix +0x908 clear).
fn obj_detach_camera(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    ctx.object(id)?.sys.camera_space = false;
    Ok(())
}

/// 6:40 FUN_14053e900: physics mode 0x34 (Peter Pan's limit flight) for
/// the AI's object; Peter Pan's limit puts Sora in it.
fn obj_limit_peterpan_idle(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let spec = ctx.env.movement_spec(0x3e);
    ctx.object(id)?.sys.to_limit_flight(spec);
    Ok(())
}

/// 6:26 FUN_14053e1a0 -> FUN_14053d680: the AI's object hovers (physics
/// mode 0x2a) from a velocity of the vector's direction times its w (NaN
/// lanes read 0); the two numbers are the spec's fast and slow inertia
/// (DAT_142b59480 / 84).
fn obj_limit_hover(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let v = native(read_sanitized(ctx, m, args[1].value)?);
    ctx.world.hover_spec[4] = args[2].as_float();
    ctx.world.hover_spec[5] = args[3].as_float();
    let spec = ctx.world.hover_spec;
    ctx.object(id)?.sys.to_limit_hover(v, spec);
    Ok(())
}

/// 7:21 FUN_14053f880 -> FUN_14053d660: the hover spec's nine words
/// (DAT_142b59470..+0x24) from the script's block.
fn obj_limit_hover_set_spec(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let at = args[0].value;
    for k in 0..9u32 {
        let a = at + 4 * k;
        ctx.world.hover_spec[k as usize] = ctx.read_f32(m, a).ok_or(Fault::Memory(a))?;
    }
    Ok(())
}

/// 10:29 FUN_140536b50 -> FUN_1401b7d90: the friend uses command `index`
/// (FUN_1403c4200) on the object of an AI (its target record takes it).
/// The command's effect is the host's (`friend_use_command`).
fn friend_use_item(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let target = match ctx.sys_ref_ai(m, args[2].value) {
        Ok(crate::world::SysRef::Player) => Some(PLAYER_OBJECT),
        Ok(crate::world::SysRef::Object(o)) => Some(o),
        Err(_) => None,
    };
    let Some(target) = target else {
        return Ok(());
    };
    with_friend(ctx, id, |f| f.band = 0)?;
    ctx.world.events.push(crate::world::Event::Service {
        name: "friend_use_command",
        object: Some(id),
        other: Some(target),
        words: [args[1].value, args[3].value, 0, 0, 0, 0, 0, 0],
        vector: None,
    });
    Ok(())
}

/// 10:3 FUN_140536480: the personality's start-limit (vtable +0x48). The
/// summons outside Stitch and Chicken Little take FUN_1401cd1f0: the
/// current action ends. Chicken Little's FUN_1401cb6c0 also enters his
/// limit mode (+0x28 bit 0), whose deck the host shows
/// ([`friend::CHICKEN_LITTLE_DECK`]): Sora's +0x120 bit 8 and +0x124 bit
/// 14 and his own +0x120 bit 8 are set, he is carried at Sora's origin
/// (FUN_1403d9fe0 with bone 0x4000, flags 4) and Sora enters physics mode
/// 0x3e (FUN_1403b4da0). The fades, the reticle and camera mode 9 are the
/// host's. Stitch's FUN_1401c9d60 is [`friend::stitch::start_limit`].
fn start_limit(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    action_clear(ctx, m, args)?;
    let id = me(ctx, m, args[0].value)?;
    if with_stitch(ctx, id, |_| ())?.is_some() {
        return with_object_friend(ctx, id, |world, o, f, env| {
            friend::stitch::start_limit(world, o, f, env)
        });
    }
    let chicken_little = with_friend(ctx, id, |f| {
        let cl = f.personality == friend::Personality::ChickenLittle;
        if cl {
            f.limit = true;
        }
        cl
    })?;
    if chicken_little == Some(true) {
        let sora = &mut ctx.object(PLAYER_OBJECT)?.sys;
        sora.flags |= 0x100;
        sora.state_flags |= 0x4000;
        sora.to_limit_stand(0x3e);
        let sys = &mut ctx.object(id)?.sys;
        sys.flags |= 0x100;
        sys.to_ground_mode();
        sys.attached = Some(crate::sysobj::Attach {
            to: PLAYER_OBJECT,
            bone: 0x4000,
            flags: 4,
            callback: None,
        });
    }
    Ok(())
}

/// 10:4 FUN_1405364b0: the personality's end-limit (vtable +0x50). Chicken
/// Little's FUN_1401cb180 leaves his limit mode: the bits FUN_1401cb6c0 set
/// clear, he is let go (FUN_1403da250) and Sora goes back to mode 0. A
/// no-op for the summons outside Stitch and Chicken Little. Stitch's
/// FUN_1401c7e00 is [`friend::stitch::end_limit`].
fn end_limit(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    if with_stitch(ctx, id, |_| ())?.is_some() {
        return with_object_friend(ctx, id, |world, _, f, _| {
            friend::stitch::end_limit(world, f)
        });
    }
    let was = with_friend(ctx, id, |f| std::mem::replace(&mut f.limit, false))?;
    if was == Some(true) {
        let sora = &mut ctx.object(PLAYER_OBJECT)?.sys;
        sora.flags &= !0x100;
        sora.state_flags &= !0x4000;
        sora.limit_stand = None;
        let sys = &mut ctx.object(id)?.sys;
        sys.flags &= !0x100;
        sys.attached = None;
        sys.to_ground_mode();
    }
    Ok(())
}

/// 10:5 FUN_1405364e0 -> FUN_1401cb6a0: Chicken Little's aim point.
fn chickenlittle_get_shoot_target(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let aim = with_friend(ctx, id, |f| f.aim)?.unwrap_or_default();
    vector(ctx, m, 5, aim, args);
    Ok(())
}

/// 10:51 FUN_140537060 -> FUN_1401cb360: the record (arg 1) names the
/// enemy TARGET nearest Chicken Little's aim point on the screen, within
/// the half width and height (args 2, 3) of it; empty without one. Points
/// go through the view and are scaled by the projection over their depth.
/// [I] The iterator's own skips (FUN_1401b51a0, FUN_1403bd420,
/// FUN_1403ba4a0) are read as the nearest-target search's.
fn chickenlittle_get_nearest_target(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let (w, h) = (args[2].as_float(), args[3].as_float());
    let aim = with_friend(ctx, id, |f| f.aim)?.unwrap_or_default();
    let screen = |v: [f32; 4]| [v[0] * (v[3] / v[2]), v[1] * (v[3] / v[2])];
    let mut best: Option<(ObjId, u32, f32)> = None;
    if let Some(centre) = ctx.env.camera_view(aim).map(screen) {
        let mut ids: Vec<ObjId> = ctx.world.objects.iter().flatten().map(|o| o.id).collect();
        ids.push(ctx.me.id);
        ids.sort_unstable();
        ids.dedup();
        for o in ids {
            let s = &ctx.object(o)?.sys;
            let alive = !s.gone
                && s.hp > 0
                && s.state_flags & crate::sysobj::state_flag::DEAD_MARKED == 0
                && s.flags & crate::world::SEARCH_SKIP_FLAGS == 0;
            if s.class_flags & class_flag::ENEMY == 0 || !alive {
                continue;
            }
            for (shape, at) in super::searcher::shapes(ctx, o) {
                let Some(p) = ctx.env.camera_view(at).map(screen) else {
                    continue;
                };
                let (dx, dy) = (p[0] - centre[0], p[1] - centre[1]);
                let d = dx * dx + dy * dy;
                if dx.abs() <= w && dy.abs() <= h && best.is_none_or(|b| d < b.2) {
                    best = Some((o, shape, d));
                }
            }
        }
    }
    let t = match best {
        Some((o, shape, _)) => crate::sysobj::Target {
            id: super::target::target_id_of(o),
            kind: -1,
            group: shape as i32,
        },
        None => crate::sysobj::Target {
            id: 0,
            kind: -1,
            group: 0,
        },
    };
    super::target::set_record(ctx, m, args[1].value, t)
}

/// 10:53 FUN_140537110 -> FUN_1401ce1e0: watch an effect (arg 1) for the
/// friend that owns the AI object (arg 0).
fn add_watch_effect(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let effect = args[1].value;
    let env = ctx.env;
    with_friend(ctx, id, |f| f.add_watch(effect, env))?;
    Ok(())
}

/// 10:54 FUN_140537160 -> FUN_1401ce410: the effect is still on the
/// friend's watch list.
fn is_effect_exist(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let effect = args[1].value;
    let on = with_friend(ctx, id, |f| f.watches(effect))?.unwrap_or(false);
    int(args, on as i32);
    Ok(())
}

/// 10:59 FUN_140537320 -> FUN_1401cb6b0: set the aim point (NaN lanes 0).
fn chickenlittle_set_shoot_target(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = read_sanitized(ctx, m, args[1].value)?;
    let v = native(v);
    with_friend(ctx, id, |f| f.aim = [v[0], v[1], v[2]])?;
    Ok(())
}

/// 10:8 FUN_140536590 -> FUN_1401b4d00: the target's last known position.
fn target_last_position(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let p = with_friend(ctx, id, |f| f.target_last)?.unwrap_or_default();
    vector(ctx, m, 8, p, args);
    Ok(())
}

/// Genie's forms (0x140727e80): form command, its limit command. The rows
/// are the form numbers less one (DAT_1409acdd7 = row + 1).
pub const GENIE_FORMS: [(u16, u16); 6] = [
    (0x217, 0x21e),
    (0x218, 0x220),
    (0, 0),
    (0x219, 0x222),
    (0x21a, 0x224),
    (0x25f, 0x260),
];

/// 10:9 FUN_1405365e0 -> FUN_1401cf2d0: Genie takes form `cmd`. The six
/// part calls hide the model (FUN_1403cc9b0 -> FUN_1401a7cf0, which takes
/// no part) and the matching row's shows it again (FUN_1403cd810); the form
/// number is the row's plus one (FUN_1403ddfb0, DAT_1409acdd7, which the
/// next Genie starts in), reported to the host as `genie_form`.
fn genie_change_form(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let cmd = args[1].as_int() as u16;
    with_friend(ctx, id, |f| f.genie_form = cmd)?;
    let row = GENIE_FORMS
        .iter()
        .position(|e| e.0 == cmd)
        .unwrap_or(GENIE_FORMS.len());
    ctx.object(id)?.sys.hidden = row == GENIE_FORMS.len();
    ctx.world.events.push(crate::world::Event::Service {
        name: "genie_form",
        object: Some(id),
        other: None,
        words: [row as u32 + 1, u32::from(cmd), 0, 0, 0, 0, 0, 0],
        vector: None,
    });
    Ok(())
}

/// 10:10 FUN_140536610 -> FUN_1401cf410: the limit command of Genie's form
/// (0 when none).
fn genie_get_limit_command(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let form = with_friend(ctx, id, |f| f.genie_form)?.unwrap_or(0);
    let limit = GENIE_FORMS
        .iter()
        .find(|e| e.0 == form && form != 0)
        .map_or(0, |e| e.1);
    int(args, i32::from(limit));
    Ok(())
}

/// 10:18 FUN_140536850: the object's +0x6c8 bit 10 (a boss).
fn is_boss(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx
        .sys_of(args[0].value)
        .or_else(|_| ctx.sys_of_ai(m, args[0].value))?;
    let v = ctx.object(id)?.sys.class_flags & class_flag::PRIORITY_TARGET != 0;
    int(args, v as i32);
    Ok(())
}

/// 10:20 FUN_1405368d0: follow Sora now (FUN_1401b4820).
fn follow_player(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let env = ctx.env;
    friend::follow_now(ctx.object(id)?, env, false);
    Ok(())
}

/// 10:21: follow the target now (FUN_1401b4190).
fn follow_enemy(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let env = ctx.env;
    friend::follow_now(ctx.object(id)?, env, true);
    Ok(())
}

/// 10:23 FUN_1405369e0: the enemy (arg 0) is attacking the object of arg
/// 1 (its +0xd38). [I] Only Sora's attackers are known here.
fn enemy_is_attacked_from(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let enemy = me(ctx, m, args[0].value)?;
    let other = ctx.sys_of_ai(m, args[1].value).ok();
    let v = other == Some(PLAYER_OBJECT) && ctx.world.player_attackers.contains(&enemy);
    int(args, v as i32);
    Ok(())
}

/// 10:25 FUN_140536a90 -> FUN_1401cbdf0: Sora's attack notification is
/// fresh (within 8 frames) and not yet taken.
fn peterpan_receive_notify(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = receivable(ctx, id)?;
    int(args, v as i32);
    Ok(())
}

/// FUN_1401cbdf0: the notification is fresh and this friend's bit 9 clear.
fn receivable(ctx: &mut Ctx<'_>, id: ObjId) -> Result<bool, Fault> {
    let fresh = ctx.world.player_notify.fresh();
    let taken = with_friend(ctx, id, |f| f.flags & flag::NOTIFY_TAKEN != 0)?.unwrap_or(true);
    Ok(fresh && !taken)
}

/// 10:26 FUN_140536ad0: when FUN_1401cbdf0 holds, FUN_1401cbd60 takes the
/// notification (bit 9).
fn peterpan_accept_notify(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    if receivable(ctx, id)? {
        with_friend(ctx, id, |f| f.flags |= flag::NOTIFY_TAKEN)?;
    }
    Ok(())
}

/// 10:27 FUN_140536b10 -> FUN_1401b3dd0: inertia on (bit 14 clear).
/// [I] Flight mode 0x20 / 0x1f is not a separate state here.
fn enable_inertia(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::INERTIA_OFF, false)
}

/// 10:28 FUN_140536b30 -> FUN_1401b3920: inertia off (bit 14).
fn disable_inertia(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::INERTIA_OFF, true)
}

/// 10:30 FUN_140536bb0: the object is blown (mode flag bit 6) by a finishing
/// blow (+0x124 bit 17).
fn is_finish_blow(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx
        .sys_of(args[0].value)
        .or_else(|_| ctx.sys_of_ai(m, args[0].value))?;
    let s = &ctx.object(id)?.sys;
    let v = s.mode_flags() & 1 << 6 != 0 && s.state_flags & 1 << 17 != 0;
    int(args, v as i32);
    Ok(())
}

/// 10:36 FUN_140536d70: (+0x18c & 0x1c) != 0.
fn is_control(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = ctx.object(id)?.sys.motion.flags.word & 0x1c != 0;
    int(args, v as i32);
    Ok(())
}

/// 10:37 FUN_140536db0: +0x18c bit 30.
fn is_moveonly(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = ctx.object(id)?.sys.motion.flags.word & 1 << 30 != 0;
    int(args, v as i32);
    Ok(())
}

/// 10:39 FUN_140536e20 -> FUN_1401b7f10(F, 0, 0): no current action.
fn action_clear(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    with_friend(ctx, id, |f| f.current = None)?;
    Ok(())
}

/// 10:41 FUN_140536eb0 -> FUN_1401b8240: bit 30.
fn start_leave(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::LEAVE, true)
}

/// 10:42 FUN_140536ed0 -> FUN_1401b5120.
fn is_start_leave(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = with_friend(ctx, id, |f| f.flags & flag::LEAVE != 0)?.unwrap_or(false);
    int(args, v as i32);
    Ok(())
}

/// 10:44 FUN_140536f40: the physics mode's flag bit 4.
fn is_tornado(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let v = ctx.object(id)?.sys.mode_flags() & 1 << 4 != 0;
    int(args, v as i32);
    Ok(())
}

/// 7:19 FUN_14053f830 -> FUN_1401b8910(F, arg != 0): whether the target
/// changed.
fn update_target(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let force = args[1].value != 0;
    let env = ctx.env;
    // The object is taken out of the world while its friend senses it.
    let changed = if id == ctx.me.id {
        friend::sense_now(ctx.world, ctx.me, env, force)
    } else {
        let mut o = ctx
            .world
            .objects
            .get_mut(id as usize)
            .and_then(Option::take)
            .ok_or(Fault::Memory(args[0].value))?;
        let c = friend::sense_now(ctx.world, &mut o, env, force);
        ctx.world.objects[id as usize] = Some(o);
        c
    };
    int(args, changed as i32);
    Ok(())
}

/// 7:24 FUN_14053f9c0 -> FUN_1401b3e10 / 7:25 FUN_14053f9e0 ->
/// FUN_1401b3960: bit 10 clear / set.
fn enable_system_wishdir(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::NO_SYSTEM_WISH, false)
}

fn disable_system_wishdir(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::NO_SYSTEM_WISH, true)
}

/// 7:26 FUN_14053fa00 -> FUN_1401b2870: the friend's callback (+0xfa0)
/// with the command; only Stitch's limit sets one (FUN_1401c7bf0, which
/// returns 0).
fn call(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = me(ctx, m, args[0].value)?;
    let cmd = args[1].as_int();
    let callback = ctx.object(id)?.friend.as_deref().and_then(|f| f.call);
    if callback == Some(friend::Call::StitchCommand) {
        with_object_friend(ctx, id, |world, o, f, env| {
            friend::stitch::command(world, o, f, env, cmd)
        })?;
    }
    int(args, 0);
    Ok(())
}

/// Run `f` with the friend object `id` and its personality apart from the
/// world (the trap's own object, or one of the list).
fn with_object_friend(
    ctx: &mut Ctx<'_>,
    id: ObjId,
    f: impl FnOnce(
        &mut crate::world::World,
        &mut crate::world::Object,
        &mut friend::Friend,
        &dyn crate::env::Env,
    ),
) -> Result<(), Fault> {
    let env = ctx.env;
    if id == ctx.me.id {
        let Some(mut fr) = ctx.me.friend.take() else {
            return Ok(());
        };
        f(ctx.world, ctx.me, &mut fr, env);
        ctx.me.friend = Some(fr);
        return Ok(());
    }
    let mut o = ctx
        .world
        .objects
        .get_mut(id as usize)
        .and_then(Option::take)
        .ok_or(Fault::Memory(id))?;
    if let Some(mut fr) = o.friend.take() {
        f(ctx.world, &mut o, &mut fr, env);
        o.friend = Some(fr);
    }
    ctx.world.objects[id as usize] = Some(o);
    Ok(())
}

/// 7:27 FUN_14053fa40 -> FUN_1403d89f0: the running limit's command: the
/// limit object's own when the script runs in one (a summon's limit),
/// else the host's.
fn limit_start_command(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let command = ctx
        .world
        .limit
        .as_ref()
        .map_or_else(|| ctx.env.limit_command(), |l| l.command);
    int(args, i32::from(command));
    Ok(())
}

/// 10:33 FUN_140536ca0: Sora's newest attacker (0 when none); it and all
/// older ones leave the list (FUN_1401b4b90(-1), FUN_1401b6260(o, 1)).
fn get_player_attacker(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    match ctx.world.player_attackers.pop() {
        Some(o) => {
            ctx.world.player_attackers.clear();
            args[0] = Slot::address(crate::world::object_handle(o));
        }
        None => args[0] = Slot::address(0),
    }
    Ok(())
}

/// 10:34 FUN_140536cf0 -> FUN_1401b6260(obj, all): the object leaves the
/// list, with every older entry when `all`.
fn remove_player_attacker(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let Ok(o) = ctx.sys_of(args[0].value) else {
        return Ok(());
    };
    let list = &mut ctx.world.player_attackers;
    if let Some(i) = list.iter().position(|&x| x == o) {
        if args[1].value != 0 {
            list.drain(..=i);
        } else {
            list.remove(i);
        }
    }
    Ok(())
}

/// 10:46 / 10:47: never / again follow enemies (bit 12).
fn disable_enemy_follow(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::NO_ENEMY_FOLLOW, true)
}

fn enable_enemy_follow(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::NO_ENEMY_FOLLOW, false)
}

/// 10:48 FUN_1401b3910 / 10:49 FUN_1401b3dc0: never / again follow Sora
/// (bit 17).
fn disable_player_follow(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::NO_PLAYER_FOLLOW, true)
}

fn enable_player_follow(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    set_flag(ctx, m, args, flag::NO_PLAYER_FOLLOW, false)
}

#[cfg(test)]
mod tests;
