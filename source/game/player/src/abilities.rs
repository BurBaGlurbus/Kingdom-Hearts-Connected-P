//! A battler's ability record (battle stats +0x1d0), translated from the
//! PC executable: reset FUN_1404016b0, one ability FUN_140401730, and the
//! equipped-slot pass FUN_1404014a0.
//!
//! The record holds a 256-bit set (bit n = ability item +4 = n) and the
//! counters and rates that individual ability items add to. Offsets in the
//! field docs are relative to the record (stats +0x1d0).

/// SSTM battle parameters the ability adders read (03system `pref/sstm`,
/// `DAT_142ae5760`), by byte offset into the record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    /// +0x11c (u16, installed 2).
    pub w11c: u16,
    /// +0x120 / +0x124 / +0x128 (installed 0.25 / 0.5 / 0.75).
    pub f120: f32,
    pub f124: f32,
    pub f128: f32,
    /// +0x12c DrawRange (installed 125).
    pub draw_range: f32,
    /// +0x130 ComboDamageUp (installed 10).
    pub combo_damage_up: u16,
    /// +0x134 ReactionDamageUp (installed 50).
    pub reaction_damage_up: u16,
    /// +0x138 / +0x13c / +0x140 (installed 50 / 0.2 / 0.8).
    pub f138: f32,
    pub f13c: f32,
    pub f140: f32,
    /// +0x148 (byte, installed 3).
    pub b148: u8,
    /// +0x14c element boost (u16, installed 20).
    pub element_up: u16,
    /// +0x150 .. +0x160 (installed 75 / 0.75 / 0.8 / 0.5 / 0.5).
    pub f150: f32,
    pub f154: f32,
    pub f158: f32,
    pub f15c: f32,
    pub f160: f32,
    /// +0x164 (u16, installed 50).
    pub w164: u16,
    /// +0x16c / +0x170 / +0x174 (installed 0.8 / 0.5 / 0.25).
    pub f16c: f32,
    pub f170: f32,
    pub f174: f32,
}

impl Rates {
    /// Reads the fields from the SSTM record bytes (None when short).
    pub fn read(record: &[u8]) -> Option<Self> {
        let f = |at: usize| {
            record
                .get(at..at + 4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        };
        let h = |at: usize| {
            record
                .get(at..at + 2)
                .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
        };
        Some(Rates {
            w11c: h(0x11c)?,
            f120: f(0x120)?,
            f124: f(0x124)?,
            f128: f(0x128)?,
            draw_range: f(0x12c)?,
            combo_damage_up: h(0x130)?,
            reaction_damage_up: h(0x134)?,
            f138: f(0x138)?,
            f13c: f(0x13c)?,
            f140: f(0x140)?,
            b148: *record.get(0x148)?,
            element_up: h(0x14c)?,
            f150: f(0x150)?,
            f154: f(0x154)?,
            f158: f(0x158)?,
            f15c: f(0x15c)?,
            f160: f(0x160)?,
            w164: h(0x164)?,
            f16c: f(0x16c)?,
            f170: f(0x170)?,
            f174: f(0x174)?,
        })
    }
}

/// Ability bits read by the attack code.
pub mod bit {
    /// Guard: PTYA 0x0b's ability bit (item 0x52).
    pub const GUARD: u16 = 1;
    /// Damage modifier gates in FUN_1403c12f0 (stats +0x1dc bits 6–8).
    pub const COMBO_BOOST: u16 = 0x66;
    pub const AIR_COMBO_BOOST: u16 = 0x67;
    pub const REACTION_BOOST: u16 = 0x68;
    /// Stats +0x1dc bit 11: no combo limit while MP charges
    /// (FUN_1403d7850 / FUN_1403d7780 with FUN_1403c1d10).
    pub const BERSERK: u16 = 0x6b;
    /// Stats +0x1e4 bit 13 (item 0x21b "Combo Master"): a combo keeps going
    /// after a miss (FUN_1403d6fc0).
    pub const COMBO_MASTER: u16 = 0xad;
    /// Item 0x9e "Aerial Recovery" (FUN_1403d6d50 tests stats +0x1d0 bit
    /// 20): a Jump press in a blow flight.
    pub const AERIAL_RECOVERY: u16 = 20;
    /// Item 0x194 "No Experience" (stats +0x1dc bit 20): FUN_1403ed110
    /// refuses a kill's EXP (the Winner's Proof's ability).
    pub const NO_EXPERIENCE: u16 = 0x74;
    /// Item 0x193 "Magic Lock-On" (FUN_1403c5170 tests ability 115):
    /// Fire, Blizzard, Thunder and Magnet lock a target when there is none.
    pub const MAGIC_LOCK_ON: u16 = 0x73;
}

/// Ability items whose effect FUN_140401730 applies beyond setting the bit
/// (names from the installed system text).
pub mod item {
    pub const COMBO_PLUS: u16 = 0xa2;
    pub const AIR_COMBO_PLUS: u16 = 0xa3;
    pub const COMBO_BOOST: u16 = 0x186;
    pub const AIR_COMBO_BOOST: u16 = 0x187;
    pub const REACTION_BOOST: u16 = 0x188;
    pub const FINISHING_PLUS: u16 = 0x189;
    pub const NEGATIVE_COMBO: u16 = 0x18a;
    pub const BERSERK_CHARGE: u16 = 0x18b;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Abilities {
    /// +0x00: the ability bit set.
    pub bits: [u32; 8],
    /// +0x20..+0x24: growth levels (1-based, 0 = none) of the five growth
    /// families in [`GROWTH`].
    pub growth: [u8; 5],
    /// +0x30 / +0x31: ground and air combo limits (stats +0x200 / +0x201),
    /// 3 each by default.
    pub ground_combo: i8,
    pub air_combo: i8,
    /// +0x32: finishers that may chain (stats +0x202), 1 by default.
    pub finishers: u8,
    /// +0x33: Experience Boost (default 1; item 0x191 adds 1).
    pub experience_boost: u8,
    /// +0x34: Defender (item 0x19e adds SSTM +0x148).
    pub defender: u8,
    /// +0x38: Draw (item 0x195 adds SSTM DrawRange).
    pub draw: f32,
    /// +0x3c: MP Haste, MP Hastera and MP Hastega (default 1; items 0x19d /
    /// 0x1a5 / 0x1a6 add SSTM +0x120 / +0x124 / +0x128).
    pub mp_haste: f32,
    /// +0x40: Drive Boost (default 1; item 0x18d adds SSTM +0x13c).
    pub drive_boost: f32,
    /// +0x44: Form Boost (default 1; item 0x18e multiplies by SSTM +0x140).
    pub form_boost: f32,
    /// +0x48 / +0x4a / +0x4c: fire, blizzard and thunder damage percents
    /// (stats +0x218.. in FUN_1403c12f0), 100 by default; items 0x198..0x19a
    /// add SSTM +0x14c.
    pub element: [u16; 3],
    /// +0x4e / +0x50: Combo Boost and Air Combo Boost (SSTM ComboDamageUp
    /// each).
    pub combo_boost: u16,
    pub air_combo_boost: u16,
    /// +0x52: Reaction Boost (SSTM ReactionDamageUp each).
    pub reaction_boost: u16,
    /// +0x54: Item Boost (item 0x19b adds SSTM +0x164).
    pub item_boost: u16,
    /// +0x56: Berserk Charge's strength bonus while MP charges (SSTM
    /// +0x11c each; FUN_1403c12f0 adds it to the attack stat).
    pub berserk_strength: u16,
    /// +0x58: Damage Drive (item 0x18c adds SSTM +0x138); +0x5c: MP Rage
    /// (0x19c adds +0x150).
    pub damage_drive: f32,
    pub mp_rage: f32,
    /// +0x60: Hyper Healing (default 1; item 0x1a3 multiplies by SSTM +0x154).
    pub hyper_healing: f32,
    /// +0x64: Combination Boost (default 1; item 0x190 multiplies by SSTM
    /// +0x158).
    pub combination_boost: f32,
    /// +0x68 / +0x6c: Jackpot and Lucky Lucky (items 0x196 / 0x197 add SSTM
    /// +0x15c / +0x160).
    pub jackpot: f32,
    pub lucky_lucky: f32,
    /// +0x70: Summon Boost (default 1; item 0x18f multiplies by SSTM +0x16c).
    pub summon_boost: f32,
    /// +0x74: Drive Converter (default 1; item 0x21c multiplies by SSTM
    /// +0x170).
    pub drive_converter: f32,
    /// +0x78: Damage Control (item 0x21e adds SSTM +0x174).
    pub damage_control: f32,
}

impl Default for Abilities {
    /// FUN_1404016b0.
    fn default() -> Self {
        Abilities {
            bits: [0; 8],
            growth: [0; 5],
            ground_combo: 3,
            air_combo: 3,
            finishers: 1,
            experience_boost: 1,
            defender: 0,
            draw: 0.,
            mp_haste: 1.,
            drive_boost: 1.,
            form_boost: 1.,
            element: [100; 3],
            combo_boost: 0,
            air_combo_boost: 0,
            reaction_boost: 0,
            item_boost: 0,
            berserk_strength: 0,
            damage_drive: 0.,
            mp_rage: 0.,
            hyper_healing: 1.,
            combination_boost: 1.,
            jackpot: 0.,
            lucky_lucky: 0.,
            summon_boost: 1.,
            drive_converter: 1.,
            damage_control: 0.,
        }
    }
}

/// Growth families' ability bits, five levels each (`DAT_1405c9f00`).
pub type GrowthTable = [[u8; 5]; 5];

/// `DAT_1405c9f00`: High Jump, Quick Run, Aerial Dodge, Glide and Dodge
/// Roll, levels 1–4 (the fifth byte is 0 in each family).
pub const GROWTH: GrowthTable = [
    [0x02, 0x03, 0x04, 0x05, 0x00],
    [0x06, 0x07, 0x08, 0x09, 0x00],
    [0x0a, 0x0b, 0x0c, 0x0d, 0x00],
    [0x0e, 0x0f, 0x10, 0x11, 0x00],
    [0xb5, 0xb6, 0xb7, 0xb8, 0x00],
];

impl Abilities {
    pub fn has(&self, bit: u16) -> bool {
        let word = (bit >> 5) as usize;
        word < self.bits.len() && self.bits[word] >> (bit & 31) & 1 != 0
    }

    pub fn set(&mut self, bit: u16) {
        let word = (bit >> 5) as usize;
        if word < self.bits.len() {
            self.bits[word] |= 1 << (bit & 31);
        }
    }

    /// Takes a bit away (a room's switch; the executable never clears one
    /// alone).
    pub fn clear(&mut self, bit: u16) {
        let word = (bit >> 5) as usize;
        if word < self.bits.len() {
            self.bits[word] &= !(1 << (bit & 31));
        }
    }

    /// FUN_140401730: one ability item (id) with its bit (item +4).
    pub fn add(&mut self, item: u16, bit: u16, sstm: &Rates) {
        match item {
            0 => return,
            item::COMBO_PLUS => self.ground_combo = self.ground_combo.wrapping_add(1),
            item::AIR_COMBO_PLUS => self.air_combo = self.air_combo.wrapping_add(1),
            item::COMBO_BOOST => {
                self.combo_boost = self.combo_boost.wrapping_add(sstm.combo_damage_up)
            }
            item::AIR_COMBO_BOOST => {
                self.air_combo_boost = self.air_combo_boost.wrapping_add(sstm.combo_damage_up)
            }
            item::REACTION_BOOST => {
                self.reaction_boost = self.reaction_boost.wrapping_add(sstm.reaction_damage_up)
            }
            item::FINISHING_PLUS => self.finishers = self.finishers.wrapping_add(1),
            item::NEGATIVE_COMBO => {
                self.ground_combo = self.ground_combo.wrapping_sub(1);
                self.air_combo = self.air_combo.wrapping_sub(1);
            }
            item::BERSERK_CHARGE => {
                self.berserk_strength = self.berserk_strength.wrapping_add(sstm.w11c)
            }
            0x18c => self.damage_drive += sstm.f138,
            0x18d => self.drive_boost += sstm.f13c,
            0x18e => self.form_boost *= sstm.f140,
            0x18f => self.summon_boost *= sstm.f16c,
            0x190 => self.combination_boost *= sstm.f158,
            0x191 => self.experience_boost = self.experience_boost.wrapping_add(1),
            0x195 => self.draw += sstm.draw_range,
            0x196 => self.jackpot += sstm.f15c,
            0x197 => self.lucky_lucky += sstm.f160,
            0x198..=0x19a => {
                let k = (item - 0x198) as usize;
                self.element[k] = self.element[k].wrapping_add(sstm.element_up);
            }
            0x19b => self.item_boost = self.item_boost.wrapping_add(sstm.w164),
            0x19c => self.mp_rage += sstm.f150,
            0x19d => self.mp_haste += sstm.f120,
            0x19e => self.defender = self.defender.wrapping_add(sstm.b148),
            0x1a3 => self.hyper_healing *= sstm.f154,
            0x1a5 => self.mp_haste += sstm.f124,
            0x1a6 => self.mp_haste += sstm.f128,
            0x21c => self.drive_converter *= sstm.f170,
            0x21e => self.damage_control += sstm.f174,
            _ => {}
        }
        self.set(bit);
    }

    /// FUN_1404014a0's slot pass: every equipped slot (bit 15) of the
    /// member's 80 adds its item, then every equipped slot of the 24 in the
    /// active Drive form's record. `bit_of` gives an item's ability bit
    /// (item +4) and whether its byte +6 is non-zero (action/growth),
    /// which the member's own slots skip while a form is active. Then each
    /// growth family's level is the highest of its five bits present.
    pub fn equip(
        &mut self,
        slots: &[u16],
        form_slots: Option<&[u16]>,
        bit_of: impl Fn(u16) -> Option<(u16, bool)>,
        growth: &GrowthTable,
        sstm: &Rates,
    ) {
        for &slot in slots.iter().take(80) {
            if slot & 0x8000 == 0 {
                continue;
            }
            let id = slot & 0x7fff;
            let Some((bit, action_or_growth)) = bit_of(id) else {
                continue;
            };
            if !action_or_growth || form_slots.is_none() {
                self.add(id, bit, sstm);
            }
        }
        for &slot in form_slots.unwrap_or(&[]).iter().take(24) {
            if slot & 0x8000 != 0
                && let Some((bit, _)) = bit_of(slot & 0x7fff)
            {
                self.add(slot & 0x7fff, bit, sstm);
            }
        }
        for (family, bits) in growth.iter().enumerate() {
            if let Some(level) = (0..5).rev().find(|&l| self.has(bits[l] as u16)) {
                self.growth[family] = level as u8 + 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn rates() -> Rates {
        let mut r = vec![0u8; 0x17c];
        let put16 =
            |r: &mut Vec<u8>, at: usize, v: u16| r[at..at + 2].copy_from_slice(&v.to_le_bytes());
        put16(&mut r, 0x11c, 2);
        put16(&mut r, 0x130, 10);
        put16(&mut r, 0x134, 50);
        put16(&mut r, 0x14c, 20);
        Rates::read(&r).unwrap()
    }

    #[test]
    fn a_cleared_bit_leaves_its_neighbours() {
        let mut a = Abilities::default();
        a.set(bit::MAGIC_LOCK_ON);
        a.set(bit::MAGIC_LOCK_ON + 1);
        a.clear(bit::MAGIC_LOCK_ON);
        assert!(!a.has(bit::MAGIC_LOCK_ON));
        assert!(a.has(bit::MAGIC_LOCK_ON + 1));
    }

    #[test]
    fn combo_items_adjust_the_limits_and_set_their_bits() {
        let s = rates();
        let mut a = Abilities::default();
        assert_eq!((a.ground_combo, a.air_combo, a.finishers), (3, 3, 1));
        a.add(item::COMBO_PLUS, 162, &s);
        a.add(item::COMBO_PLUS, 162, &s);
        a.add(item::NEGATIVE_COMBO, 394, &s);
        a.add(item::FINISHING_PLUS, 393, &s);
        a.add(item::COMBO_BOOST, 7, &s);
        a.add(item::BERSERK_CHARGE, bit::BERSERK, &s);
        a.add(0x199, 9, &s);
        assert_eq!((a.ground_combo, a.air_combo, a.finishers), (4, 2, 2));
        assert_eq!(a.combo_boost, 10);
        assert_eq!(a.berserk_strength, 2);
        assert_eq!(a.element, [100, 120, 100]);
        assert!(a.has(162) && a.has(bit::BERSERK) && a.has(7) && a.has(9));
        assert!(!a.has(8));
    }

    #[test]
    fn only_equipped_slots_count_and_growth_takes_the_top_level() {
        let s = rates();
        let mut a = Abilities::default();
        let bit_of = |id: u16| Some((id, false));
        let mut growth = [[0u8; 5]; 5];
        growth[0] = [2, 3, 4, 5, 6];
        a.equip(&[0x8000 | 2, 4, 0x8000 | 3, 0], None, bit_of, &growth, &s);
        assert!(a.has(2) && a.has(3) && !a.has(4));
        assert_eq!(a.growth[0], 2);
    }

    #[test]
    fn equipped_pass_respects_member_and_form_record_sizes() {
        let s = rates();
        let bit_of = |_| Some((21, false));
        let slots = vec![0x8000 | item::COMBO_PLUS; 81];
        let form_slots = vec![0x8000 | item::AIR_COMBO_PLUS; 25];
        let mut a = Abilities::default();
        a.equip(&slots, Some(&form_slots), bit_of, &GROWTH, &s);
        assert_eq!((a.ground_combo, a.air_combo), (83, 27));
    }

    #[test]
    fn a_form_replaces_member_growth_but_keeps_support_duplicates() {
        let s = rates();
        let bit_of = |id| match id {
            2 | 5 => Some((id, true)),
            item::COMBO_PLUS => Some((21, false)),
            _ => None,
        };
        let slots = [0x8005, 0x8000 | item::COMBO_PLUS];
        let form_slots = [0x8002, 0x8000 | item::COMBO_PLUS];
        let mut a = Abilities::default();
        a.equip(&slots, Some(&form_slots), bit_of, &GROWTH, &s);
        assert_eq!(a.growth[0], 1);
        assert!(!a.has(5));
        assert_eq!(a.ground_combo, 5);
    }

    #[test]
    fn defender_is_an_unsigned_wrapping_byte_when_consumed() {
        let mut s = rates();
        s.b148 = 128;
        let mut a = Abilities::default();
        a.add(0x19e, 126, &s);
        assert_eq!(a.defender as i32, 128);
        a.add(0x19e, 126, &s);
        assert_eq!(a.defender, 0);
    }
}
