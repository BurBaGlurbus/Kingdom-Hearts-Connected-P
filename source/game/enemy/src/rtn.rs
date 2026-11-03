//! The routine action ("interrupt action") state of a townsperson-like
//! object, sysobj +0xc28..+0xcd4, which the `5:*` traps read. The engine's
//! routine update (FUN_1404179e0: zones, meetings between townspeople) raises
//! and ends these actions; this crate keeps the state and leaves that update
//! to the host.

use crate::world::ObjId;

/// One action: the block FUN_140416800 copies (+0xc28 and, saved, +0xc78).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RtnAction {
    /// +0xc28: the kind (3 look at a point, 4 a zone action, 5 a meeting).
    pub kind: i32,
    /// +0xc2c: what the object turns to (1 an object, 2 Sora, 3 a point).
    pub dir_kind: i32,
    /// +0xc30.
    pub word: u32,
    /// +0xc34: the object the action is with.
    pub partner: Option<ObjId>,
    /// +0xc38: the point to turn to.
    pub point: [f32; 4],
    /// +0xc48: the zone of a zone action.
    pub zone: Option<u32>,
    /// +0xc50: the direction changes.
    pub changed_dir: bool,
    /// +0xc54 and +0xc58: how far across the zone the action runs, and
    /// which way (the action is over once the walker is past it).
    pub threshold: f32,
    pub sign: f32,
    /// +0xc5c: the action has started; +0xc5d: it is forced.
    pub started: bool,
    pub forced: bool,
    /// +0xc60: the direction a look action holds.
    pub look: [f32; 4],
    /// +0xc70: the time left.
    pub timer: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rtn {
    pub action: RtnAction,
    /// +0xc78: the action the last `end` replaced.
    pub saved: RtnAction,
    /// +0xcc8: the wait before the next action.
    pub cooldown: u32,
    /// +0xcd0: the object `set_rtn_partner` named.
    pub partner: Option<ObjId>,
}

impl Rtn {
    /// FUN_140417e60: end the action. The ending is the same whichever the
    /// kind was (the kind is cleared before it is read): the wait before the
    /// next action is the last routine time parameter.
    pub fn end(&mut self, time_parameters: &[u32; 9]) {
        self.saved = self.action.clone();
        self.action = RtnAction::default();
        self.cooldown = time_parameters[8];
    }
}
