//! A battler's equipment stat totals, translated from the PC executable's
//! status builder FUN_1403c1d90 (the stat part) and its per-item adder
//! FUN_1403c1190.
//!
//! Strength, Magic, Defense and AP start from the member's level row plus the
//! record's permanent bonuses (`status +0x188/+0x18a/+0x18c/+0x18e`). Every
//! equipped item then adds its stat row's four bytes (+4..+7) and scales the
//! seven resistance percents (`status +0x1a8..+0x1ae`, starting at 100) by its
//! bytes +8..+0xe, rounded up: `(row * current + 99) / 100` stored in a byte.

/// The stat row of one equipped item (03system stat table, FUN_14041e8f0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub strength: u8,
    pub magic: u8,
    pub defense: u8,
    pub ap: u8,
    /// Physical, Fire, Blizzard, Thunder, Dark, neutral, general (100 = none).
    pub resist: [u8; 7],
}

impl Row {
    /// An item with no stat row adds nothing (FUN_1403c1190 returns when
    /// FUN_1403e3ef0 finds none).
    pub const NONE: Row = Row {
        strength: 0,
        magic: 0,
        defense: 0,
        ap: 0,
        resist: [100; 7],
    };
}

/// The totals a status record holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Totals {
    pub strength: u16,
    pub magic: u16,
    pub defense: u16,
    pub ap: u16,
    pub resist: [u8; 7],
}

impl Totals {
    /// FUN_1403c1d90's start: the level row's stats plus the record's
    /// bonuses (16-bit adds), resists at 100.
    pub fn start(strength: u16, magic: u16, defense: u16, ap: u16) -> Self {
        Totals {
            strength,
            magic,
            defense,
            ap,
            resist: [100; 7],
        }
    }

    /// FUN_1403c1190 for one equipped item.
    pub fn add(&mut self, row: &Row) {
        self.strength = self.strength.wrapping_add(row.strength as u16);
        self.magic = self.magic.wrapping_add(row.magic as u16);
        self.defense = self.defense.wrapping_add(row.defense as u16);
        self.ap = self.ap.wrapping_add(row.ap as u16);
        for (have, scale) in self.resist.iter_mut().zip(row.resist) {
            *have = (scale as u32 * *have as u32).div_ceil(100) as u8;
        }
    }
}

/// FUN_1403eda00: the level row's AP; Critical difficulty (3) gives Sora
/// (character 1) and his Final-Form-less twin (0xe) one and a half times it.
pub fn level_ap(row_ap: u8, difficulty: u8, character: u16) -> u16 {
    let ap = row_ap as u32;
    if difficulty == 3 && (character == 1 || character == 0xe) {
        ((ap + ap * 2) >> 1) as u16
    } else {
        ap as u16
    }
}

/// The totals for a member: `start` plus every item's row in order (member
/// weapons, linked form weapon, armor, accessories), each [`Row::NONE`] when
/// the item has no stat row.
pub fn totals(start: Totals, rows: impl IntoIterator<Item = Row>) -> Totals {
    let mut t = start;
    for row in rows {
        t.add(&row);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(strength: u8, magic: u8, defense: u8, ap: u8, resist: [u8; 7]) -> Row {
        Row {
            strength,
            magic,
            defense,
            ap,
            resist,
        }
    }

    #[test]
    fn items_add_stats_and_scale_resists_upward() {
        let start = Totals::start(10, 4, 3, 6);
        let t = totals(
            start,
            [
                row(3, 1, 0, 0, [100; 7]),
                row(0, 0, 3, 2, [100, 80, 100, 100, 100, 100, 100]),
                row(0, 0, 1, 0, [100, 75, 100, 100, 100, 100, 100]),
            ],
        );
        assert_eq!((t.strength, t.magic, t.defense, t.ap), (13, 5, 7, 8));
        // 80% then 75%: (80*100+99)/100 = 80, (75*80+99)/100 = 60.
        assert_eq!(t.resist[1], 60);
        // Rounding is upward: 99% of 99 is 98.01, stored as 99.
        let mut r = Totals::start(0, 0, 0, 0);
        r.add(&row(0, 0, 0, 0, [99; 7]));
        r.add(&row(0, 0, 0, 0, [99; 7]));
        assert_eq!(r.resist, [99; 7]);
    }

    #[test]
    fn two_items_wrap_the_sixteen_bit_totals_and_a_byte_resist() {
        let mut t = Totals::start(u16::MAX, 0, 0, 0);
        t.add(&row(2, 0, 0, 0, [100; 7]));
        assert_eq!(t.strength, 1);
        // A row byte above 100 (a weakness) is stored modulo 256.
        let mut w = Totals::start(0, 0, 0, 0);
        w.resist = [200; 7];
        w.add(&row(0, 0, 0, 0, [200; 7]));
        assert_eq!(w.resist[0], (200u32 * 200).div_ceil(100) as u8);
    }

    #[test]
    fn critical_difficulty_raises_only_sora_level_ap() {
        assert_eq!(level_ap(7, 1, 1), 7);
        assert_eq!(level_ap(7, 3, 1), 10);
        assert_eq!(level_ap(7, 3, 0xe), 10);
        assert_eq!(level_ap(7, 3, 2), 7);
    }
}
