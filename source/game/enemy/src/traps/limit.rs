//! The limit object as party-limit scenes see it (the 0x128-byte object
//! FUN_1403d7fa0 builds, DAT_142a24cc0) and the limit trap group of table 2
//! (2:47 to 2:63, 2:89) with 1:263. The host builds [`LimitState`] when a
//! limit starts (FUN_1403d8b40) and carries out what the traps ask that it
//! owns: the limit file's motions, effects and cameras, the threads the
//! scene starts and the reaction commands.

use crate::script::{Fault, Memory, Slot};
use crate::sysobj::Fade;
use crate::world::{Ctx, Event, Light, ObjId, PLAYER_HANDLE, PLAYER_OBJECT, TrapFn, object_handle};

type Args = [Slot; 16];

/// Motion slots of the limit file's motions: slot `MOTIONS + id` plays the
/// participant's motion `id` of the limit's own set (FUN_1403c63e0).
pub const MOTIONS: usize = 0x4000;
/// The object an effect of the limit file's own container is started on
/// (`Event::EffectStart::object`); `other` names the object it is bound to.
pub const LIMIT_EFFECTS: ObjId = 0x00ff_fffe;

/// A thread the scene asked for (`limit_start_thread`): its entry and its
/// argument word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadRequest {
    pub entry: i32,
    pub argument: Slot,
}

/// What the scene reads of the limit object.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitState {
    /// The address scripts hold for the object (TR22's argument).
    pub handle: u32,
    /// The limit's command (limt +0x28).
    pub command: u16,
    /// +0x10 / +0x14: the participants other than Sora (characters > 1).
    pub members: [Option<ObjId>; 2],
    /// +0x118: the time left on the gauge.
    pub time: f32,
    /// +0x50: limit motions still playing (and the Trinity command window).
    pub pending: i32,
    /// +0x54 bit 3: the Trinity command window is up (FUN_1403d96d0).
    pub special: bool,
    /// +0x4c: the colour filter (a light, FUN_1403f6f80(0)).
    pub filter: u32,
    /// The special command deck (mode 3, DAT_142ae9ab0) while the window is
    /// up: three (command, extra) rows, Trinity's 0x1e7, 0x1e6, 0x1e5.
    pub combo: [(u16, u16); 3],
    /// +0xc0: hits the limit's combo counter holds (`limit_reset_hit_counter`).
    pub hits: i32,
    /// +0x54 bit 2: the scene thread has ended and the limit is closing.
    pub ending: bool,
    /// DAT_14074b980: the last token a limit motion handed out.
    pub token: i32,
    /// Threads `limit_start_thread` asked for, taken by the host.
    pub threads: Vec<ThreadRequest>,
    /// The reaction command Sora last got to run (`player_set_exec_rc`,
    /// player +0xdd8).
    pub exec_rc: u16,
}

impl LimitState {
    /// The limit object for a run: its address, the command and its members,
    /// with a fresh colour filter (FUN_1403d7fa0's tail).
    pub fn new(
        world: &mut crate::world::World,
        command: u16,
        members: [Option<ObjId>; 2],
        time: f32,
    ) -> Self {
        let handle = world.alloc(0x128);
        let filter = world.add_light(Light {
            kind: 0,
            fade: Fade {
                value: 1.0,
                rate: 0.0,
            },
            flags: vec![2],
            color: [0x80; 4],
            fog: [0.; 4],
        });
        LimitState {
            handle,
            command,
            members,
            time,
            pending: 0,
            special: false,
            combo: [(0, 0); 3],
            hits: 0,
            filter,
            ending: false,
            token: 0,
            threads: Vec::new(),
            exec_rc: 0,
        }
    }

    /// FUN_1403d96d0's tail for Trinity Limit (command 0x1e4): its `comd`
    /// sheets take the command menu in mode 3 (FUN_1403b2a90, rows seeded
    /// by FUN_1404065a0 from DAT_1405cc4e8) and the window counts as a
    /// motion still playing (+0x54 bit 3, +0x50).
    pub fn open_special(&mut self) {
        self.special = true;
        self.combo = [(0x1e7, 2), (0x1e6, 3), (0x1e5, 4)];
        self.pending += 1;
    }

    /// FUN_1403c65e0's +0x1b0 counter: a limit motion that ended, or that
    /// another motion replaced, no longer counts as playing.
    pub fn settle(&mut self, sys: &mut crate::sysobj::Sysobj) {
        if sys.limit_motion && (sys.motion_ended() || sys.motion.slot < MOTIONS) {
            sys.limit_motion = false;
            self.pending -= 1;
        }
    }

    /// The next motion token (FUN_1403c64a0: DAT_14074b980 - 1, never
    /// zero or above).
    pub fn next_token(&mut self) -> i32 {
        self.token -= 1;
        if self.token >= 0 {
            self.token = -1;
        }
        self.token
    }
}

pub fn lookup(table: u16, index: u16) -> Option<TrapFn> {
    Some(match (table, index) {
        (1, 263) => player_set_exec_rc,
        (2, 47) => limit_motion_start,
        (2, 48) => limit_player,
        (2, 49) => limit_friend,
        (2, 50) => limit_camera_start,
        (2, 54) => limit_start_thread,
        (2, 55) => limit_light,
        (2, 56) => lockon_target,
        (2, 57) => limit_effect_start,
        (2, 58) => limit_effect_start_pos,
        (2, 59) => limit_effect_start_bind,
        (2, 60) => limit_time,
        (2, 63) => limit_reference,
        (2, 89) => limit_reset_special_command,
        (6, 43) => trinity_combo_start,
        (7, 36) => limit_reset_hit_counter,
        (8, 0) => obj_target_radius,
        _ => return None,
    })
}

fn state<'a>(ctx: &'a mut Ctx<'_>) -> Result<&'a mut LimitState, Fault> {
    ctx.world.limit.as_mut().ok_or(Fault::Memory(0))
}

fn address(args: &mut Args, handle: u32) {
    args[0] = Slot::address(handle);
}

/// 1:263 FUN_14042fee0 -> FUN_1403aaa40: the reaction command Sora runs
/// next (player +0xdd8), unless the command has flag bit 9.
fn player_set_exec_rc(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let command = args[1].as_int() as u16;
    if let Some(l) = ctx.world.limit.as_mut() {
        l.exec_rc = command;
    }
    ctx.world.events.push(Event::Service {
        name: "player_set_exec_rc",
        object: Some(PLAYER_OBJECT),
        other: None,
        words: [command as u32, 0, 0, 0, 0, 0, 0, 0],
        vector: None,
    });
    Ok(())
}

/// 2:47 FUN_140432e20 -> FUN_1403d93e0 -> FUN_1403c63e0: the participant
/// plays motion `id` of the limit file's set (its own entry, by objentry
/// key), blending over `blend`; the limit counts it as playing (+0x50)
/// until it ends. The script gets the motion's token (+0x184). Without an
/// entry for the participant its motion stops (FUN_1403c65e0's fallback).
fn limit_motion_start(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[1].value)?;
    let (motion, blend) = (args[2].as_int(), args[3].as_float());
    let slot = MOTIONS + motion.max(0) as usize;
    let token = state(ctx)?.next_token();
    // The object, then its weapons (obj+0xd60, when it has any) play the
    // motion of their own keys (FUN_1403c64a0); each counts in +0x50.
    let weapons = ctx.object(id)?.sys.weapons.clone();
    for (k, who) in std::iter::once(id).chain(weapons).enumerate() {
        let env = ctx.env;
        // A limit motion still playing stops counting: the new motion takes
        // over its counter (FUN_1403c8a40).
        if std::mem::take(&mut ctx.object(who)?.sys.limit_motion) {
            state(ctx)?.pending -= 1;
        }
        let obj = ctx.object(who)?;
        let clips = env.clips(obj.sys.model);
        let found = clips.length(slot) > 0;
        if found {
            obj.sys.motion_start(slot, blend, clips);
            // FUN_1403c65e0 marks a motion of another set (+0x190 bit 2), so
            // FUN_1403c8a40 sets +0x18c bit 14 (`obj_motion_is_capture`).
            obj.sys.motion.flags.word |= 1 << 14;
        } else if k == 0 {
            obj.sys.motion_stop();
        }
        obj.sys.limit_token = token;
        obj.sys.limit_motion = found;
        if found {
            state(ctx)?.pending += 1;
        }
    }
    args[0] = Slot::int(token);
    Ok(())
}

/// 2:48 FUN_140432e90: Sora (L+0xc).
fn limit_player(_: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    address(args, PLAYER_HANDLE);
    Ok(())
}

/// 2:49 FUN_140432ec0: participant `i` (L+0x10 + 4i), 0 for none.
fn limit_friend(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let i = args[1].as_int();
    let member = state(ctx)?
        .members
        .get(i.max(0) as usize)
        .copied()
        .flatten();
    address(args, member.map_or(0, object_handle));
    Ok(())
}

/// 2:50 FUN_140432f00 -> FUN_1403d8cf0: start camera event `name` (a type
/// 22 entry of the limit file) on Sora, with the screen shake `amount`
/// when positive. The host owns cameras.
fn limit_camera_start(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let name = m.string(args[1].value, 4).unwrap_or_default();
    let mut word = 0u32;
    for (k, b) in name.iter().take(4).enumerate() {
        word |= u32::from(*b) << (8 * k);
    }
    ctx.world.events.push(Event::Service {
        name: "limit_camera_start",
        object: Some(PLAYER_OBJECT),
        other: None,
        words: [
            word,
            args[2].value,
            (args[3].as_int() != 0) as u32,
            0,
            0,
            0,
            0,
            0,
        ],
        vector: None,
    });
    Ok(())
}

/// 2:54 FUN_140432f60 -> FUN_1403e1e30: a new thread of the scene program
/// at `entry` with one argument.
fn limit_start_thread(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let request = ThreadRequest {
        entry: args[0].as_int(),
        argument: args[1],
    };
    state(ctx)?.threads.push(request);
    Ok(())
}

/// 2:55 FUN_140432f80: the colour filter (L+0x4c).
fn limit_light(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let filter = state(ctx)?.filter;
    address(args, filter);
    Ok(())
}

/// 2:56 FUN_140431ad0: the object's lock-on record (&(o+0xdc8)->+4); only
/// Sora has one.
fn lockon_target(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let record = if id == PLAYER_OBJECT {
        crate::world::LOCKON_RECORD
    } else {
        0
    };
    address(args, record);
    Ok(())
}

fn limit_effect(
    ctx: &mut Ctx<'_>,
    effect: i32,
    argument: i32,
    position: Option<[f32; 3]>,
    other: Option<ObjId>,
) -> u32 {
    let handle = ctx.world.next_effect_handle();
    ctx.world.events.push(Event::EffectStart {
        object: LIMIT_EFFECTS,
        effect,
        arguments: [argument, 0],
        handle,
        position,
        bound: other.is_some(),
        other,
    });
    handle
}

/// 2:57 FUN_140432fb0 -> FUN_1403d9b60: effect `id` of the limit file's
/// container, neither placed nor bound; the script gets its handle.
fn limit_effect_start(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let (effect, argument) = (args[1].as_int(), args[2].as_int());
    let handle = limit_effect(ctx, effect, argument, None, None);
    address(args, handle);
    Ok(())
}

/// 2:58 FUN_140433000 -> FUN_1403d9b60 -> FUN_1402c6b30: effect `id` of
/// the limit file's container (priority 30000) at a point (NaN lanes
/// read as 0).
fn limit_effect_start_pos(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let (effect, argument) = (args[1].as_int(), args[3].as_int());
    let lane = |k: u32| {
        let v = ctx.read_f32(m, args[2].value + 4 * k).unwrap_or(0.);
        if v.is_nan() { 0. } else { v }
    };
    // Script frame (Y down) to the engine's (Y up).
    let position = [lane(0), -lane(1), lane(2)];
    let handle = limit_effect(ctx, effect, argument, Some(position), None);
    address(args, handle);
    Ok(())
}

/// 2:59 FUN_1404330b0 -> FUN_1403d9b60: effect `id` of the limit file's
/// container bound to an object.
fn limit_effect_start_bind(
    ctx: &mut Ctx<'_>,
    m: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let (effect, argument) = (args[1].as_int(), args[3].as_int());
    let other = ctx.sys_of_ai(m, args[2].value)?;
    let handle = limit_effect(ctx, effect, argument, None, Some(other));
    address(args, handle);
    Ok(())
}

/// 2:60 FUN_140433130: the time left (L+0x118).
fn limit_time(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let time = state(ctx)?.time;
    args[0] = Slot::float(time);
    Ok(())
}

/// 2:63 FUN_140433160: the motions still playing (L+0x50).
fn limit_reference(ctx: &mut Ctx<'_>, _: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let pending = state(ctx)?.pending;
    args[0] = Slot::int(pending);
    Ok(())
}

/// 2:89 FUN_140433190 -> FUN_1403d9490: close the Trinity command window
/// (+0x54 bit 3) and count it out of +0x50.
fn limit_reset_special_command(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    _: &mut Args,
) -> Result<(), Fault> {
    let l = state(ctx)?;
    if l.special {
        l.special = false;
        l.pending -= 1;
        ctx.world.events.push(Event::Service {
            name: "limit_reset_special_command",
            object: Some(PLAYER_OBJECT),
            other: None,
            words: [0; 8],
            vector: None,
        });
    }
    Ok(())
}

/// 6:43 FUN_14053ea50: row `n` of the special deck becomes 0x1e8.
fn trinity_combo_start(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let n = args[0].as_int();
    if let Some(row) = state(ctx)?.combo.get_mut(n.max(0) as usize) {
        row.0 = 0x1e8;
    }
    Ok(())
}

/// 7:36 FUN_14053fb30 -> FUN_1403d9450: the combo counter's hits (+0xc0),
/// and the counter resets (FUN_1403fa860, its vtable +0x10).
fn limit_reset_hit_counter(
    ctx: &mut Ctx<'_>,
    _: &mut Memory<'_>,
    args: &mut Args,
) -> Result<(), Fault> {
    let hits = std::mem::take(&mut state(ctx)?.hits);
    args[0] = Slot::int(hits);
    Ok(())
}

/// 8:0 FUN_140537dc0: the radius of the object's TARGET shape
/// (FUN_1403be0f0 +4); 0 when the host has none.
fn obj_target_radius(ctx: &mut Ctx<'_>, m: &mut Memory<'_>, args: &mut Args) -> Result<(), Fault> {
    let id = ctx.sys_of_ai(m, args[0].value)?;
    let r = ctx.env.target_radius(id).unwrap_or(0.);
    args[0] = Slot::float(r);
    Ok(())
}
