//! kn::Friend, the controller the executable builds summons (and party
//! members) on (FUN_1401b06e0), translated for the summons: the sensing
//! (FUN_1401b8910), the target choice (FUN_1401b6b80, FUN_1401b3520,
//! FUN_1401cbe50), the following (FUN_1401b41a0 on the ground,
//! FUN_1401b6360 in flight, FUN_1401b4820, FUN_1401b23d0), the wish
//! smoothing (FUN_1401b1b20), the think (FUN_1401b53b0, Chicken Little's
//! FUN_1401cb050) and the action choice (FUN_1401b1d40) over each summon's
//! action table.
//!
//! Positions here are the engine's (+Y up); the executable's height
//! differences (Y down) keep their sign as `dy` = self − other in +Y up.
//!
//! [Open] Not ported: Stitch's camera-space controller (FUN_1401ca240),
//! the warp trail's teleport (FUN_1401b7750), the step and ledge probes and
//! their jump requests (FUN_1403b7f10), the support items, limits.

pub mod stitch;

use crate::env::Env;
use crate::sysobj::class_flag;
use crate::traps::target::{OBJECT_TARGET, PLAYER_TARGET};
use crate::world::{ObjId, PLAYER_OBJECT, World};

type Vec3 = [f32; 3];

/// The personality index (+0xe08) of the summons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Personality {
    /// 0xe, character 200.
    Stitch,
    /// 0xf, character 0xc9.
    Genie,
    /// 0x10, character 0xca.
    PeterPan,
    /// 0x11, character 0xcb.
    Tink,
    /// 0x12, character 0xcc.
    ChickenLittle,
}

impl Personality {
    /// FUN_1401b06e0's switch on objentry +0x4c.
    pub fn of_character(character: u16) -> Option<Self> {
        Some(match character {
            200 => Self::Stitch,
            0xc9 => Self::Genie,
            0xca => Self::PeterPan,
            0xcb => Self::Tink,
            0xcc => Self::ChickenLittle,
            _ => return None,
        })
    }

    /// FUN_1401b53b0 step 3: Genie and Peter Pan keep to flight (mode 0x1f).
    pub fn flies(self) -> bool {
        matches!(self, Self::Genie | Self::PeterPan)
    }
}

/// Bits of the flag word +0xe18.
pub mod flag {
    pub const WARP_OFF: u32 = 1 << 1;
    pub const CONTINUES: u32 = 1 << 2;
    pub const TARGET_CHANGED: u32 = 1 << 3;
    pub const IDLE_TURN: u32 = 1 << 4;
    pub const MAGIC: u32 = 1 << 5;
    /// Beyond the leash from Sora: enemies are ignored.
    pub const LEASHED: u32 = 1 << 6;
    pub const MANUAL_WISH: u32 = 1 << 7;
    /// Peter Pan took Sora's attack notification (`peterpan_accept_...`).
    pub const NOTIFY_TAKEN: u32 = 1 << 9;
    /// IVTABLE +0x108 answers 0 (`friend_disable_system_wishdir`).
    pub const NO_SYSTEM_WISH: u32 = 1 << 10;
    pub const NO_REST: u32 = 1 << 11;
    pub const NO_ENEMY_FOLLOW: u32 = 1 << 12;
    pub const INERTIA_OFF: u32 = 1 << 14;
    pub const TARGET_LOCKED: u32 = 1 << 16;
    pub const NO_PLAYER_FOLLOW: u32 = 1 << 17;
    /// The first frame: locomotion returns at once.
    pub const FRESH: u32 = 1 << 28;
    pub const FIRST_DONE: u32 = 1 << 29;
    /// `friend_start_leave`, read only by `friend_is_start_leave`.
    pub const LEAVE: u32 = 1 << 30;
    pub const CONTROLLED: u32 = 1 << 31;
}

/// The personality's parameter blob (0x14071d800 + offset table), the
/// summon values: the gait bands (down, up) of the field table (which the
/// battle-vs-player table equals) and of the battle-vs-enemy table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blob {
    pub field: [(f32, f32); 4],
    pub enemy: [(f32, f32); 4],
    /// +0x68 target search radius, +0x6c drop range, +0x80 leash.
    pub search: f32,
    pub drop: f32,
    pub leash: f32,
    /// B+0x70 / +0x74: the wait after an action, its least and most.
    pub wait: (f32, f32),
}

const BANDS: [(f32, f32); 4] = [(0., 200.), (160., 240.), (200., 1000.), (0., 0.)];
const BANDS_ENEMY: [(f32, f32); 4] = [(0., 0.), (160., 240.), (200., 1000.), (0., 0.)];

impl Blob {
    pub fn of(p: Personality) -> Self {
        let (field, enemy) = match p {
            Personality::Genie => (
                [(0., 300.), (270., 360.), (330., 1000.), (0., 0.)],
                BANDS_ENEMY,
            ),
            Personality::ChickenLittle => (
                [(0., 160.), (120., 200.), (160., 1000.), (0., 0.)],
                [(0., 0.), (120., 200.), (160., 1000.), (0., 0.)],
            ),
            _ => (BANDS, BANDS_ENEMY),
        };
        let wait = match p {
            Personality::Stitch => (90., 180.),
            _ => (30., 90.),
        };
        Blob {
            field,
            enemy,
            search: 600.,
            drop: 1000.,
            leash: 1000.,
            wait,
        }
    }

    /// FUN_1401cc890 / FUN_1401cc8c0: (down, up) of `band`; the field table
    /// out of battle and when following Sora, the enemy table otherwise.
    fn band(&self, band: usize, battle: bool, player: bool) -> (f32, f32) {
        let t = if !battle || player {
            &self.field
        } else {
            &self.enemy
        };
        t[band.min(3)]
    }
}

/// An action-table entry (0x48 bytes): its flags and routine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// E+0: bit 0 not recorded, bit 1 attack, bit 4/5 support passes,
    /// bit 9 continuable.
    pub flags: u32,
    pub action: Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// FUN_1401cb2a0 / FUN_1401cbda0: test true (with the current target);
    /// run starts the script label E+0x40, `battle` (FUN_1403caa90).
    Battle,
    /// FUN_1401cf450: follow Sora (FUN_1401b4820).
    Follow,
    /// FUN_1401cf470: Sora's HP under 25% → command 1 on Sora.
    HealPlayer,
    /// Stitch (table 0x140724340): FUN_1401c7f40 stand, FUN_1401c8950
    /// cross the screen, FUN_1401ca6e0 / FUN_1401c8100 the HP and MP
    /// specials, FUN_1401c8610 / FUN_1401c8700 / FUN_1401c8440 the limit
    /// commands (their motions 0xc9, 0xcd, 0xf2).
    StitchIdle,
    StitchMove,
    StitchHp,
    StitchMp,
    StitchCommand(u16),
}

/// The action tables (decoded from their static initializers).
pub fn table(p: Personality) -> &'static [Entry] {
    const BATTLE: [Entry; 1] = [Entry {
        flags: 0x3,
        action: Action::Battle,
    }];
    const GENIE: [Entry; 2] = [
        Entry {
            flags: 0x201,
            action: Action::Follow,
        },
        Entry {
            flags: 0x10,
            action: Action::HealPlayer,
        },
    ];
    const STITCH: [Entry; 7] = [
        Entry {
            flags: 0x201,
            action: Action::StitchIdle,
        },
        Entry {
            flags: 0x201,
            action: Action::StitchMove,
        },
        Entry {
            flags: 0x212,
            action: Action::StitchHp,
        },
        Entry {
            flags: 0x220,
            action: Action::StitchMp,
        },
        Entry {
            flags: 0x212,
            action: Action::StitchCommand(0xc9),
        },
        Entry {
            flags: 0x212,
            action: Action::StitchCommand(0xcd),
        },
        Entry {
            flags: 0x212,
            action: Action::StitchCommand(0xf2),
        },
    ];
    match p {
        Personality::ChickenLittle | Personality::PeterPan => &BATTLE,
        Personality::Genie => &GENIE,
        Personality::Stitch => &STITCH,
        // Tink has none.
        Personality::Tink => &[],
    }
}

/// A sensing block (+0xe1c Sora's, +0xe40 the target's): the height
/// difference, the unit 3D and horizontal vectors and their lengths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Block {
    /// The other's height minus this one's, in the executable's Y-down
    /// sense (negative = the other is above).
    pub dy: f32,
    pub dir: Vec3,
    pub dist: f32,
    pub hdir: Vec3,
    pub hdist: f32,
}

impl Block {
    fn between(me: Vec3, other: Vec3) -> Self {
        let v = [other[0] - me[0], other[1] - me[1], other[2] - me[2]];
        let dist = len(v);
        let h = [v[0], 0., v[2]];
        let hdist = len(h);
        Block {
            dy: -v[1],
            dir: unit(v),
            dist,
            hdir: unit(h),
            hdist,
        }
    }
}

/// What the think asks of its host this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Requests {
    /// Start this script label (`start_if_not_outranked`).
    pub label: Option<&'static [u8]>,
    /// Use command `index` on `target` (FUN_1401b7d90).
    pub command: Option<(u32, ObjId)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Friend {
    pub personality: Personality,
    pub blob: Blob,
    /// +0xe18.
    pub flags: u32,
    /// +0xe14: 0 stand, 1 walk, 2 run.
    pub band: usize,
    /// +0xe78: unit direction and speed.
    pub wish: [f32; 4],
    /// +0xf00 ring of six and its index (+0xf60, -1 empty).
    ring: [[f32; 4]; 6],
    ring_index: i32,
    pub player: Block,
    pub target: Block,
    /// +0xe64: the target's last known position.
    pub target_last: Vec3,
    /// +0xee8 the current entry, +0xf80 consecutive recorded actions.
    pub current: Option<usize>,
    pub recorded: i32,
    /// +0xf88: the last enemy that hit this friend.
    pub last_attacker: Option<ObjId>,
    /// +0x128 walk, +0x12c run speed.
    pub walk: f32,
    pub run: f32,
    /// The warp trail has a point (DAT_140ad6ae0): Sora has moved 45 units.
    trail: Option<Vec3>,
    trail_valid: bool,
    seed: u32,
    /// Genie's form command (personality +0x28, `genie_change_form`).
    pub genie_form: u16,
    /// Chicken Little's aim point (personality +0x40).
    pub aim: Vec3,
    /// The watched effects (work +0x18 count, +0x20 sixteen-byte pairs of
    /// effect and serial; our effect handles are never reused, so the
    /// handle is both).
    pub watch: Vec<u32>,
    /// +0xf64: the wait timer (down a frame at a time while above 0).
    pub wait: f32,
    /// kn::Stitch's personality state.
    pub stitch: Option<Box<stitch::Stitch>>,
    /// +0x28 bit 0: Chicken Little's limit mode (FUN_1401cb6c0 sets it,
    /// FUN_1401cb180 clears it).
    pub limit: bool,
    /// +0xfa0: the reaction-command callback `friend_call` (7:26,
    /// FUN_1401b2870) runs; only Stitch's limit sets one (FUN_1401c7bf0).
    pub call: Option<Call>,
    /// The personality's target list (FUN_1401b4ac0: count +0x18, 32
    /// objects from +0x20), which FUN_1401cd8e0 clears.
    pub targets: Vec<ObjId>,
}

/// A `friend_call` callback (+0xfa0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// FUN_1401c7bf0: Stitch's limit commands.
    StitchCommand,
}

/// Stitch's limit deck: FUN_1401c9d60 -> FUN_1403db140(obj, 4), whose rows
/// FUN_1404060e0 takes from 0x1405cc470 (command, selector): 0x20e Square,
/// 0x20d Cross (two rows). [I] The other table (0x1405cc468) as for
/// [`CHICKEN_LITTLE_DECK`].
pub const STITCH_DECK: [(u16, u16); 2] = [(0x20e, 3), (0x20d, 4)];

/// Chicken Little's limit deck: FUN_1401cb6c0 -> FUN_1403db140(obj, 5)
/// gives Sora's command menu his own `comd` sheet in mode 5, whose rows
/// FUN_1404060e0 takes from 0x1405cc488 (command, selector): 0x213
/// Triangle, 0x214 Square, 0x212 (his shot) Cross. [I] The other table
/// (0x1405cc478, chosen while DAT_14079cf00 +0x274 is 0 or 7) is read as
/// the circle-confirm layout this build does not use.
pub const CHICKEN_LITTLE_DECK: [(u16, u16); 3] = [(0x213, 2), (0x214, 3), (0x212, 4)];
/// FUN_1401cb830 without a locked target: the aim point lies this far
/// along the camera's forward from its eye (DAT_140623bf8).
pub const CHICKEN_LITTLE_AIM: f32 = 300.;

impl Friend {
    /// FUN_1401b06e0 for a summon: flags 0x10000000 plus the personality's
    /// (Stitch and Tink warp off, Genie / Peter Pan / Tink no rest rule,
    /// Chicken Little never follows enemies).
    pub fn new(personality: Personality, walk: f32, run: f32) -> Self {
        let mut flags = flag::FRESH;
        flags |= match personality {
            Personality::Stitch => flag::WARP_OFF,
            Personality::Genie | Personality::PeterPan => flag::NO_REST,
            Personality::Tink => flag::NO_REST | flag::WARP_OFF,
            Personality::ChickenLittle => flag::NO_ENEMY_FOLLOW,
        };
        Friend {
            personality,
            blob: Blob::of(personality),
            flags,
            band: 0,
            wish: [0., 0., 0., 0.],
            ring: [[0., 0., 0., 1.]; 6],
            ring_index: -1,
            player: Block::default(),
            target: Block::default(),
            target_last: [0.; 3],
            current: None,
            recorded: 0,
            last_attacker: None,
            walk,
            run,
            trail: None,
            trail_valid: false,
            seed: 1,
            genie_form: 0,
            aim: [0.; 3],
            watch: Vec::new(),
            wait: 0.,
            stitch: (personality == Personality::Stitch)
                .then(|| Box::new(stitch::Stitch::default())),
            limit: false,
            call: None,
            targets: Vec::new(),
        }
    }
}

/// One frame of a friend (FUN_1401b15f0's part before the movement): the
/// think, then the wish becomes the object's movement (+0x10) and facing.
/// Returns what the host must start.
pub fn tick(
    world: &mut World,
    id: ObjId,
    me: &mut crate::world::Object,
    env: &dyn Env,
) -> Requests {
    let mut out = Requests::default();
    let Some(mut f) = me.friend.take() else {
        return out;
    };
    // FUN_1401b39e0 -> FUN_1401ce050: each frame the watch lists drop the
    // effects that have ended [I: the sweep runs ahead of the friend's
    // own update here].
    f.sweep_watch(env);
    // FUN_1401b5e30: the warp trail takes a point every 45 units of Sora's.
    let player = env.player_position();
    match f.trail {
        None => f.trail = Some(player),
        Some(p) if dist2(p, player) >= 2025. => {
            f.trail = Some(player);
            f.trail_valid = true;
        }
        _ => {}
    }
    f.wish = [0.; 4];
    f.flags &= !flag::MANUAL_WISH;
    // FUN_1401b15f0: the wait timer runs down while it is above 0.
    if f.wait > 0. {
        f.wait -= 1.;
    }
    if f.personality == Personality::Stitch {
        stitch::tick(world, me, &mut f, env, &mut out);
    } else {
        think(world, id, me, &mut f, env, &mut out);
    }
    f.flags &= !flag::FRESH;
    me.friend = Some(f);
    out
}

fn controlled(me: &crate::world::Object) -> bool {
    // +0x18c & 0x1c (the free windows), or +0x18c bit 30 (move only).
    me.sys.motion.flags.word & (0x1c | 1 << 30) != 0
}

/// FUN_1401b53b0 (and Chicken Little's FUN_1401cb050, which runs it).
fn think(
    world: &mut World,
    id: ObjId,
    me: &mut crate::world::Object,
    f: &mut Friend,
    env: &dyn Env,
    out: &mut Requests,
) {
    if !controlled(me) {
        return;
    }
    f.flags |= flag::CONTROLLED;
    let _ = id;
    me.sys.target.id = record_id(sense(world, me, f, env, false));
    let battle = env.battle();
    let move_only = me.sys.motion.flags.word & 1 << 30 != 0;
    if !battle || move_only {
        follow_player(me, f, env);
    } else {
        f.flags &= !flag::MAGIC;
        // The support target (+0xf78): Sora when FUN_1403c1d30 says he needs
        // healing (HP <= MaxHP / 4); a summon (index >= 0xe) looks no
        // further.
        let support = env.player_hp().is_some_and(|(hp, max)| hp <= max / 4);
        if !(support && support_pass(world, me, f, env, out)) {
            choose(world, me, f, env, out);
        }
    }
    if f.flags & flag::MANUAL_WISH == 0 {
        smooth(f);
    }
    // FUN_1401cb050: Chicken Little aims at the target, or 200 ahead.
    if f.personality == Personality::ChickenLittle {
        f.aim = position(world, target_of(me)).unwrap_or_else(|| {
            let (s, c) = me.sys.yaw.sin_cos();
            let p = me.sys.position;
            [p[0] + s * 200., p[1], p[2] + c * 200.]
        });
    }
    // +0x10 = +0xe78 (FUN_1401b53b0); the ground mode turns the object
    // toward the direction it moves [I].
    me.sys.move_direction = [f.wish[0], f.wish[1], f.wish[2]];
    me.sys.move_speed = f.wish[3];
    if f.wish[3] > 0. && (f.wish[0] != 0. || f.wish[2] != 0.) {
        me.sys.wish_yaw = Some(f.wish[0].atan2(f.wish[2]));
    }
}

/// FUN_1401b8910: Sora's block, the leash, the target choice and the
/// target's block.
fn sense(
    world: &World,
    me: &crate::world::Object,
    f: &mut Friend,
    env: &dyn Env,
    force: bool,
) -> ObjId {
    f.flags &= !flag::TARGET_CHANGED;
    let at = me.sys.position;
    f.player = Block::between(at, env.player_position());
    // The leash (summons' style is 0: no scaling).
    if f.flags & flag::NO_PLAYER_FOLLOW == 0 && f.blob.leash < f.player.hdist {
        f.flags |= flag::LEASHED;
    }
    let old = target_of(me);
    let battle = env.battle();
    let mut target = old;
    if !battle {
        if force || f.flags & flag::TARGET_LOCKED == 0 {
            target = 0;
        }
    } else if force || f.flags & flag::TARGET_LOCKED == 0 {
        let keep = !force
            && f.flags & flag::LEASHED == 0
            && valid(world, old)
            && !reselect(world, me, f, old);
        if !keep {
            target = select(world, me, f, env).unwrap_or(0);
            if !valid(world, target) {
                target = 0;
            }
        }
    }
    if target != old {
        f.flags |= flag::TARGET_CHANGED;
    }
    if let Some(p) = position(world, target) {
        f.target = Block::between(at, p);
        f.target_last = p;
    } else {
        f.target = Block::default();
    }
    target
}

/// The validity routine (FUN_1401b8910 by personality): true = reselect.
fn reselect(world: &World, me: &crate::world::Object, f: &Friend, target: ObjId) -> bool {
    match f.personality {
        // 0x1401b35d0: keep.
        Personality::Genie | Personality::Tink => false,
        // 0x1401cbd90 and 0x1401b3700: always.
        Personality::PeterPan | Personality::Stitch => true,
        // FUN_1401b3520: the last attacker differs, or beyond the drop range.
        Personality::ChickenLittle => {
            if let Some(a) = f.last_attacker
                && a != target
            {
                return true;
            }
            position(world, target)
                .is_none_or(|p| dist2(p, me.sys.position) > f.blob.drop * f.blob.drop)
        }
    }
}

/// The selector (FUN_1401b8910 by personality).
fn select(world: &World, me: &crate::world::Object, f: &Friend, env: &dyn Env) -> Option<ObjId> {
    match f.personality {
        // 0x1401b7490: Stitch picks his own.
        Personality::Stitch => None,
        // FUN_1401cbe50: an enemy attacking Sora, else the target Sora
        // attacked within 8 frames, else the old one.
        Personality::PeterPan => world
            .player_attackers
            .iter()
            .copied()
            .filter(|&o| valid(world, o))
            .min_by(|&a, &b| {
                let d = |o| position(world, o).map_or(f32::MAX, |p| dist2(p, me.sys.position));
                d(a).total_cmp(&d(b))
            })
            .or_else(|| {
                // FUN_1401b72c0 with the fresh, untaken notification's target.
                let n = world.player_notify;
                (n.fresh() && f.flags & flag::NOTIFY_TAKEN == 0)
                    .then_some(n.target)
                    .filter(|&o| valid(world, o))
            })
            .or(Some(target_of(me))),
        _ => nearest(world, me, f, env),
    }
}

/// FUN_1401b6b80: the last attacker when it is in reach, else the nearest
/// battle target whose surface (distance − radius) is under the search
/// radius.
fn nearest(world: &World, me: &crate::world::Object, f: &Friend, env: &dyn Env) -> Option<ObjId> {
    let r = f.blob.search;
    let key = |o: ObjId| {
        let p = position(world, o)?;
        let radius = env.target_radius(o).unwrap_or(0.);
        Some(dist2(p, me.sys.position).sqrt() - radius)
    };
    if let Some(a) = f.last_attacker
        && valid(world, a)
        && key(a).is_some_and(|k| k < r)
    {
        return Some(a);
    }
    world
        .objects
        .iter()
        .flatten()
        .filter(|o| o.id != me.id && valid(world, o.id))
        .filter_map(|o| Some((o.id, key(o.id)?)))
        .filter(|&(_, k)| k < r)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(o, _)| o)
}

/// Sora's attack notification. FUN_1401b56e0 mode 1 notes an attack
/// (DAT_14071d684 = Sora's +0x180, DAT_140ad6a74 = DAT_140ad6a80 = the
/// motion clock, DAT_140ad6a78 = the target when FUN_1401b51a0 accepts it)
/// and clears bit 9 of both friends; FUN_1401b5d60 ages it each frame
/// (DAT_140ad6a74 counts down) and drops it when Sora's +0x180 changes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerNotify {
    /// The attack noted (None = -1).
    pub attack: Option<u32>,
    /// Frames since (DAT_140ad6a80 - DAT_140ad6a74).
    pub age: f32,
    /// The attack's target (0 = none).
    pub target: ObjId,
}

impl PlayerNotify {
    /// FUN_1401cbdf0's test: noted, and DAT_140ad6a80 <= DAT_140ad6a74 + 8.
    pub fn fresh(&self) -> bool {
        self.attack.is_some() && self.age <= 8.
    }
}

/// FUN_1401b56e0 mode 1: Sora began attack `attack` at `target`. The
/// friends' notifications (bit 9) are untaken again; `me` is a friend the
/// host holds outside the list.
pub fn note_player_attack(
    world: &mut World,
    me: Option<&mut crate::world::Object>,
    attack: u32,
    target: ObjId,
) {
    let target = if valid(world, target) { target } else { 0 };
    world.player_notify = PlayerNotify {
        attack: Some(attack),
        age: 0.,
        target,
    };
    let untake = |o: &mut crate::world::Object| {
        if let Some(f) = o.friend.as_deref_mut() {
            f.flags &= !flag::NOTIFY_TAKEN;
        }
    };
    for o in world.objects.iter_mut().flatten() {
        untake(o);
    }
    for o in world.held.iter_mut() {
        untake(o);
    }
    if let Some(o) = me {
        untake(o);
    }
}

/// FUN_1401b5d60: once a frame; `current` is Sora's attack now (+0x180).
pub fn age_player_notify(world: &mut World, current: Option<u32>) {
    let n = &mut world.player_notify;
    if n.attack.is_none() || n.attack != current {
        *n = PlayerNotify::default();
    } else {
        n.age += 1.;
    }
}

/// The object the target record (+0xbf8) names: FUN_1401b8910 keeps the
/// friend's choice there as an engine object, like `target_set_obj`. Sora
/// and the host's targets name no object here.
fn target_of(me: &crate::world::Object) -> ObjId {
    let id = me.sys.target.id;
    if id != PLAYER_TARGET && id & OBJECT_TARGET != 0 {
        id & !OBJECT_TARGET
    } else {
        0
    }
}

/// The record id for an object (0 for none).
fn record_id(o: ObjId) -> u32 {
    if o == 0 { 0 } else { OBJECT_TARGET | o }
}

/// FUN_1401b51a0 with FUN_1403bd420: a live, targetable battle target
/// (+0x6c8 bit 4).
fn valid(world: &World, o: ObjId) -> bool {
    if o == 0 || o == PLAYER_OBJECT {
        return false;
    }
    world
        .objects
        .get(o as usize)
        .and_then(|x| x.as_deref())
        .is_some_and(|x| x.sys.class_flags & class_flag::ENEMY != 0 && x.targetable())
}

fn position(world: &World, o: ObjId) -> Option<Vec3> {
    if o == 0 {
        return None;
    }
    world
        .objects
        .get(o as usize)
        .and_then(|x| x.as_deref())
        .map(|x| x.sys.position)
}

/// Trap 7:19 (FUN_1401b8910 with `force`): sense again now; true when the
/// target changed.
pub fn sense_now(world: &World, me: &mut crate::world::Object, env: &dyn Env, force: bool) -> bool {
    let Some(mut f) = me.friend.take() else {
        return false;
    };
    me.sys.target.id = record_id(sense(world, me, &mut f, env, force));
    let changed = f.flags & flag::TARGET_CHANGED != 0;
    me.friend = Some(f);
    changed
}

/// Traps 10:20 (FUN_1401b4820) and 10:21 (FUN_1401b4190): one step toward
/// Sora or the target now, copied to the object's movement (+0x10).
pub fn follow_now(me: &mut crate::world::Object, env: &dyn Env, enemy: bool) {
    let Some(mut f) = me.friend.take() else {
        return;
    };
    if enemy {
        let block = f.target;
        locomote(me, &mut f, env, block, false);
    } else {
        follow_player(me, &mut f, env);
    }
    me.sys.move_direction = [f.wish[0], f.wish[1], f.wish[2]];
    me.sys.move_speed = f.wish[3];
    me.friend = Some(f);
}

/// FUN_1401b4820: follow Sora; a flier going faster than walking keeps up
/// with Sora's speed + 2.
fn follow_player(me: &crate::world::Object, f: &mut Friend, env: &dyn Env) {
    let block = f.player;
    locomote(me, f, env, block, true);
    if f.personality.flies() && f.wish[3] > f.walk {
        let v = env.player_velocity();
        let s = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if s > f.wish[3] {
            f.wish[3] = s + 2.;
        }
    }
}

/// FUN_1401b23d0: toward the target unless enemies are not followed or
/// the leash holds, else Sora.
fn follow(world: &World, me: &crate::world::Object, f: &mut Friend, env: &dyn Env) {
    let target = target_of(me);
    if f.flags & (flag::NO_ENEMY_FOLLOW | flag::LEASHED) == 0 && valid(world, target) {
        let block = f.target;
        locomote(me, f, env, block, false);
        return;
    }
    if f.flags & flag::NO_PLAYER_FOLLOW == 0 {
        follow_player(me, f, env);
    }
}

/// FUN_1401b41a0 (ground) and FUN_1401b6360's flight branch: the wished
/// direction, the gait band and its speed, and the idle turn toward Sora.
fn locomote(me: &crate::world::Object, f: &mut Friend, env: &dyn Env, b: Block, player: bool) {
    if f.flags & flag::FRESH != 0 {
        return;
    }
    let flight = f.personality.flies();
    let battle = env.battle();
    // The direction: Sora's slot (slot 0 = Sora's own position) or the
    // block's vector; flat on the ground, 3D in flight.
    let dir = if player && f.flags & flag::IDLE_TURN == 0 {
        let p = env.player_position();
        let s = me.sys.position;
        let v = if flight {
            [p[0] - s[0], p[1] - s[1], p[2] - s[2]]
        } else {
            [p[0] - s[0], 0., p[2] - s[2]]
        };
        unit(v)
    } else if flight {
        b.dir
    } else {
        b.hdir
    };
    f.wish = [dir[0], dir[1], dir[2], 0.];
    // The gait band on the horizontal (ground) or 3D (flight) distance.
    let d = if flight { b.dist } else { b.hdist };
    if f.band != 3 {
        let (down, up) = f.blob.band(f.band, battle, player);
        if d > up {
            if f.band < 2 {
                f.band += 1;
            }
        } else if d < down && f.band > 0 {
            f.band -= 1;
        }
    }
    let mut speed = match f.band {
        1 => f.walk,
        2 => f.run,
        _ => 0.,
    };
    // The idle turn toward Sora: facing · direction under 0.5 starts it,
    // over 0.97 ends it.
    if player && speed == 0. {
        let facing = [me.sys.yaw.sin(), 0., me.sys.yaw.cos()];
        let (lean, turn_speed) = if flight {
            (0.8, f.walk * 0.2)
        } else {
            (0.4, f.walk)
        };
        let hd = if flight { b.dir } else { b.hdir };
        if f.flags & flag::IDLE_TURN == 0 {
            if f.trail_valid && dot(facing, hd) < 0.5 {
                f.flags |= flag::IDLE_TURN;
            }
            f.flags &= !flag::LEASHED;
        } else {
            let w = unit([
                f.wish[0] + facing[0] * lean,
                f.wish[1] + facing[1] * lean,
                f.wish[2] + facing[2] * lean,
            ]);
            f.wish = [w[0], w[1], w[2], 0.];
            speed = turn_speed;
            if dot(facing, hd) > 0.97 {
                f.flags &= !(flag::IDLE_TURN | flag::LEASHED);
            } else {
                f.flags &= !flag::LEASHED;
            }
        }
    } else {
        f.flags &= !flag::IDLE_TURN;
    }
    // A target well above and almost overhead: stand.
    if !flight && b.hdist < 8. && b.dy < -250. {
        speed = 0.;
    }
    f.wish[3] = speed;
}

/// FUN_1401b3a30: the support pass. History and current action clear; the
/// entries flagged 0x10, then those flagged 0x20, are tested and one that
/// passes runs. Nothing passing restores the current action.
fn support_pass(
    world: &World,
    me: &mut crate::world::Object,
    f: &mut Friend,
    env: &dyn Env,
    out: &mut Requests,
) -> bool {
    let table = table(f.personality);
    let saved = f.current;
    f.flags |= flag::MAGIC;
    f.current = None;
    f.recorded = 0;
    for bit in [0x10, 0x20] {
        let passing: Vec<usize> = (0..table.len())
            .filter(|&i| table[i].flags & bit != 0)
            .filter(|&i| test(&table[i], world, me, f, env))
            .collect();
        if !passing.is_empty() {
            let k = passing[f.rand(passing.len() as u32) as usize];
            let e = &table[k];
            run(e, world, me, f, env, out);
            f.recorded = if e.flags & 1 == 0 { f.recorded + 1 } else { 0 };
            f.current = Some(k);
            return true;
        }
    }
    f.current = saved;
    false
}

/// FUN_1401b1d40: carry on a continuable action, else (the rest rule
/// aside) a uniform pick among the entries whose test passes; nothing →
/// follow and stand, facing the way.
pub(crate) fn choose(
    world: &World,
    me: &mut crate::world::Object,
    f: &mut Friend,
    env: &dyn Env,
    out: &mut Requests,
) {
    let table = table(f.personality);
    if let Some(c) = f.current
        && let Some(e) = table.get(c)
        && e.flags & 0x200 != 0
    {
        f.flags |= flag::CONTINUES;
        if test(e, world, me, f, env) {
            run(e, world, me, f, env, out);
            return;
        }
    }
    if f.flags & flag::NO_REST == 0 && f.recorded >= 2 {
        follow(world, me, f, env);
        f.recorded = 0;
        f.current = None;
        return;
    }
    f.flags &= !flag::CONTINUES;
    // Summons (index >= 0xe) have no banned entry and no ability slots: one
    // pass over the table.
    let passing: Vec<usize> = (0..table.len())
        .filter(|&i| test(&table[i], world, me, f, env))
        .collect();
    if !passing.is_empty() {
        let k = passing[f.rand(passing.len() as u32) as usize];
        let e = &table[k];
        run(e, world, me, f, env, out);
        if e.flags & 1 == 0 {
            f.recorded += 1;
        } else {
            f.recorded = 0;
        }
        f.current = Some(k);
        return;
    }
    follow(world, me, f, env);
    f.wish[3] = 0.;
    f.recorded = 0;
    f.current = None;
}

impl Friend {
    /// FUN_14019be90(n): `(int)(rand() * n * 3.0517578e-05)`, the C runtime's
    /// generator (×214013 + 2531011, bits 16..30) [I: its seed is the
    /// process's; here 1].
    fn rand(&mut self, n: u32) -> u32 {
        let r = self.crt_rand();
        ((r as f32) * n as f32 * 3.051_757_8e-5) as u32
    }

    /// The C runtime's `rand()`: 0..=0x7fff.
    fn crt_rand(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(214013).wrapping_add(2531011);
        (self.seed >> 16) & 0x7fff
    }

    /// FUN_1401b4960: `rand(20001) × 5.0e-5`, 0..=1.
    fn rand01(&mut self) -> f32 {
        self.rand(0x4e21) as f32 * 5.0e-5
    }

    /// FUN_14019bf50: `rand() × (hi − lo) × 3.0517578e-05 + lo`.
    fn rand_range(&mut self, lo: f32, hi: f32) -> f32 {
        self.crt_rand() as f32 * (hi - lo) * 3.051_757_8e-5 + lo
    }

    /// FUN_1401ce620: keep the effects that still play, in order.
    fn sweep_watch(&mut self, env: &dyn Env) {
        self.watch.retain(|&e| e != 0 && env.effect_active(e));
    }

    /// FUN_1401ce1e0 (`friend_add_watch_effect`): drop the effect if listed
    /// (FUN_1401ce480 also sweeps), make room in the 32 slots (sweep, then
    /// the oldest goes) and append it. A null effect is ignored.
    pub fn add_watch(&mut self, effect: u32, env: &dyn Env) {
        if effect == 0 {
            return;
        }
        self.watch.retain(|&e| e != effect);
        self.sweep_watch(env);
        if self.watch.len() == 32 {
            self.sweep_watch(env);
            if self.watch.len() == 32 {
                self.watch.remove(0);
            }
        }
        self.watch.push(effect);
    }

    /// FUN_1401ce410 (`friend_is_effect_exist`): the effect is listed.
    pub fn watches(&self, effect: u32) -> bool {
        self.watch.contains(&effect)
    }
}

fn test(
    e: &Entry,
    world: &World,
    me: &crate::world::Object,
    f: &mut Friend,
    env: &dyn Env,
) -> bool {
    match e.action {
        Action::Battle | Action::Follow => true,
        // FUN_1401b3310(Sora, 0.25): HP under (int)(MaxHP × 0.25).
        Action::HealPlayer => env
            .player_hp()
            .is_some_and(|(hp, max)| hp < (max as f32 * 0.25) as i32),
        a => stitch::test(a, world, me, f, env),
    }
}

fn run(
    e: &Entry,
    world: &World,
    me: &mut crate::world::Object,
    f: &mut Friend,
    env: &dyn Env,
    out: &mut Requests,
) {
    let _ = world;
    match e.action {
        Action::Battle => out.label = Some(b"battle"),
        Action::Follow => follow_player(me, f, env),
        Action::HealPlayer => {
            f.band = 0;
            out.command = Some((1, PLAYER_OBJECT));
        }
        a => stitch::run(a, me, f, env),
    }
}

/// FUN_1401b1b20: the wish (direction × speed) goes into a ring of six; the
/// wish becomes the mean of itself and the ring's average, and its speed
/// snaps to 0, walk or run.
fn smooth(f: &mut Friend) {
    let w = f.wish;
    let v = [w[0] * w[3], w[1] * w[3], w[2] * w[3], 1.];
    let v = if f.flags & flag::IDLE_TURN != 0 {
        [0., 0., 0., 0.]
    } else {
        v
    };
    if f.ring_index < 0 {
        f.ring = [v; 6];
        f.ring_index = 0;
    } else {
        f.ring_index = (f.ring_index + 1) % 6;
        f.ring[f.ring_index as usize] = v;
    }
    let mut sum = [0.; 3];
    for r in &f.ring {
        for k in 0..3 {
            sum[k] += r[k];
        }
    }
    let avg_speed = len(sum) / 6.;
    let avg_dir = unit(sum);
    let dir = unit([
        (w[0] + avg_dir[0]) * 0.5,
        (w[1] + avg_dir[1]) * 0.5,
        (w[2] + avg_dir[2]) * 0.5,
    ]);
    let mut speed = (w[3] + avg_speed) * 0.5;
    if speed < f.run {
        speed = if speed < f.walk * 0.5 {
            0.
        } else if speed >= (f.run + f.walk) * 0.5 {
            f.run
        } else {
            f.walk
        };
    }
    if f.flags & flag::IDLE_TURN != 0 {
        speed = f.walk;
    }
    let dir = if len(dir) > 0. {
        dir
    } else {
        [w[0], w[1], w[2]]
    };
    f.wish = [dir[0], dir[1], dir[2], speed];
}

fn len(v: Vec3) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
fn unit(v: Vec3) -> Vec3 {
    let l = len(v);
    if l > 0. { v.map(|x| x / l) } else { [0.; 3] }
}
fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn dist2(a: Vec3, b: Vec3) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    dot(d, d)
}

#[cfg(test)]
mod tests;
