//! Table 5, the route and routine traps, with `obj_set_path` (1:172, 1:177).
//! The path follower is [`crate::route`]; the routine action state is
//! [`crate::rtn`]. Addresses are the traps' (FUN_1405576xx..FUN_140558xxx).
//!
//! Vectors cross the script boundary as the executable holds them (+Y
//! down), so a position taken from an object is flipped on the way in and
//! results are handed back as they are.

use super::math::{native, native_yaw};
use crate::route::{self, Build, PathKey, V4};
use crate::script::{Fault, Memory, Slot};
use crate::sysobj::class_flag;
use crate::world::{Ctx, Event, ObjId, TrapFn};

type Args = [Slot; 16];

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    Some(match (table, index) {
        (1, 172) => can_look,
        (1, 177) => obj_set_path,
        (5, 0) => get_start_rtn_action,
        (5, 1) => set_path_way,
        (5, 2) => reverse_path_way,
        (5, 3) => get_path_dir,
        (5, 4) => end_rtn_action,
        (5, 5) => get_rtn_action,
        (5, 6) => get_rtn_action_dir,
        (5, 7) => is_rtn_change_dir,
        (5, 8) => create_active_path,
        (5, 9) => get_path_dir_from_obj,
        (5, 10) => forward_path_current_pointer,
        (5, 11) => is_end_rtn_action,
        (5, 12) => reset_active_path,
        (5, 13) => set_path_target_point,
        (5, 14) => get_path_point_pos,
        (5, 15) => clear_active_path,
        (5, 16) => reset_leave_way,
        (5, 18) => reset_path_current_pointer,
        (5, 19) => get_path_current_pos,
        (5, 20) => get_path_current_dir,
        (5, 21) => get_path_first_point_pos,
        (5, 22) => get_path_last_point_pos,
        (5, 23) => set_path_by_id,
        (5, 24) => set_path_by_group,
        (5, 25) => get_path_dir_r,
        (5, 26) => set_rtn_partner,
        (5, 28) => eh22_path_move_next,
        (5, 29) => eh22_path_move_before,
        (5, 30) => eh22_path_is_moving,
        (5, 31) => eh22_path_get_point,
        (5, 32) => eh22_path_play,
        (5, 34) => get_obj_head_pos,
        _ => return None,
    })
}

/// FUN_1403b5b40: the object's position as the executable holds it.
fn position(ctx: &mut Ctx<'_>, id: ObjId) -> Result<V4, Fault> {
    let p = ctx.object(id)?.sys.position;
    Ok(native([p[0], p[1], p[2], 1.0]))
}

/// FUN_1403b56e0: the facing vector, w the heading.
fn facing(ctx: &mut Ctx<'_>, id: ObjId) -> Result<V4, Fault> {
    let sys = &ctx.object(id)?.sys;
    let f = native([sys.forward()[0], 0.0, sys.forward()[2], 0.0]);
    Ok([f[0], 0.0, f[2], native_yaw(sys.yaw)])
}

/// The shared result vector of the route traps (DAT_140789ee8), handed to the
/// script by address.
fn return_vector(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args, v: V4) {
    let cell = ctx.world.static_cell(5, 3, 0);
    for (k, lane) in v.iter().enumerate() {
        ctx.write_f32(m, cell + 4 * k as u32, *lane);
    }
    args[0] = Slot::address(cell);
}

fn has_route(ctx: &mut Ctx<'_>, id: ObjId) -> Result<bool, Fault> {
    Ok(ctx.object(id)?.sys.route.is_some())
}

fn int(args: &mut Args, v: i32) {
    args[0] = Slot::int(v);
}

// --- Setting a path ----------------------------------------------------------

/// FUN_1403b9de0 / FUN_1403b9eb0: drop the object's route and build one from
/// a walk path of its unit (or `unit`), falling back to the area's paths for
/// the objects that may use them.
fn install_route(
    ctx: &mut Ctx<'_>,
    id: ObjId,
    unit: Option<u32>,
    key: PathKey,
) -> Result<bool, Fault> {
    let sys = &mut ctx.object(id)?.sys;
    sys.route = None;
    let unit = unit.or(sys.unit);
    let class = sys.class_flags;
    let mut found = unit.and_then(|u| ctx.env.walk_path(Some(u), key));
    if found.is_none() && class & class_flag::AREA_PATHS != 0 {
        found = ctx.env.walk_path(None, key);
    }
    let Some(path) = found else {
        return Ok(false);
    };
    let at = position(ctx, id)?;
    let sys = &ctx.object(id)?.sys;
    let routine = class & class_flag::ROUTINE != 0;
    let build = Build {
        routine,
        flatten: routine && sys.rtn_flag(3),
        start_at_object: sys.rtn_flag(1),
        one_way: sys.rtn_flag(2),
    };
    let world = &mut *ctx.world;
    let (route, place) = route::Route::from_path(&path, build, &at, &mut || world.crt_rand());
    let sys = &mut ctx.object(id)?.sys;
    if let Some(p) = place {
        let p = native([p[0], p[1], p[2], 1.0]);
        sys.position = [p[0], p[1], p[2]];
    }
    sys.route = Some(Box::new(route));
    Ok(true)
}

/// 1:177 FUN_14042d8e0 -> FUN_1403b9de0: follow the unit's walk path `id`.
fn obj_set_path(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let ok = install_route(ctx, id, None, PathKey::Id(args[1].as_int()))?;
    int(args, ok as i32);
    Ok(())
}

/// 5:23 FUN_140558040 -> FUN_1403b9eb0: follow the walk path of group `n`.
fn set_path_by_id(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let ok = install_route(ctx, id, None, PathKey::Group(args[1].value as u8))?;
    int(args, ok as i32);
    Ok(())
}

/// 5:24 FUN_140558080: follow the first walk path of the spawn point with
/// this id (the id without bit 16).
fn set_path_by_group(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let unit = ctx.env.walk_unit(args[1].value & 0xfffe_ffff);
    let mut ok = false;
    if let Some(unit) = unit {
        let id = ctx.sys_of_ai(m, args[0].value)?;
        ok = install_route(ctx, id, Some(unit), PathKey::Id(-1))?;
    }
    int(args, ok as i32);
    Ok(())
}

// --- Walking the route -------------------------------------------------------

/// 5:1 FUN_1405577e0 -> FUN_140551850: set the walking direction.
fn set_path_way(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let forward = args[1].as_int() != 0;
    if let Some(route) = ctx.object(id)?.sys.route.as_mut() {
        route.set_forward(forward);
    }
    Ok(())
}

/// 5:2 FUN_140557860 -> FUN_1405510a0: turn the walker round; the answer is
/// the direction, 0 without a route.
fn reverse_path_way(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let forward = ctx
        .object(id)?
        .sys
        .route
        .as_mut()
        .is_some_and(|r| r.reverse());
    int(args, forward as i32);
    Ok(())
}

fn path_direction(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
    ahead: f32,
    blend: f32,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    if !has_route(ctx, id)? {
        return Ok(());
    }
    let (at, face) = (position(ctx, id)?, facing(ctx, id)?);
    let route = ctx.object(id)?.sys.route.as_mut().expect("checked");
    let v = route.follow_direction(&at, &face, ahead, blend);
    return_vector(ctx, m, args, v);
    Ok(())
}

/// 5:3 FUN_140557650: the direction to follow the path, looking `ahead` on.
fn get_path_dir(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let ahead = args[1].as_float();
    path_direction(ctx, m, args, ahead, 1.0)
}

/// 5:25 FUN_140557710: the same with the weight of the two directions.
fn get_path_dir_r(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let (ahead, blend) = (args[1].as_float(), args[2].as_float());
    path_direction(ctx, m, args, ahead, blend)
}

/// 5:9 FUN_140557aa0: the direction another object should take to follow an
/// active path.
fn get_path_dir_from_obj(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let other = ctx.sys_of_ai(m, args[1].value)?;
    let ahead = args[2].as_float();
    if !ctx.object(id)?.sys.route.as_ref().is_some_and(|r| r.active) {
        return Ok(());
    }
    let (at, face) = (position(ctx, other)?, facing(ctx, other)?);
    let route = ctx.object(id)?.sys.route.as_mut().expect("checked");
    let v = route.follow_direction(&at, &face, ahead, 1.0);
    return_vector(ctx, m, args, v);
    Ok(())
}

/// 5:8 FUN_140557a50 -> FUN_1403b8110: make the object follow another along
/// an active path, or re-aim the one it has.
fn create_active_path(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let target = ctx.sys_of_ai(m, args[1].value)?;
    let (own, there) = (position(ctx, id)?, position(ctx, target)?);
    let sys = &mut ctx.object(id)?.sys;
    match sys.route.as_mut() {
        Some(route) => route.set_target(Some(target), &own, &there),
        None => sys.route = Some(Box::new(route::Route::active(target, &own, &there))),
    }
    Ok(())
}

/// 5:10 FUN_140557ba0 -> FUN_14054f4a0: move the walker on by a length.
fn forward_path_current_pointer(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let length = args[1].as_float();
    if let Some(route) = ctx.object(id)?.sys.route.as_mut() {
        route.advance_by_length(length);
    }
    Ok(())
}

/// 5:12 FUN_140557d30 -> FUN_140550c30: restart an active path towards the
/// target it had.
fn reset_active_path(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let Some(target) = ctx
        .object(id)?
        .sys
        .route
        .as_ref()
        .filter(|r| r.active)
        .and_then(|r| r.target)
    else {
        return Ok(());
    };
    let (own, there) = (
        position(ctx, id)?,
        position(ctx, target).unwrap_or([0.0; 4]),
    );
    if let Some(route) = ctx.object(id)?.sys.route.as_mut() {
        route.set_target(None, &own, &there);
    }
    Ok(())
}

/// 5:13 FUN_140557d80 -> FUN_140551840.
fn set_path_target_point(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let index = args[1].as_int();
    if let Some(route) = ctx.object(id)?.sys.route.as_mut() {
        route.stop_index = index;
    }
    Ok(())
}

/// 5:14 FUN_140557dc0: node `n`'s position.
fn get_path_point_pos(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let index = args[1].as_int();
    if let Some(route) = ctx.object(id)?.sys.route.as_ref() {
        let v = route.point(index);
        return_vector(ctx, m, args, v);
    }
    Ok(())
}

/// 5:15 FUN_140557e30 -> FUN_14054f3a0.
fn clear_active_path(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    if let Some(route) = ctx.object(id)?.sys.route.as_mut() {
        route.clear();
    }
    Ok(())
}

/// 5:16 FUN_140557e70 -> FUN_140550e90: walk away from the other object.
fn reset_leave_way(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let other = ctx.sys_of_ai(m, args[1].value)?;
    let (own, there) = (position(ctx, id)?, position(ctx, other)?);
    if let Some(route) = ctx.object(id)?.sys.route.as_mut() {
        route.leave_way(&own, &there);
    }
    Ok(())
}

/// 5:18 FUN_140557be0 -> FUN_140550d80: put the walker at the place nearest
/// the object.
fn reset_path_current_pointer(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let own = position(ctx, id)?;
    if let Some(route) = ctx.object(id)?.sys.route.as_mut() {
        route.reset_to(&own);
    }
    Ok(())
}

fn route_vector(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
    read: impl Fn(&route::Route) -> V4,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    if let Some(route) = ctx.object(id)?.sys.route.as_ref() {
        let v = read(route);
        return_vector(ctx, m, args, v);
    }
    Ok(())
}

/// 5:19 FUN_140557c30 -> FUN_14054f720.
fn get_path_current_pos(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    route_vector(ctx, m, args, |r| r.position())
}

/// 5:20 FUN_140557c90 -> FUN_14054f6c0.
fn get_path_current_dir(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    route_vector(ctx, m, args, |r| r.direction())
}

/// 5:21 FUN_140557f80 -> FUN_14054f470.
fn get_path_first_point_pos(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    route_vector(ctx, m, args, |r| r.first_point())
}

/// 5:22 FUN_140557fe0 -> FUN_140550c00.
fn get_path_last_point_pos(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    route_vector(ctx, m, args, |r| r.last_point())
}

// --- Routine actions ---------------------------------------------------------

/// 5:0 FUN_140557610 -> FUN_1404181d0: the action is due: forced, or the
/// walker has passed the point of its zone.
fn get_start_rtn_action(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let at = ctx.object(id)?.sys.position;
    let action = ctx.object(id)?.sys.rtn.action.clone();
    let mut due = false;
    if !action.started {
        if action.forced {
            due = true;
        } else if let Some(zone) = action.zone {
            let progress = ctx.env.zone_progress(zone, at);
            let past = if 0.5 <= action.sign {
                action.threshold < progress
            } else {
                progress < action.threshold
            };
            due = !past;
        }
    }
    if due {
        ctx.object(id)?.sys.rtn.action.started = true;
    }
    int(args, due as i32);
    Ok(())
}

/// 5:4 FUN_140557900 -> FUN_140417e60: end the action.
fn end_rtn_action(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let sys = &mut ctx.object(id)?.sys;
    let times = sys.rtn_time;
    sys.rtn.end(&times);
    Ok(())
}

/// 5:5 FUN_140557920: the action block's first three words.
fn get_rtn_action(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let a = &ctx.object(id)?.sys.rtn.action;
    let words = [a.kind as u32, a.dir_kind as u32, a.word];
    let cell = ctx.world.static_cell(5, 5, 0);
    for (k, w) in words.iter().enumerate() {
        ctx.write_u32(m, cell + 4 * k as u32, *w);
    }
    args[0] = Slot::address(cell);
    Ok(())
}

/// 5:6 FUN_140557970 -> FUN_140418070: the direction the action turns the
/// object to (its own facing when the action names none).
fn get_rtn_action_dir(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let mut dir = facing(ctx, id)?;
    let own = position(ctx, id)?;
    let action = ctx.object(id)?.sys.rtn.action.clone();
    if (action.forced || action.zone.is_some()) && action.dir_kind != 0 {
        let flat = |p: V4| [p[0], 0.0, p[2], p[3]];
        let own = flat(own);
        let alive = |ctx: &mut Ctx<'_>, o: ObjId| ctx.object(o).is_ok_and(|o| !o.sys.gone);
        if action.kind == 3 {
            if let Some(o) = action.partner
                && alive(ctx, o)
            {
                dir = action.look;
            }
        } else {
            let target = match action.dir_kind {
                1 => match action.partner {
                    Some(o) if alive(ctx, o) => Some(flat(position(ctx, o)?)),
                    _ => None,
                },
                2 => Some(flat(position(ctx, crate::world::PLAYER_OBJECT)?)),
                3 => Some(action.point),
                _ => None,
            };
            if let Some(target) = target {
                dir = route::sub(&target, &own);
                route::normalize(&mut dir);
            }
        }
    }
    return_vector(ctx, m, args, dir);
    Ok(())
}

/// 5:7 FUN_1405579c0 -> FUN_1404183b0.
fn is_rtn_change_dir(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let v = ctx.object(id)?.sys.rtn.action.changed_dir;
    int(args, v as i32);
    Ok(())
}

/// 5:11 FUN_140557cf0 -> FUN_140418310: the action is over.
fn is_end_rtn_action(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let action = ctx.object(id)?.sys.rtn.action.clone();
    let mut over = true;
    if action.started && 0.0 < action.timer {
        if action.kind == 4 && !ctx.env.player_in_reaction(id) {
            over = true;
        } else if action.kind != 5 {
            over = false;
        } else {
            // A meeting ends when the partner has left it: it no longer
            // answers with this object (or has begun motion 0x32).
            over = match action.partner {
                Some(p) if ctx.object(p).is_ok_and(|o| !o.sys.gone) => {
                    let partner = ctx.object(p)?;
                    !(partner.sys.rtn.action.partner == Some(id) && partner.sys.motion.slot != 0x32)
                }
                _ => true,
            };
        }
    }
    int(args, over as i32);
    Ok(())
}

/// 5:26 FUN_1405580e0: name the object this one's routine meets.
fn set_rtn_partner(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let other = ctx.sys_of_ai(m, args[1].value)?;
    ctx.object(id)?.sys.rtn.partner = Some(other);
    Ok(())
}

/// 5:34 FUN_140557a00 -> FUN_1403b58c0: where the object's head is.
fn get_obj_head_pos(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let own = position(ctx, id)?;
    let v = match ctx.env.head_position(id) {
        Some(p) => native([p[0], p[1], p[2], 1.0]),
        None => own,
    };
    return_vector(ctx, m, args, v);
    Ok(())
}

/// 1:172 FUN_14042d6c0 -> FUN_1403e4bc0: the other object's head lies within
/// the angle the object can look through (15 degrees without a LOOK shape).
fn can_look(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let other = ctx.sys_of_ai(m, args[1].value)?;
    let head = match ctx.env.head_position(other) {
        Some(p) => native([p[0], p[1], p[2], 1.0]),
        None => position(ctx, other)?,
    };
    let (origin, limit, inclusive) = match ctx.env.look_cone(id) {
        Some((centre, limit)) => (native([centre[0], centre[1], centre[2], 1.0]), limit, true),
        None => (position(ctx, id)?, 0.261_799_4, false),
    };
    let mut ahead = route::sub(&head, &origin);
    ahead[1] = 0.0;
    ahead[3] = 0.0;
    route::normalize(&mut ahead);
    let face = facing(ctx, id)?;
    let dot = ahead[1] * face[1] + ahead[0] * face[0] + ahead[2] * face[2];
    let angle = if 1.0 <= dot {
        0.0
    } else if dot <= -1.0 {
        std::f32::consts::PI
    } else {
        dot.acos()
    };
    let ok = if inclusive {
        angle.abs() <= limit
    } else {
        angle.abs() < limit
    };
    int(args, ok as i32);
    Ok(())
}

// --- The event path ------------------------------------------------------------

/// 5:28 FUN_140553fd0: move to the next point of the event path.
fn eh22_path_move_next(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, _: &mut Args) -> Result<(), Fault> {
    let points = ctx.env.event_path();
    let e = &mut ctx.world.eh22;
    // Without an event path nothing can arrive, so nothing starts.
    if e.moving == 0 && !points.is_empty() {
        let mut next = e.next;
        if (points.len() as i32) < next {
            next = 0;
        }
        e.current = next;
        let value = usize::try_from(next)
            .ok()
            .and_then(|i| points.get(i))
            .map_or(0, |p| p.0);
        e.moving = 1;
        e.next = e.current + 1;
        e.value = value as f32;
    }
    Ok(())
}

/// 5:29 FUN_140553f20: move back to the point before.
fn eh22_path_move_before(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, _: &mut Args) -> Result<(), Fault> {
    let points = ctx.env.event_path();
    let e = &mut ctx.world.eh22;
    // Without an event path nothing can arrive, so nothing starts.
    if e.moving == 0 && !points.is_empty() {
        let mut before = e.next - 1;
        if before < 0 {
            before = points.len() as i32 - 1;
        }
        e.current = before;
        let value = usize::try_from(before)
            .ok()
            .and_then(|i| points.get(i))
            .map_or(0, |p| p.1);
        e.moving = 2;
        e.next = e.current;
        e.value = value as f32;
    }
    Ok(())
}

/// 5:30 FUN_140558150 -> FUN_140553ef0.
fn eh22_path_is_moving(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    int(args, (ctx.world.eh22.moving != 0) as i32);
    Ok(())
}

/// 5:31 FUN_140558170 -> FUN_140553ec0: the point the walker goes to next.
fn eh22_path_get_point(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    int(args, ctx.world.eh22.next);
    Ok(())
}

/// 5:32 FUN_140558190: what the event path should play.
fn eh22_path_play(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let e = &mut ctx.world.eh22;
    e.play_value = args[1].value;
    e.play = args[2].as_int() != 0;
    ctx.world.events.push(Event::Service {
        name: "eh22_path_play",
        object: None,
        other: None,
        words: [args[1].value, args[2].value, 0, 0, 0, 0, 0, 0],
        vector: None,
    });
    Ok(())
}

#[cfg(test)]
mod tests;
