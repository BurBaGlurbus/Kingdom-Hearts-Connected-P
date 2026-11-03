//! Reaction commands an object offers through its own collision, translated
//! from the executable (Steam build 15194255).
//!
//! A model's collision table (type-23 entry) can hold REACTION shapes (kind 5,
//! the shape's argument is the command id; kind 14 offers the command a
//! clashed attack left at +0xcf0). Whether a shape's group is switched on, and
//! whether its command is offered, is decided per tick by FUN_1403ba600 from
//! four sources, in this order:
//!
//! 1. command entries scripts set (+0x540, traps 1:49/50/51, persistent);
//! 2. command entries motion ranges 6/7 set (+0x4e0, cleared every tick);
//! 3. group bits scripts set (+0x5a8, traps 1:52/53/54, persistent; kept in
//!    [`crate::sysobj::Sysobj::collision`]);
//! 4. group bits motion ranges 4/5 set (+0x5a0, cleared every tick);
//!
//! and finally the table's default group mask (its u32 at +4).

/// One command entry (12 bytes: command, group (-1 = any), enabled).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub command: i32,
    pub group: i32,
    pub enabled: bool,
}

/// Eight entries, as both arrays hold.
pub type Entries = [Entry; 8];

/// FUN_14040a590 (`enabled` true) / FUN_14040a500 (false): the entry with
/// this command and group, else the first free one (command 0), takes the
/// flag. [O] With all eight in use the executable writes through a null
/// pointer; here nothing changes.
pub fn set(entries: &mut Entries, command: i32, group: i32, enabled: bool) {
    let slot = entries
        .iter()
        .position(|e| e.command == command && e.group == group)
        .or_else(|| entries.iter().position(|e| e.command == 0));
    if let Some(i) = slot {
        entries[i] = Entry {
            command,
            group,
            enabled,
        };
    }
}

/// FUN_14040a6e0: the matching entry's command is cleared (the slot is free
/// again; group and flag stay).
pub fn reset(entries: &mut Entries, command: i32, group: i32) {
    if let Some(e) = entries
        .iter_mut()
        .find(|e| e.command == command && e.group == group)
    {
        e.command = 0;
    }
}

/// FUN_14040a660 (`enabled` true) / FUN_14040a620 (false): an entry for this
/// command whose group is -1 or `group` holds that flag.
pub fn holds(entries: &Entries, command: i32, group: i32, enabled: bool) -> bool {
    entries.iter().any(|e| {
        e.command == command && (e.group == -1 || e.group == group) && e.enabled == enabled
    })
}

/// The per-object reaction state (+0x4e0, +0x540, +0x5a0).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    /// +0x540: script entries.
    pub script: Entries,
    /// +0x4e0: motion entries.
    pub motion: Entries,
    /// +0x5a0: the motion's {enable, disable} group bits.
    pub motion_bits: [u32; 2],
}

impl Groups {
    /// FUN_14040e5c0's part: FUN_1403f1f70 zeroes +0x5a0, FUN_14040a4e0
    /// clears the +0x4e0 array.
    pub fn clear_motion(&mut self) {
        self.motion = Entries::default();
        self.motion_bits = [0; 2];
    }

    /// Motion range kinds 4..7 (FUN_14040d810): 4 enables group `p0`, 5
    /// disables it, 6 enables command `p0` for group `p1`, 7 disables it.
    pub fn range_event(&mut self, kind: u8, p0: u16, p1: u16) {
        let (p0, p1) = (i32::from(p0 as i16), i32::from(p1 as i16));
        match kind {
            4 => self.motion_bits[0] |= bit(p0),
            5 => self.motion_bits[1] |= bit(p0),
            6 => set(&mut self.motion, p0, p1, true),
            7 => set(&mut self.motion, p0, p1, false),
            _ => {}
        }
    }

    /// FUN_1403ba550: whether collision group `group` is on, from the
    /// script's bits (`script_bits`, +0x5a8 {enable, disable}), the motion's
    /// and the table's default mask.
    pub fn group_on(&self, script_bits: [u32; 2], default_mask: u32, group: i32) -> bool {
        let b = bit(group);
        if script_bits[0] & b != 0 {
            return true;
        }
        if script_bits[1] & b != 0 {
            return false;
        }
        if self.motion_bits[0] & b != 0 {
            return true;
        }
        if self.motion_bits[1] & b != 0 {
            return false;
        }
        default_mask & b != 0
    }

    /// FUN_1403ba600: whether a reaction shape of `group` may offer
    /// `command`.
    pub fn offered(
        &self,
        script_bits: [u32; 2],
        default_mask: u32,
        command: i32,
        group: i32,
    ) -> bool {
        if holds(&self.script, command, group, true) {
            return true;
        }
        if holds(&self.script, command, group, false) {
            return false;
        }
        if holds(&self.motion, command, group, true) {
            return true;
        }
        if holds(&self.motion, command, group, false) {
            return false;
        }
        self.group_on(script_bits, default_mask, group)
    }
}

/// Group `g` as a bit of a group word (groups past 31 have none).
fn bit(g: i32) -> u32 {
    u32::try_from(g)
        .ok()
        .and_then(|g| 1u32.checked_shl(g))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_entries_outrank_motion_entries_and_group_bits() {
        let mut g = Groups::default();
        // Group 3 is off by default (mask 1); range 4 [3] turns it on.
        assert!(!g.offered([0; 2], 1, 664, 3));
        g.range_event(4, 3, 0);
        assert!(g.offered([0; 2], 1, 664, 3));
        // The script's disable bit outranks the motion's enable.
        assert!(!g.offered([0, 1 << 3], 1, 664, 3));
        // A script entry disabling the command outranks every bit.
        set(&mut g.script, 664, -1, false);
        assert!(!g.offered([1 << 3, 0], 1, 664, 3));
        reset(&mut g.script, 664, -1);
        assert!(g.offered([1 << 3, 0], 1, 664, 3));
        g.clear_motion();
        assert!(!g.offered([0; 2], 1, 664, 3));
    }

    #[test]
    fn entries_reuse_their_slot_and_take_the_first_free_one() {
        let mut e = Entries::default();
        set(&mut e, 10, 2, true);
        set(&mut e, 11, 2, true);
        set(&mut e, 10, 2, false);
        assert_eq!(
            e[0],
            Entry {
                command: 10,
                group: 2,
                enabled: false
            }
        );
        assert!(holds(&e, 10, 2, false));
        assert!(!holds(&e, 10, 3, false));
        reset(&mut e, 10, 2);
        set(&mut e, 12, -1, true);
        assert_eq!(e[0].command, 12, "the freed slot is the first free one");
        assert!(holds(&e, 12, 7, true), "group -1 matches any group");
        for k in 0..20 {
            set(&mut e, 100 + k, 0, true);
        }
        assert_eq!(e.iter().filter(|x| x.command != 0).count(), 8);
    }
}
