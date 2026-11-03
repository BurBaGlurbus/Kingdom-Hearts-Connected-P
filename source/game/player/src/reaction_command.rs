//! Sora's reaction command, translated from the executable (Steam build
//! 15194255): the command manager's search for the Triangle slot
//! (FUN_140408520 → FUN_1404081c0), its candidates, the slot it keeps and
//! the press that takes it (FUN_1403b2340).
//!
//! Every tick a search builds one candidate record (FUN_1404020e0) and
//! offers it, in this order, everything that may show in the slot:
//!
//! 1. the command Sora last took with Triangle (+0xdd8), priority 0xc9, mode
//!    8 (the "taken" look);
//! 2. the command a script, a limit or a summon offered on the manager's
//!    record 1 (FUN_1403b2660(mgr, 1)), priority 0xcb;
//! 3. the partner's command (+0x18c bit 20, +0x638, +0x2b0), priority 0xcc,
//!    mode 7;
//! 4. every object's REACTION shapes (FUN_140407de0) and every attack that
//!    carries a command (FUN_140407b40), at the record's priority (0xca for
//!    the state the command is locked to);
//! 5. with nothing found: Auto Summon (a party member down, the second deck
//!    up), the auto forms (HP at a quarter or less) and Auto Limit (a member
//!    within 500), each picked at random among the usable ones.
//!
//! A candidate beats the best by a higher priority, or by a lower score at the
//! same priority (FUN_140408080). The score is the horizontal distance,
//! shortened for the target already shown and more for the locked-on one; bit
//! 30 of the score marks a command that may not be used now (drawn dimmed).
//! The slot (FUN_140408520) keeps the winner and the target marker over it.

/// Bit 30 of a score: the candidate is shown dimmed.
pub const DIMMED: i32 = 0x4000_0000;

/// The priorities the driver gives its own sources (FUN_1404081c0).
pub const HOLD: u8 = 0xc9;
pub const STATE: u8 = 0xca;
pub const OFFERED: u8 = 0xcb;
pub const PARTNER: u8 = 0xcc;

/// Display modes the driver sets on its own sources.
pub const MODE_DIM: u8 = 5;
pub const MODE_PARTNER: u8 = 7;
pub const MODE_HOLD: u8 = 8;

/// 0x1405b1c50: the slot's RGB multiplier by mode.
pub const MODE_RGB: [f32; 10] = [1., 1., 1., 1., 0.6, 0.6, 0.6, 1., 1., 0.95];

/// 0x1405cca14: the target marker's `fiel` group by the record's mark (+0x24).
pub const MARKERS: [u16; 10] = [0, 0x32, 0x34, 0x36, 0x38, 0x3a, 0x3c, 0x3e, 0x40, 0x42];

/// 0x1405cca40: the auto-form abilities (Auto Final, Master, Limit, Wisdom,
/// Valor), offered while HP is at a quarter or less (FUN_1404079a0).
pub const AUTO_FORMS: [u16; 5] = [0x184, 0x183, 0x238, 0x182, 0x181];
/// 0x1405cca50: the summon charms Auto Summon offers (Ukulele, Lamp, Feather,
/// Baseball; FUN_140407a90).
pub const AUTO_SUMMONS: [u16; 4] = [0x19, 0x9f, 0xa0, 0x17f];
/// 0x1405b4fe8: how near a member must be for Auto Limit (FUN_1403e7950).
pub const AUTO_LIMIT_DISTANCE: f32 = 500.;
/// The Auto Limit ability (item 0x1a1; a member's status bit 129).
pub const AUTO_LIMIT: u16 = 0x1a1;

/// 0x140585c7c and 0x140623a4c: the score factors for the target already
/// shown and for the locked-on one (FUN_1404088f0).
const SHOWN_FACTOR: f32 = 0.8;
const LOCKED_FACTOR: f32 = 0.5;

/// A command record's fields the search reads (03system `cmd`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Row {
    pub id: u16,
    /// +2: the dispatcher's case; a record whose +2 is not its own id takes
    /// its form mask from that record (FUN_1403b6cb0).
    pub exec: u16,
    /// +4.
    pub argument: i16,
    /// +6: the submenu (-1 none).
    pub submenu: i8,
    /// +0xc.
    pub flags: u32,
    /// +0x10, +0x14, +0x18: Sora's facing gate, the target's yaw offset and
    /// the target's facing gate (cosines; -1 = none).
    pub facing_cos: f32,
    pub target_yaw: f32,
    pub target_cos: f32,
    /// +0x1c: the cost the press stores in mgr+0xd.
    pub cost: u8,
    /// +0x1d: the camera on execute.
    pub camera: u8,
    /// +0x1e.
    pub priority: u8,
    /// +0x1f: whose windows decide the press (FUN_1403b6950): 0 Sora's, 1
    /// the target's, 2 both.
    pub decider: u8,
    /// +0x22: the state id a state-locked command needs (Sora +0x638).
    pub chain: u16,
    /// +0x24: the target marker (index into [`MARKERS`]).
    pub mark: u8,
    /// +0x25: what execution does to Sora's current action.
    pub action: u8,
    /// +0x28: past this horizontal distance the offer is dimmed (0 none).
    pub distance: u16,
    /// +0x2c: forms that cannot use the command (bit f).
    pub form_mask: u16,
    /// +0x2e: the command group (bit g of mgr+0xbe4 locks it).
    pub group: u8,
}

/// An object reference (FUN_1403bee60 / FUN_1403bee00): the object, the
/// shape group it was found through, and for an attack-borne offer the
/// attack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Target {
    pub owner: u32,
    pub group: u8,
    pub attack: Option<u32>,
}

/// Where a candidate came from, so the host can carry the press out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    None,
    /// Sora's last taken command (+0xdd8).
    Hold,
    /// The manager's record 1.
    Offered,
    /// The partner (+0x2b0).
    Partner,
    /// A REACTION shape or an attack's.
    Shape,
    /// Auto Summon, an auto form or Auto Limit (the item that offered it).
    Auto(u16),
}

/// FUN_140402130's candidate record (+0 flags, +2 command, +4 kind, +5 mode,
/// +8 the target's reference).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Candidate {
    /// Bit 1: the slot restarts its art (FUN_140408520).
    pub flags: u16,
    pub command: u16,
    pub kind: u8,
    pub mode: u8,
    pub target: Target,
    pub source: Source,
}

impl Candidate {
    /// FUN_1404020e0: an empty record (kind 0, mode 5).
    pub fn empty() -> Self {
        Self {
            mode: MODE_DIM,
            ..Self::default()
        }
    }

    /// FUN_140402120.
    pub fn valid(&self) -> bool {
        self.command != 0
    }

    /// FUN_140402110: a refused press buzzes in modes 5 and 6.
    pub fn refused(&self) -> bool {
        self.mode.wrapping_sub(5) < 2
    }

    pub fn dimmed(&self) -> bool {
        self.mode >= 4
    }
}

/// Sora's side of the search.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sora {
    /// The centre, radius and height of his OBJ shape (collision kind 1).
    pub centre: [f32; 3],
    pub radius: f32,
    pub height: f32,
    /// His heading about Y (forward is (sin, 0, cos)).
    pub yaw: f32,
    /// The battle flag (DAT_142a11404 != 0).
    pub battle: bool,
    /// FUN_1403ba9a0: a no-control motion or state (+0x18c & 0x1000020,
    /// +0x120 bit 23, the controller flag).
    pub no_control: bool,
    /// Motion range 0x1a's state (+0x18c bit 21) and its value (+0x638).
    pub state: Option<u16>,
    /// The Drive form in effect (+0xde0; bit `form` of a record's mask
    /// disables it, FUN_1403b6cb0; 0 without a form).
    pub form: u8,
    /// Anti form (+0xde0 == 0x0c): no reaction command at all.
    pub anti: bool,
    /// The class byte of his model's object entry (+7): 1 Sora, 0xe Roxas,
    /// 0x14 the lion, 0x1b Mickey.
    pub class: u8,
    /// A limit runs (+0x6cb bit 0 with +0xe14).
    pub limit: bool,
    /// DAT_142a11400 bit 1: the battle is won (commands are locked).
    pub won: bool,
    /// FUN_1403ba4c0 on Sora, or DAT_142a11400 bit 14: no search.
    pub disabled: bool,
    /// +0x6c8 bit 7: the controlled leader.
    pub leader: bool,
    /// +0xdd8: the command he last took with Triangle, and its target
    /// (+0xbf8).
    pub hold: Option<(u16, Target)>,
    /// +0x18c bit 20: the partner's command (+0x638) and the partner.
    pub partner: Option<(u16, u32)>,
    /// A party limit runs (DAT_142a24cc0) or +0x6c8 bit 25: no shapes.
    pub no_shapes: bool,
    /// +0x6c8 bit 29 with the lock mode 2: only the partner's shapes.
    pub partner_only: Option<u32>,
    /// mgr+0xbe4: the locked command groups (FUN_1403f29c0).
    pub group_lock: u32,
}

/// One REACTION shape in the world, or an attack carrying a command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// Who offers it (the host's object key) and the shape's group.
    pub owner: u32,
    pub group: u8,
    pub command: u16,
    pub centre: [f32; 3],
    pub radius: f32,
    pub height: f32,
    /// The owner's heading about Y.
    pub owner_yaw: f32,
    /// An attack-borne offer (FUN_140407b40): the attack's id.
    pub attack: Option<u32>,
}

impl Shape {
    pub fn target(&self) -> Target {
        Target {
            owner: self.owner,
            group: self.group,
            attack: self.attack,
        }
    }
}

/// FUN_140407de0: the command of a REACTION entry (kind 5 its argument, kind
/// 14 the command a clashed attack left at +0xcf0; argument 0x83 takes the
/// spawn record's +0x2c when the object has one).
pub fn shape_command(kind: u8, argument: u16, clash: u16, spawn: Option<u16>) -> u16 {
    if kind != 5 {
        return clash;
    }
    match spawn {
        Some(c) if argument == 0x83 => c,
        _ => argument,
    }
}

/// An auto ability's record (03system `item`, FUN_1403e39a0): its kind (0x13
/// an ability, 0x14 an item), whether Sora has it (the ability equipped,
/// FUN_1403d7470 bit; the item owned, FUN_1403c4720) and its command (+0x10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoItem {
    pub kind: u8,
    pub held: bool,
    pub command: u16,
}

/// What the search asks of the world.
pub trait Env {
    /// FUN_1403b68b0.
    fn row(&self, id: u16) -> Option<Row>;
    /// FUN_140401d60's switch on the record's dispatcher case, before the
    /// shared tail ([`exec_mode`] is the case for records without state).
    fn exec_mode(&self, row: &Row) -> u8 {
        exec_mode(row)
    }
    /// FUN_1403ba600: the owner offers this command through this group.
    fn offered(&self, shape: &Shape) -> bool;
    /// FUN_1403ba4c0 on an object: inactive (dead, hidden, not in play).
    fn inactive(&self, _owner: u32) -> bool {
        false
    }
    /// FUN_1403ad500: an event slot is busy (refuses commands 0x1e, 0x25c).
    fn event_busy(&self) -> bool {
        false
    }
    /// FUN_1403e39a0 for the auto tables.
    fn auto_item(&self, _item: u16) -> Option<AutoItem> {
        None
    }
}

/// FUN_140401d60's switch for records whose case reads no state: case 0
/// (and the empty records 0x67, 0x68) is 5, the rest 0.
pub fn exec_mode(row: &Row) -> u8 {
    match row.exec {
        0 | 0x67 | 0x68 => MODE_DIM,
        _ => 0,
    }
}

/// FUN_140401d60's shared tail: 5 while the command may not be used now
/// (FUN_1403b6cf0: a no-control-gated command while Sora has no control, a
/// battle-only command (+0xe bit 0) outside battle, any command without flag
/// bit 5 once the
/// battle is won; FUN_1403b6cb0: the form's bit of the mask; FUN_1403f29c0:
/// the command's group is locked).
pub fn mode(env: &impl Env, row: &Row, sora: &Sora) -> u8 {
    let locked = row.flags & 2 != 0 && sora.no_control
        || row.flags & 0x1_0000 != 0 && !sora.battle
        || row.flags & 0x20 == 0 && sora.won
        || form_masked(env, row, sora.form)
        || row.group != 0 && sora.group_lock >> (row.group & 0x1f) & 1 != 0;
    if locked { MODE_DIM } else { env.exec_mode(row) }
}

/// FUN_1403b6cb0: follows the record's +2 to the record that holds the mask.
pub fn form_masked(env: &impl Env, row: &Row, form: u8) -> bool {
    let mut r = *row;
    let mut hops = 0;
    while r.id != r.exec && hops < 16 {
        let Some(next) = env.row(r.exec) else { break };
        r = next;
        hops += 1;
    }
    u32::from(r.form_mask) >> (form & 0x1f) & 1 != 0
}

/// FUN_1403c94c0 with FUN_1403c9890: a sphere against an ellipsoid, both
/// given as centre, radius and height (half the vertical extent).
pub fn shapes_touch(a: [f32; 3], ra: f32, ha: f32, b: [f32; 3], rb: f32, hb: f32) -> bool {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    let (r, h) = (ra + rb, ha + hb);
    if r <= 0. || h <= 0. {
        return false;
    }
    (dx * dx + dz * dz) / (r * r) + dy * dy / (h * h) < 1.
}

/// FUN_1403b6a10: whether Sora's model may use the command at all.
pub fn form_allows(row: &Row, sora: &Sora) -> bool {
    if sora.anti {
        return false;
    }
    if sora.limit {
        return row.flags & 0x8000 == 0;
    }
    match sora.class {
        1 => row.flags & 0x400 == 0,
        0xe => row.flags & 0x800 == 0,
        0x14 => row.flags & 0x1000 == 0,
        0x1b => row.flags & 0x2000 == 0,
        _ => true,
    }
}

/// FUN_1404088f0: the horizontal distance (shortened by 0.8 for the target
/// already shown, by 0.5 for the locked-on one when Sora leads), with bit 30
/// set when a facing gate fails or a no-control-gated command meets Sora
/// without control.
pub fn score(row: &Row, sora: &Sora, shape: &Shape, shown: bool, locked: bool) -> i32 {
    let d = horizontal(sora.centre, shape.centre);
    let mut s = d as i32;
    if sora.leader && locked {
        s = (s as f32 * LOCKED_FACTOR) as i32;
    } else if shown {
        s = (s as f32 * SHOWN_FACTOR) as i32;
    }
    s = s.min(0x3fff_ffff);
    let to = [
        shape.centre[0] - sora.centre[0],
        shape.centre[2] - sora.centre[2],
    ];
    let len = to[0].hypot(to[1]);
    let unit = if len > 0. {
        [to[0] / len, to[1] / len]
    } else {
        [0., 0.]
    };
    let facing = [sora.yaw.sin(), sora.yaw.cos()];
    let mut dim =
        row.facing_cos > -1. && unit[0] * facing[0] + unit[1] * facing[1] < row.facing_cos;
    if row.target_cos > -1. {
        // The target's heading turned by the row's offset plus pi: Sora must
        // stand inside its cone.
        let a = shape.owner_yaw + row.target_yaw + std::f32::consts::PI;
        dim |= unit[0] * a.sin() + unit[1] * a.cos() < row.target_cos;
    }
    dim |= row.flags & 2 != 0 && sora.no_control;
    if dim { s | DIMMED } else { s }
}

fn horizontal(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// The best candidate of a search (the stack record P of FUN_140408520).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offer {
    pub candidate: Candidate,
    pub score: i32,
    pub priority: u8,
}

impl Offer {
    pub fn command(&self) -> u16 {
        self.candidate.command
    }

    pub fn dimmed(&self) -> bool {
        self.candidate.dimmed()
    }
}

/// What the auto tail of FUN_1404081c0 reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Auto {
    /// The manager's current menu kind (*(mgr+0xaf0)): the tail runs on the
    /// top deck (0) and the second deck (6) only (FUN_1403b1800).
    pub menu_kind: Option<u8>,
    /// +0x18c bit 14: no auto offer.
    pub blocked: bool,
    /// Sora has a status record (+0x5c0).
    pub status: bool,
    /// The status's ability bit 56 (+0x1d7 bit 0): Auto Summon.
    pub auto_summon: bool,
    /// A party member in play is down (+0x9b8 bit 2).
    pub member_down: bool,
    /// FUN_1403c1d30: HP at a quarter of the maximum or less.
    pub hp_low: bool,
    /// FUN_1403e75d0: Auto Limit may offer now.
    pub limit_ready: bool,
    /// The limits Auto Limit may offer (FUN_1403e7950 after FUN_1403e81a0,
    /// FUN_1403e7e10, the member's Auto Limit bit and the distance): their
    /// commands, each with the member as target.
    pub limits: Vec<(u16, Target)>,
}

/// The search's inputs besides the world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inputs {
    pub sora: Sora,
    /// Sora as a target (for the auto offers).
    pub sora_target: Target,
    pub shapes: Vec<Shape>,
    pub attacks: Vec<Shape>,
    /// The manager's record 1 (FUN_1403b2660(mgr, 1)).
    pub offered: Option<(u16, Target)>,
    /// The slot's command and target (rcState+0x32 and +0x18).
    pub shown: Option<(u16, Target)>,
    /// The lock-on target (Sora +0xdc8 + 4).
    pub locked: Option<u32>,
    pub auto: Auto,
}

struct Search<'a, E: Env> {
    env: &'a E,
    inputs: &'a Inputs,
    best: Option<Offer>,
}

impl<E: Env> Search<'_, E> {
    fn best_priority(&self) -> i32 {
        self.best.map_or(0, |b| i32::from(b.priority))
    }

    fn best_score(&self) -> i32 {
        self.best.map_or(-1, |b| b.score)
    }

    /// FUN_140408080: the candidate replaces the best unless it ranks lower,
    /// or (but for the hold) the model may not use it, an event slot is busy
    /// for 0x1e / 0x25c, or its object is inactive (but for 0x1c8).
    fn consider(
        &mut self,
        row: &Row,
        target: Target,
        priority: u8,
        score: i32,
        source: Source,
    ) -> bool {
        let p = i32::from(priority);
        if p < self.best_priority() || p == self.best_priority() && self.best_score() <= score {
            return false;
        }
        if priority != HOLD {
            if !form_allows(row, &self.inputs.sora) {
                return false;
            }
            if (row.id == 0x1e || row.id == 0x25c) && self.env.event_busy() {
                return false;
            }
            if row.id != 0x1c8 && target != Target::default() && self.env.inactive(target.owner) {
                return false;
            }
        }
        self.store(row, target, priority, score, source);
        true
    }

    /// FUN_140408bd0: stores the candidate (FUN_140402130 sets its mode), and
    /// past the record's distance range dims it.
    fn store(&mut self, row: &Row, target: Target, priority: u8, score: i32, source: Source) {
        let mut score = score;
        if row.distance != 0 && score < DIMMED {
            let at = self.target_point(&target);
            if let Some(at) = at
                && horizontal(self.inputs.sora.centre, at) > f32::from(row.distance)
            {
                score |= DIMMED;
            }
        }
        self.best = Some(Offer {
            candidate: Candidate {
                flags: 0,
                command: row.id,
                kind: 1,
                mode: mode(self.env, row, &self.inputs.sora),
                target,
                source,
            },
            score,
            priority,
        });
    }

    fn target_point(&self, target: &Target) -> Option<[f32; 3]> {
        let inputs = self.inputs;
        inputs
            .shapes
            .iter()
            .chain(&inputs.attacks)
            .find(|s| s.target() == *target)
            .map(|s| s.centre)
    }

    /// FUN_140407de0 (`attack` false) and FUN_140407b40 (true) for one shape.
    fn shape(&mut self, shape: &Shape, attack: bool) {
        let inputs = self.inputs;
        let sora = &inputs.sora;
        if shape.command == 0 || !attack && !self.env.offered(shape) {
            return;
        }
        let Some(row) = self.env.row(shape.command) else {
            return;
        };
        if sora.battle && row.flags & 0x10 == 0 {
            return;
        }
        let state_ok = row.chain != 0 && sora.state == Some(row.chain);
        if row.flags & 0x40 != 0 && !state_ok {
            return;
        }
        if !shapes_touch(
            shape.centre,
            shape.radius,
            shape.height,
            sora.centre,
            sora.radius,
            sora.height,
        ) {
            return;
        }
        let mut priority = row.priority;
        if priority < STATE && state_ok {
            priority = STATE;
        }
        let target = shape.target();
        let s = score(
            &row,
            sora,
            shape,
            inputs.shown.is_some_and(|(_, t)| t == target),
            inputs.locked == Some(shape.owner),
        );
        self.consider(&row, target, priority, s, Source::Shape);
    }

    /// FUN_1404077f0: the usable commands of an auto table; the one already
    /// shown stays, else one at random.
    fn auto_table(&mut self, items: &[u16], seed: &mut u32) -> bool {
        let inputs = self.inputs;
        let mut usable = Vec::new();
        for &item in items {
            let Some(a) = self.env.auto_item(item) else {
                continue;
            };
            if (a.kind == 0x13 || a.kind == 0x14) && !a.held {
                continue;
            }
            let Some(row) = self.env.row(a.command) else {
                continue;
            };
            if mode(self.env, &row, &inputs.sora) >= 4 {
                continue;
            }
            if inputs.shown.map(|(c, _)| c) == Some(row.id) {
                self.store(&row, inputs.sora_target, 0, 0, Source::Auto(item));
                return true;
            }
            usable.push((row, item));
        }
        if usable.is_empty() {
            return false;
        }
        let (row, item) = usable[random_index(seed, usable.len())];
        self.store(&row, inputs.sora_target, 0, 0, Source::Auto(item));
        true
    }

    /// FUN_1403e7950: Auto Limit's usable limits; the one shown stays, else
    /// one at random.
    fn auto_limit(&mut self, seed: &mut u32) -> bool {
        let inputs = self.inputs;
        let auto = &inputs.auto;
        if !auto.limit_ready {
            return false;
        }
        // FUN_140407a00: the offer's target is Sora.
        let mut usable = Vec::new();
        for &(command, _member) in &auto.limits {
            let Some(row) = self.env.row(command) else {
                continue;
            };
            if mode(self.env, &row, &inputs.sora) >= 4 {
                continue;
            }
            if inputs.shown.map(|(c, _)| c) == Some(row.id) {
                self.store(&row, inputs.sora_target, 0, 0, Source::Auto(AUTO_LIMIT));
                return true;
            }
            usable.push(row);
        }
        if usable.is_empty() {
            return false;
        }
        let row = usable[random_index(seed, usable.len())];
        self.store(&row, inputs.sora_target, 0, 0, Source::Auto(AUTO_LIMIT));
        true
    }
}

/// The draw of `DAT_140783ca0` (×0x10dcd) scaled to `n` choices as
/// FUN_1404077f0 and FUN_1403e7950 do.
pub fn random_index(seed: &mut u32, n: usize) -> usize {
    *seed = seed.wrapping_mul(0x10dcd);
    let n = n as u64;
    // (n - 1 + 2^32) / n in the executable.
    let step = 0x1_0000_0000_u64.div_ceil(n);
    ((u64::from(*seed) / step) as usize).min(n as usize - 1)
}

/// FUN_1404081c0: the best offer this tick. `seed` is the shared random
/// state the auto tail draws from.
pub fn search(env: &impl Env, inputs: &Inputs, seed: &mut u32) -> Option<Offer> {
    let sora = &inputs.sora;
    if sora.disabled {
        return None;
    }
    let mut s = Search {
        env,
        inputs,
        best: None,
    };
    // a) The command taken last, at 0xc9 in mode 8.
    if sora.leader
        && let Some((cmd, target)) = sora.hold
        && s.best_priority() < i32::from(STATE)
        && (s.best_priority() != i32::from(HOLD) || s.best_score() > 0)
        && let Some(row) = env.row(cmd)
    {
        s.store(&row, target, HOLD, 0, Source::Hold);
        if let Some(b) = s.best.as_mut() {
            b.candidate.mode = MODE_HOLD;
        }
    }
    // b) The manager's record 1 at 0xcb.
    if let Some((cmd, target)) = inputs.offered
        && let Some(row) = env.row(cmd)
    {
        s.consider(&row, target, OFFERED, 0, Source::Offered);
    }
    // c) The partner's command at 0xcc, mode 7.
    if let Some((cmd, partner)) = sora.partner
        && let Some(row) = env.row(cmd)
    {
        let target = Target {
            owner: partner,
            ..Target::default()
        };
        if s.consider(&row, target, PARTNER, 0, Source::Partner)
            && let Some(b) = s.best.as_mut()
        {
            b.candidate.mode = MODE_PARTNER;
        }
    }
    // d) The shapes, unless a party limit runs.
    if !sora.no_shapes {
        for shape in &inputs.shapes {
            if sora.partner_only.is_none_or(|p| p == shape.owner) {
                s.shape(shape, false);
            }
        }
        for shape in &inputs.attacks {
            s.shape(shape, true);
        }
        if let Some(b) = s.best.as_mut()
            && b.score >= 0
            && b.score & DIMMED != 0
        {
            b.candidate.mode = MODE_DIM;
        }
        // e) Nothing found: the auto offers, on the top or second deck only.
        let auto = &inputs.auto;
        if s.best.is_none()
            && sora.leader
            && !auto.blocked
            && matches!(auto.menu_kind, Some(0) | Some(6))
        {
            if auto.status
                && auto.auto_summon
                && auto.menu_kind == Some(6)
                && auto.member_down
                && s.auto_table(&AUTO_SUMMONS, seed)
            {
                return s.best;
            }
            let forms = auto.status && auto.hp_low && s.auto_table(&AUTO_FORMS, seed);
            if !forms {
                s.auto_limit(seed);
            }
        }
    }
    s.best
}

/// The persistent slot (rcState, mgr+0xb10): the shown candidate (mgr+0xb40)
/// and the target marker (FUN_140408520).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    pub shown: Candidate,
    /// The marker's mark index (rcState+8) and its creation count: a new
    /// count means a new marker sprite (FUN_14017c570); the previous one is
    /// released (FUN_14017bfe0).
    pub mark: u8,
    pub marker: Option<u32>,
    created: u32,
    /// rcState+0x2c bit 0: the search runs.
    pub off: bool,
}

/// What the slot reads besides the search.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SlotState {
    /// FUN_14039a8c0 false: the search is suspended this tick.
    pub suspended: bool,
    /// +0xde0 == 6, the model flag bit 15 or +0x194 bit 7: shown dimmed.
    pub force_dim: bool,
}

impl Slot {
    /// FUN_140408520 after the search: keeps the winner, re-creates the
    /// marker when the command or the target changes, and dims as the
    /// state says.
    pub fn update(&mut self, env: &impl Env, offer: Option<Offer>, state: SlotState) {
        if state.suspended || self.off {
            return;
        }
        match offer {
            None => {
                self.marker = None;
                self.mark = 0;
                self.shown = Candidate::empty();
            }
            Some(o) => {
                let mut c = o.candidate;
                let row = env.row(c.command).unwrap_or_default();
                if self.shown.command != c.command || self.shown.target != c.target {
                    self.marker = None;
                    self.mark = 0;
                }
                if self.shown.command == c.command
                    && self.shown.mode != c.mode
                    && self.shown.mode == MODE_HOLD
                    && row.flags & 8 != 0
                {
                    c.flags |= 2;
                }
                self.shown = c;
                let mark = if c.mode != MODE_HOLD { row.mark } else { 0 };
                if mark != self.mark {
                    self.marker = None;
                }
                if mark != 0 && self.marker.is_none() && usize::from(mark) < MARKERS.len() {
                    self.created += 1;
                    self.marker = Some(self.created);
                }
                self.mark = mark;
                if state.force_dim {
                    self.shown.mode = MODE_DIM;
                }
                if o.score > 0 && o.score & DIMMED != 0 {
                    self.shown.mode = MODE_DIM;
                }
            }
        }
    }

    /// The marker's `fiel` group, when one shows.
    pub fn marker_group(&self) -> Option<(u32, u16)> {
        let serial = self.marker?;
        Some((serial, *MARKERS.get(usize::from(self.mark))?))
    }
}

/// The manager's flags (mgr+0x24) the press reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Manager {
    /// Bit 0: no input at all.
    pub no_input: bool,
    /// Bit 1: the menu is locked; the slot shows dimmed.
    pub locked: bool,
}

/// What a Triangle press did (FUN_1403b2340).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// No reaction command shows: the press goes to the menu (vt+0x18).
    Menu,
    /// The command was taken: the request (FUN_1403d7f30: the command, the
    /// record's argument, the target), the cost (mgr+0xd), the burst on the
    /// HUD (FUN_14015d660) and the request's flag 0x10.
    Taken {
        command: u16,
        argument: i16,
        cost: u8,
    },
    /// A dimmed command (mode 5 or 6): the refusal sound (se 5).
    Refused,
    /// Shown but not usable now (FUN_1403b22d0 false), or mode 4.
    Ignored,
}

/// Whose state `can_act` reads (FUN_1403ba070).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Actor {
    /// FUN_1403ba9a0's parts (controller flag +0xc bit 0, +0x18c & 0x1000020,
    /// +0x120 bit 23).
    pub no_control: bool,
    /// +0x18c bit 17 with +0x638: a command the motion takes directly.
    pub takes: Option<u16>,
    /// +0x18c bit 21 with +0x638: the state a chained command needs.
    pub state: Option<u16>,
    /// +0x18c bit 30.
    pub bit30: bool,
    /// +0x18c bit 19 and bit 10, +0x194 bit 5.
    pub bit19: bool,
    pub bit10: bool,
    pub finisher: bool,
    /// +0xb94 above zero: commands locked out.
    pub lockout: bool,
    /// +0x158 == 0 (no motion controller) or +0x18c & 0x1c (a command
    /// window, motion ranges 0..2).
    pub window: bool,
}

/// FUN_1403ba070: whether `actor` may take the command now.
pub fn can_act(actor: &Actor, row: &Row, battle: bool) -> bool {
    if row.flags & 2 != 0 && actor.no_control {
        return false;
    }
    if actor.takes == Some(row.id) || row.chain != 0 && actor.state == Some(row.chain) {
        return true;
    }
    if actor.bit30 {
        return false;
    }
    if row.flags & 0x10 == 0 && battle {
        return false;
    }
    row.flags & 0x100 != 0 && actor.bit19
        || !actor.lockout
            && (actor.window
                || row.flags & 0x80 == 0 && (actor.bit10 || actor.finisher)
                || row.flags & 4 != 0)
}

/// FUN_1403b6950: the record's decider says whose windows count.
pub fn usable(row: &Row, sora: &Actor, target: Option<&Actor>, battle: bool) -> bool {
    match row.decider {
        0 => can_act(sora, row, battle),
        1 => target.is_some_and(|t| can_act(t, row, battle)),
        2 => can_act(sora, row, battle) && target.is_some_and(|t| can_act(t, row, battle)),
        _ => false,
    }
}

/// FUN_1403b2340's reaction part: a menu lock shows the slot dimmed;
/// Triangle takes a usable command of mode below 4 (FUN_1403b22d0), buzzes a
/// mode 5 or 6 one, and goes to the menu when nothing shows.
pub fn press(
    slot: &mut Slot,
    mgr: Manager,
    triangle: bool,
    row: Option<&Row>,
    usable: impl FnOnce(&Row) -> bool,
) -> Option<Press> {
    if mgr.no_input {
        return None;
    }
    if mgr.locked {
        slot.shown.mode = MODE_DIM;
    }
    if mgr.locked || !triangle {
        return None;
    }
    if !slot.shown.valid() {
        return Some(Press::Menu);
    }
    let Some(row) = row else {
        return Some(Press::Ignored);
    };
    if slot.shown.mode < 4 {
        if usable(row) {
            return Some(Press::Taken {
                command: row.id,
                argument: row.argument,
                cost: row.cost,
            });
        }
        return Some(Press::Ignored);
    }
    Some(if slot.shown.refused() {
        Press::Refused
    } else {
        Press::Ignored
    })
}

/// FUN_1403a89a0: a request Triangle made is held in +0xdd8 (the search
/// offers it again at 0xc9) unless the record's flag bit 9 says not to.
pub fn hold_after(row: &Row, by_triangle: bool) -> Option<u16> {
    (by_triangle && row.flags & 0x200 == 0).then_some(row.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct World {
        rows: HashMap<u16, Row>,
        on: bool,
        items: HashMap<u16, AutoItem>,
    }

    impl Env for World {
        fn row(&self, id: u16) -> Option<Row> {
            self.rows.get(&id).copied()
        }
        fn offered(&self, _: &Shape) -> bool {
            self.on
        }
        fn auto_item(&self, item: u16) -> Option<AutoItem> {
            self.items.get(&item).copied()
        }
    }

    fn block() -> Row {
        Row {
            id: 664,
            exec: 664,
            argument: 664,
            flags: 0xb917,
            facing_cos: -1.,
            target_yaw: 0.,
            target_cos: -1.,
            priority: 100,
            mark: 5,
            ..Default::default()
        }
    }

    fn world() -> World {
        World {
            rows: [(664, block())].into(),
            on: true,
            items: HashMap::new(),
        }
    }

    fn sora() -> Sora {
        Sora {
            centre: [0., 85., 0.],
            radius: 35.,
            height: 55.,
            battle: true,
            class: 1,
            leader: true,
            ..Default::default()
        }
    }

    fn sphere(owner: u32, at: [f32; 3]) -> Shape {
        Shape {
            owner,
            group: 3,
            command: 664,
            centre: at,
            radius: 3000.,
            height: 3000.,
            owner_yaw: 0.,
            attack: None,
        }
    }

    fn inputs(shapes: Vec<Shape>) -> Inputs {
        Inputs {
            sora: sora(),
            shapes,
            ..Default::default()
        }
    }

    #[test]
    fn a_wide_sphere_offers_its_command_in_battle_unless_its_group_is_off() {
        let mut w = world();
        let mut seed = 1;
        let i = inputs(vec![sphere(7, [0., 170., 750.])]);
        let o = search(&w, &i, &mut seed).unwrap();
        assert_eq!(
            (
                o.command(),
                o.candidate.target.owner,
                o.score,
                o.candidate.mode
            ),
            (664, 7, 750, 0)
        );
        // Locked on: half the distance.
        let mut l = i.clone();
        l.locked = Some(7);
        assert_eq!(search(&w, &l, &mut seed).unwrap().score, 375);
        // Out of the sphere.
        let far = inputs(vec![sphere(7, [0., 170., 3100.])]);
        assert!(search(&w, &far, &mut seed).is_none());
        w.on = false;
        assert!(search(&w, &i, &mut seed).is_none());
    }

    #[test]
    fn no_control_dims_a_flag_bit_1_command_and_the_lion_never_sees_it() {
        let w = world();
        let mut seed = 1;
        let mut i = inputs(vec![sphere(7, [0., 170., 750.])]);
        i.sora.no_control = true;
        let o = search(&w, &i, &mut seed).unwrap();
        assert!(o.dimmed());
        assert_eq!(o.score & !DIMMED, 750);
        i.sora.no_control = false;
        i.sora.class = 0x14;
        assert!(search(&w, &i, &mut seed).is_none());
    }

    #[test]
    fn higher_priority_wins_then_the_lower_score() {
        let mut w = world();
        let mut talk = block();
        talk.id = 31;
        talk.exec = 31;
        talk.priority = 120;
        w.rows.insert(31, talk);
        let mut seed = 1;
        let mut s = vec![sphere(1, [0., 170., 200.]), sphere(2, [0., 170., 900.])];
        s[1].command = 31;
        assert_eq!(
            search(&w, &inputs(s.clone()), &mut seed).unwrap().command(),
            31
        );
        s[1].command = 664;
        let o = search(&w, &inputs(s), &mut seed).unwrap();
        assert_eq!(o.candidate.target.owner, 1);
    }

    #[test]
    fn the_shown_target_scores_shorter_and_the_offered_record_outranks_shapes() {
        let mut w = world();
        let mut follow = block();
        follow.id = 0x2c4;
        follow.exec = 0x2c4;
        follow.priority = 0;
        w.rows.insert(0x2c4, follow);
        let mut seed = 1;
        let s = vec![sphere(1, [0., 170., 400.]), sphere(2, [0., 170., 450.])];
        let mut i = inputs(s);
        i.shown = Some((
            664,
            Target {
                owner: 2,
                group: 3,
                attack: None,
            },
        ));
        // 450 x 0.8 = 360 beats 400.
        assert_eq!(search(&w, &i, &mut seed).unwrap().candidate.target.owner, 2);
        i.offered = Some((0x2c4, Target::default()));
        let o = search(&w, &i, &mut seed).unwrap();
        assert_eq!(
            (o.command(), o.priority, o.candidate.source),
            (0x2c4, OFFERED, Source::Offered)
        );
    }

    #[test]
    fn the_taken_command_holds_the_slot_in_mode_8_until_a_higher_source() {
        let w = world();
        let mut seed = 1;
        let mut i = inputs(vec![sphere(7, [0., 170., 750.])]);
        let t = Target {
            owner: 7,
            group: 3,
            attack: None,
        };
        i.sora.hold = Some((664, t));
        let o = search(&w, &i, &mut seed).unwrap();
        assert_eq!(
            (o.priority, o.candidate.mode, o.candidate.source),
            (HOLD, MODE_HOLD, Source::Hold)
        );
        // A state-locked shape (0xca) takes over.
        let mut chained = block();
        chained.flags |= 0x40;
        chained.chain = 665;
        let mut w2 = world();
        w2.rows.insert(664, chained);
        i.sora.state = Some(665);
        let o = search(&w2, &i, &mut seed).unwrap();
        assert_eq!((o.priority, o.candidate.source), (STATE, Source::Shape));
    }

    #[test]
    fn the_partner_command_outranks_shapes_and_the_offered_record_in_mode_7() {
        let mut w = world();
        let mut catch = block();
        catch.id = 28;
        catch.exec = 28;
        catch.priority = 255;
        w.rows.insert(28, catch);
        let mut seed = 1;
        let mut i = inputs(vec![sphere(7, [0., 170., 750.])]);
        i.offered = Some((664, Target::default()));
        i.sora.partner = Some((28, 9));
        let o = search(&w, &i, &mut seed).unwrap();
        assert_eq!(
            (
                o.command(),
                o.priority,
                o.candidate.mode,
                o.candidate.target.owner
            ),
            (28, PARTNER, MODE_PARTNER, 9)
        );
        assert_eq!(o.candidate.source, Source::Partner);
    }

    #[test]
    fn a_form_mask_is_read_through_the_record_chain_and_a_group_lock_dims() {
        let mut w = world();
        let mut base = block();
        base.id = 0x298;
        base.exec = 0x298;
        base.form_mask = 1 << 2;
        let mut alias = block();
        alias.id = 700;
        alias.exec = 0x298;
        alias.group = 3;
        w.rows.insert(0x298, base);
        w.rows.insert(700, alias);
        let mut s = sora();
        assert_eq!(mode(&w, &alias, &s), 0);
        s.form = 2;
        assert_eq!(mode(&w, &alias, &s), MODE_DIM);
        s.form = 0;
        s.group_lock = 1 << 3;
        assert_eq!(mode(&w, &alias, &s), MODE_DIM);
    }

    #[test]
    fn low_hp_offers_an_owned_auto_form_and_keeps_the_one_shown() {
        let mut w = world();
        for (item, cmd) in [(0x181u16, 0x226u16), (0x182, 0x227)] {
            let mut r = block();
            r.id = cmd;
            r.exec = 4;
            w.rows.insert(cmd, r);
            w.items.insert(
                item,
                AutoItem {
                    kind: 0x13,
                    held: true,
                    command: cmd,
                },
            );
        }
        let mut i = inputs(vec![]);
        i.auto = Auto {
            menu_kind: Some(0),
            status: true,
            hp_low: true,
            ..Default::default()
        };
        let mut seed = 1;
        let o = search(&w, &i, &mut seed).unwrap();
        assert!(matches!(o.candidate.source, Source::Auto(0x181 | 0x182)));
        assert_eq!(o.priority, 0);
        i.shown = Some((0x226, Target::default()));
        let o = search(&w, &i, &mut seed).unwrap();
        assert_eq!(o.command(), 0x226);
        // Not in a submenu, not at full HP.
        i.auto.menu_kind = Some(1);
        assert!(search(&w, &i, &mut seed).is_none());
        i.auto.menu_kind = Some(0);
        i.auto.hp_low = false;
        assert!(search(&w, &i, &mut seed).is_none());
    }

    #[test]
    fn the_random_index_matches_the_executable_scaling() {
        let mut seed = 1;
        assert_eq!(random_index(&mut seed, 1), 0);
        assert_eq!(seed, 0x10dcd);
        let mut seed = 0xffff_ffff / 0x10dcd + 1;
        // A draw near the top of the range picks the last choice.
        let i = random_index(&mut seed, 4);
        assert!(i < 4);
    }

    #[test]
    fn the_slot_restarts_its_marker_on_a_new_target_and_hides_it_in_mode_8() {
        let w = world();
        let mut slot = Slot::default();
        let offer = |owner: u32, mode: u8| Offer {
            candidate: Candidate {
                command: 664,
                kind: 1,
                mode,
                target: Target {
                    owner,
                    group: 3,
                    attack: None,
                },
                ..Default::default()
            },
            score: 10,
            priority: 100,
        };
        slot.update(&w, Some(offer(1, 0)), SlotState::default());
        let first = slot.marker_group().unwrap();
        assert_eq!(first.1, 0x3a);
        slot.update(&w, Some(offer(1, 0)), SlotState::default());
        assert_eq!(slot.marker_group(), Some(first));
        slot.update(&w, Some(offer(2, 0)), SlotState::default());
        assert_ne!(slot.marker_group().unwrap().0, first.0);
        slot.update(&w, Some(offer(2, MODE_HOLD)), SlotState::default());
        assert!(slot.marker_group().is_none());
        slot.update(&w, None, SlotState::default());
        assert!(!slot.shown.valid());
        assert_eq!(slot.shown.mode, MODE_DIM);
    }

    #[test]
    fn triangle_takes_a_usable_command_and_buzzes_a_dimmed_one() {
        let row = block();
        let mut slot = Slot::default();
        assert_eq!(
            press(&mut slot, Manager::default(), true, None, |_| true),
            Some(Press::Menu)
        );
        slot.shown = Candidate {
            command: 664,
            mode: 0,
            ..Default::default()
        };
        assert_eq!(
            press(&mut slot, Manager::default(), true, Some(&row), |_| true),
            Some(Press::Taken {
                command: 664,
                argument: 664,
                cost: 0
            })
        );
        assert_eq!(
            press(&mut slot, Manager::default(), true, Some(&row), |_| false),
            Some(Press::Ignored)
        );
        let locked = Manager {
            locked: true,
            ..Default::default()
        };
        assert_eq!(press(&mut slot, locked, true, Some(&row), |_| true), None);
        assert_eq!(slot.shown.mode, MODE_DIM);
        assert_eq!(
            press(&mut slot, Manager::default(), true, Some(&row), |_| true),
            Some(Press::Refused)
        );
        assert_eq!(hold_after(&row, true), Some(664));
        let mut no_hold = row;
        no_hold.flags |= 0x200;
        assert_eq!(hold_after(&no_hold, true), None);
    }

    #[test]
    fn can_act_follows_the_window_lockout_and_the_record_flags() {
        let row = block();
        let mut a = Actor {
            window: true,
            ..Default::default()
        };
        assert!(can_act(&a, &row, true));
        a.lockout = true;
        assert!(!can_act(&a, &row, true));
        // Flag bit 2 lets the command through outside a window.
        a = Actor::default();
        assert!(can_act(&a, &row, true));
        let mut strict = row;
        strict.flags &= !4;
        assert!(!can_act(&a, &strict, true));
        a.no_control = true;
        assert!(!can_act(&a, &row, true));
        // A command only for the field (flag bit 4 clear) in battle.
        let mut field = row;
        field.flags &= !0x10;
        assert!(!can_act(
            &Actor {
                window: true,
                ..Default::default()
            },
            &field,
            true
        ));
    }

    #[test]
    fn the_ellipsoid_test_uses_both_extents() {
        assert!(shapes_touch([0.; 3], 10., 10., [0., 0., 19.], 10., 1.));
        assert!(!shapes_touch([0.; 3], 10., 10., [0., 0., 20.], 10., 1.));
        assert!(!shapes_touch([0.; 3], 10., 10., [0., 11., 0.], 10., 1.));
    }

    #[test]
    fn shape_commands_take_the_clash_and_the_spawn_record() {
        assert_eq!(shape_command(5, 664, 0, None), 664);
        assert_eq!(shape_command(5, 0x83, 0, Some(31)), 31);
        assert_eq!(shape_command(5, 0x83, 0, None), 0x83);
        assert_eq!(shape_command(14, 0, 0x55, None), 0x55);
    }
}
