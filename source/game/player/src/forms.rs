//! Drive Forms, translated from the PC executable: the form records in the
//! save (FUN_1403a1410), the party requirement FUN_140400740, the Drive
//! command's availability (FUN_140401d60 case 4), the Antiform / forced
//! Final roll FUN_140421d60, form EXP and level-ups FUN_1403e8380 with
//! their growth-ability swaps, and the EXP sources FUN_1403e86b0 /
//! FUN_1403e8730 / FUN_1403e8760 / FUN_1403e8780.
//!
//! Form ids are obj+0xde0: 0 normal Sora, then Valor, Wisdom, Limit,
//! Master, Final and Antiform.

use crate::abilities::Abilities;

pub const NORMAL: u8 = 0;
pub const VALOR: u8 = 1;
pub const WISDOM: u8 = 2;
pub const LIMIT: u8 = 3;
pub const MASTER: u8 = 4;
pub const FINAL: u8 = 5;
pub const ANTI: u8 = 6;

/// `DAT_1405c5d08`: each form's item (type 0x15): Valor Form, Wisdom
/// Form, Limit Form, Master Form, Final Form, Antiform.
pub const FORM_ITEMS: [u16; 7] = [0, 0x1a, 0x1b, 0x233, 0x1f, 0x1d, 0x1e];

/// `0x1405c9030`: per form 1..=5 the growth family's items {none, LV1,
/// LV2, LV3, MAX}: High Jump, Quick Run, Dodge Roll, Aerial Dodge, Glide.
pub const GROWTH_ITEMS: [[u16; 5]; 5] = [
    [0, 0x5e, 0x5f, 0x60, 0x61],
    [0, 0x62, 0x63, 0x64, 0x65],
    [0, 0x234, 0x235, 0x236, 0x237],
    [0, 0x66, 0x67, 0x68, 0x69],
    [0, 0x6a, 0x6b, 0x6c, 0x6d],
];

/// `DAT_1405cbbd0`: the auto-form abilities FUN_140405900 offers, in
/// order (Auto Final, Auto Master, Auto Wisdom, Auto Valor).
pub const AUTO_FORMS: [u16; 4] = [0x184, 0x183, 0x182, 0x181];

/// SSTM +0x90 / +0xa4: the anti counter falls by 6 when Antiform is
/// chosen and by 10 for Final.
pub const ANTI_DROP: i32 = 6;
pub const FINAL_DROP: i32 = 10;
/// SSTM +0x98 / +0x9c / +0xa0: the forced Final chance,
/// min(3 × 3^n, 75) percent.
pub const FINAL_BASE: f32 = 3.;
pub const FINAL_POWER: f32 = 3.;
pub const FINAL_CAP: f32 = 75.;
/// Ability bit 175 (item 0x21d): Antiform chance 100.
pub const LIGHT_AND_DARKNESS: u16 = 0xaf;
/// The story flag FUN_14039a980(0x4842) a forced Final needs.
pub const FINAL_STORY_FLAG: u32 = 0x4842;

/// A form's save record (`save + 0xe04 + 0x38 × (form − 1)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormRecord {
    /// +0: the form's Keyblade (weapon slot 1); 0 for none.
    pub keyblade: u16,
    /// +2: level, 1-based.
    pub level: u8,
    /// +4: EXP. Antiform's is the anti counter; Final's counts forced-Final
    /// attempts until Final Form is owned.
    pub exp: i32,
    /// +8: 24 ability slots (bit 15 = equipped).
    pub abilities: [u16; 24],
}

impl Default for FormRecord {
    fn default() -> Self {
        FormRecord {
            keyblade: 0,
            level: 1,
            exp: 0,
            abilities: [0; 24],
        }
    }
}

impl FormRecord {
    /// FUN_1403a1680: the first empty slot takes `id` (equipped when its
    /// bit 15 is set). Returns false when all 24 are used.
    pub fn add_ability(&mut self, id: u16) -> bool {
        match self.abilities.iter_mut().find(|s| **s == 0) {
            Some(s) => {
                *s = id;
                true
            }
            None => false,
        }
    }

    /// The equipped-slot list the ability pass reads (FUN_1404014a0).
    pub fn slots(&self) -> &[u16] {
        &self.abilities
    }
}

/// One fmlv row (00battle `fmlv`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormLevel {
    /// form << 4 | level.
    pub id: u8,
    /// The growth step (Antiform rows: the chance in percent).
    pub growth: u8,
    /// The ability Sora gets on reaching this level.
    pub ability: u16,
    /// EXP needed at this level to reach the next (Antiform rows: the
    /// counter threshold).
    pub exp: u32,
}

/// bsearch on the id byte (comparator FUN_1403e87e0).
pub fn level_row(fmlv: &[FormLevel], id: u8) -> Option<&FormLevel> {
    fmlv.iter().find(|r| r.id == id)
}

/// A party slot as FUN_140400740 sees it: the member's character id
/// (objentry +0x4c: 2 Donald, 3 Goofy) and whether FUN_1403c38f0 holds
/// (valid, not KO'd, not stored away).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    pub character: u16,
    pub usable: bool,
}

/// FUN_140400740 / FUN_1404005e0: 0 the party allows `form`, 1 a needed
/// member is unavailable, 2 a needed member is missing. Limit Form and
/// areas whose drive rate is above 1 need nobody; Valor needs Goofy,
/// Wisdom Donald, Master, Final and Antiform both slots.
pub fn party_requirement(form: u8, members: &[Option<Member>; 2], area_rate: f32) -> u8 {
    if area_rate > 1. || form == LIMIT {
        return 0;
    }
    let mut count = 0;
    let mut result = 0;
    for m in members {
        let id = m.map_or(0, |m| m.character);
        let relevant = match form {
            1 => id == 3,
            2 => id == 2,
            f => f != 0 && f < 7,
        };
        if !relevant || !(1..7).contains(&form) {
            continue;
        }
        if id == 0 {
            return 2;
        }
        if !m.is_some_and(|m| m.usable) {
            result = 1;
        }
        count += 1;
    }
    if count == 0 { 2 } else { result }
}

/// The parts of FUN_140401d60 case 4 that decide a Drive command's colour
/// (below 4 = selectable, 5 = greyed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriveCheck {
    /// The form the command drives into (cmd +4) and its cost (cmd +0x1c).
    pub form: u8,
    pub cost: u8,
    /// Full Drive bars (stat+0x1b1).
    pub level: u8,
    /// A form is active (DAT_142a10508) or a change / summon is under way
    /// (DAT_142a10504).
    pub busy: bool,
    /// The player's own form (obj+0xde0): forms 7 and 0xc cannot drive.
    pub player_form: u8,
    /// FUN_1404005e0 for the command.
    pub party: u8,
    /// Limit Form is refused where the area flag (DAT_142a0ff68+8)->+4
    /// bit 1 is set (FUN_1403a3a90).
    pub limit_barred: bool,
    /// DAT_142a11400 bit 18: drive locked.
    pub locked: bool,
}

impl DriveCheck {
    pub fn selectable(&self) -> bool {
        !self.busy
            && self.party == 0
            && !matches!(self.player_form, 7 | 0xc)
            && !(self.form == LIMIT && self.limit_barred)
            && !self.locked
            && self.cost <= self.level
    }
}

/// What FUN_140421d60 reads besides the form records.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RollContext<'a> {
    /// FUN_14014ead0 (DAT_140749804 bit 25): no roll at all.
    pub disabled: bool,
    pub abilities: &'a Abilities,
    /// FUN_1403a3950: the area's anti multiplier ((DAT_142a0ff68+8)->+0xe),
    /// 1 by default.
    pub area_anti: i32,
    /// FUN_1403abb20: the battle flag.
    pub battle: bool,
    /// Party requirements of Antiform and Final (FUN_140400740(6) / (5)).
    pub anti_party: u8,
    pub final_party: u8,
    /// Final Form owned (item 0x1d), the story flag 0x4842, Final's cost
    /// against the full bars.
    pub final_owned: bool,
    pub final_story: bool,
    pub final_cost: u8,
    pub level: u8,
}

/// The roll's result: the form to become, and whether Final Form was
/// obtained by it (FUN_1403c3fe0(0x1d, 1)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rolled {
    pub form: u8,
    pub final_obtained: bool,
}

/// The shared battle RNG `DAT_140783ca0` (×0x10dcd per draw).
fn draw(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(0x10dcd);
    *seed
}

/// The Drive forms' records and owned forms, as the save keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Forms {
    /// Forms 1..=6 (index form − 1).
    pub records: [FormRecord; 6],
    /// Form items owned (FUN_1403c4720 > 0), by form id.
    pub owned: [bool; 7],
}

/// One level gained (FUN_1403e8380's loop body).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelUp {
    pub form: u8,
    pub level: u8,
    /// The growth item that replaced the previous level in the form's own
    /// slots, and the one that replaced a level in Sora's slots.
    pub form_growth: Option<u16>,
    pub sora_growth: Option<u16>,
    /// The ability given to Sora (fmlv +2; FUN_1403c3f00(ability, 1)).
    pub ability: Option<u16>,
}

/// `(slot ^ new) & 0x7fff ^ slot`: the first slot holding `old` (low 15
/// bits) takes `new`, keeping its equipped bit.
fn swap_slot(slots: &mut [u16], old: u16, new: u16) {
    if let Some(s) = slots.iter_mut().find(|s| **s & 0x7fff == old) {
        *s = (new ^ *s) & 0x7fff ^ *s;
    }
}

impl Forms {
    pub fn record(&self, form: u8) -> Option<&FormRecord> {
        (1..=6)
            .contains(&form)
            .then(|| &self.records[form as usize - 1])
    }

    pub fn record_mut(&mut self, form: u8) -> Option<&mut FormRecord> {
        (1..=6)
            .contains(&form)
            .then(|| &mut self.records[form as usize - 1])
    }

    /// FUN_1403e8380's cap: 2 + the number of forms 1..=5 owned.
    pub fn level_cap(&self) -> u8 {
        2 + (1..6).filter(|&f| self.owned[f]).count() as u8
    }

    /// FUN_1403c3fe0 for a form item (type 0x15): owned, and the anti
    /// counter starts again. Valor, Master and Final take `keyblade` when
    /// their slot is empty (the first owned Keyblade).
    pub fn obtain(&mut self, form: u8, keyblade: u16) {
        if !(1..=6).contains(&form) {
            return;
        }
        self.owned[form as usize] = true;
        self.records[ANTI as usize - 1].exp = 0;
        if matches!(form, VALOR | MASTER | FINAL) {
            let r = &mut self.records[form as usize - 1];
            if r.keyblade == 0 {
                r.keyblade = keyblade;
            }
        }
    }

    /// FUN_140421d60: the form a Drive command for `requested` becomes.
    /// Every request but Final, with both slots usable for Antiform, adds 1
    /// to the anti counter and rolls against fmlv's Antiform row for that
    /// counter (25 / 10 / 0 %, × the area's multiplier, 100 with Light &
    /// Darkness); in battle a hit becomes Antiform, or a forced Final
    /// Form while Final is not owned.
    pub fn roll(
        &mut self,
        requested: u8,
        ctx: &RollContext,
        fmlv: &[FormLevel],
        seed: &mut u32,
    ) -> Rolled {
        let none = Rolled {
            form: requested,
            final_obtained: false,
        };
        if ctx.disabled {
            return none;
        }
        let anti = ANTI as usize - 1;
        let drop = |a: i32, by: i32| if a - by < 1 { 0 } else { a - by };
        if requested == FINAL {
            self.records[anti].exp = drop(self.records[anti].exp, FINAL_DROP);
            return none;
        }
        if ctx.anti_party != 0 {
            return none;
        }
        // fmlv rows 0x61.. : byte +1 the chance, +4 the counter threshold.
        let rows: Vec<&FormLevel> = fmlv.iter().filter(|r| r.id >> 4 == ANTI).collect();
        let a = self.records[anti].exp;
        let mut k = 0;
        while k + 1 < rows.len() && a < rows[k].exp as i32 {
            k += 1;
        }
        let chance = if ctx.abilities.has(LIGHT_AND_DARKNESS) {
            100
        } else {
            ctx.area_anti * rows.get(k).map_or(0, |r| r.growth as i32)
        };
        let r = draw(seed);
        self.records[anti].exp += 1;
        if ((r / 0x28f_5c29) as i32) < chance && ctx.battle {
            if !ctx.final_owned
                && ctx.final_party == 0
                && ctx.final_story
                && ctx.final_cost <= ctx.level
            {
                let n = &mut self.records[FINAL as usize - 1].exp;
                let p = (FINAL_POWER.powf(*n as f32) * FINAL_BASE).min(FINAL_CAP);
                let r = draw(seed);
                *n += 1;
                if r as f32 * 2.328_306_4e-10 * 100. < p {
                    *n = 0;
                    self.obtain(FINAL, 0);
                    self.records[anti].exp = drop(self.records[anti].exp, FINAL_DROP);
                    return Rolled {
                        form: FINAL,
                        final_obtained: true,
                    };
                }
            }
            self.records[anti].exp = drop(self.records[anti].exp, ANTI_DROP);
            return Rolled {
                form: ANTI,
                final_obtained: false,
            };
        }
        none
    }

    /// FUN_1403e8380: `amount` EXP for `form`. While the level is below the
    /// cap and fmlv has a row for it, enough EXP raises the level: a growth
    /// step swaps the family's item in the form's slots (level g) and in
    /// Sora's (level g − 1), and the row's ability goes to Sora. EXP past
    /// the cap is dropped. `no_exp` is the area flag FUN_1403a3c40.
    pub fn add_exp(
        &mut self,
        form: u8,
        amount: i32,
        fmlv: &[FormLevel],
        sora_slots: &mut [u16],
        no_exp: bool,
    ) -> Vec<LevelUp> {
        let mut ups = Vec::new();
        if no_exp || !(1..=6).contains(&form) {
            return ups;
        }
        let cap = self.level_cap();
        let r = &mut self.records[form as usize - 1];
        r.exp += amount;
        loop {
            if r.level >= cap {
                break;
            }
            let Some(row) = level_row(fmlv, form << 4 | r.level) else {
                break;
            };
            if r.exp < row.exp as i32 {
                return ups;
            }
            r.level += 1;
            let mut up = LevelUp {
                form,
                level: r.level,
                ..LevelUp::default()
            };
            if let Some(next) = level_row(fmlv, form << 4 | r.level) {
                let g = next.growth as usize;
                if row.growth < next.growth && (1..=5).contains(&form) && (1..5).contains(&g) {
                    let t = &GROWTH_ITEMS[form as usize - 1];
                    swap_slot(&mut r.abilities, t[g - 1], t[g]);
                    up.form_growth = Some(t[g]);
                    let (old, new) = (if g >= 2 { t[g - 2] } else { 0 }, t[g - 1]);
                    swap_slot(sora_slots, old, new);
                    up.sora_growth = Some(new);
                }
                if next.ability != 0 {
                    up.ability = Some(next.ability);
                }
            }
            ups.push(up);
            r.exp -= row.exp as i32;
        }
        r.exp = 0;
        ups
    }
}

/// FUN_1403e86b0 (Valor: each hit Sora lands on an enemy), FUN_1403e8780
/// (Wisdom / Final: enemies defeated) and FUN_1403e8760 (Master: drive
/// orbs). `enemy` is the defeated enemy's character id (objentry +0x4c).
pub fn kill_exp(form: u8, enemy: u16) -> i32 {
    let e = enemy as i32;
    match form {
        WISDOM if (1000..1000 + 0x3f4).contains(&e) && !(0x417..0x417 + 0xd).contains(&e) => 1,
        FINAL if (3000..=3000 + 0x3f6).contains(&e) => 1,
        _ => 0,
    }
}

/// FUN_140400b20's rule: whether party member `character` leaves for form
/// `form` (Valor takes Goofy, Wisdom Donald, Master, Final and Antiform
/// both, Limit nobody).
pub fn absorbs(form: u8, character: u16) -> bool {
    match form {
        VALOR => character == 3,
        WISDOM => character == 2,
        LIMIT => false,
        f => (1..7).contains(&f),
    }
}

/// FUN_1403c2fc0(id, 0, 100, 200, 300) → FUN_1403f3fa0 / FUN_1403f4180:
/// where a member returns after a revert. From 100 above Sora's feet, at
/// a radius drawn in 200..300 (two LCG draws), the eight directions π/4
/// apart are tried from a rotating base sector plus jitter; a spot needs
/// ground within 200 above and 1200 below it, and every failed ring
/// moves 100 inward. Returns the spot and the yaw facing Sora. The line
/// of sight test from Sora is not reproduced (no ray query here).
pub fn return_spot(
    world: &impl crate::World,
    sora: crate::Vec3,
    seed: &mut u32,
    sector: &mut u8,
) -> (crate::Vec3, f32) {
    use std::f32::consts::FRAC_PI_4;
    let (rmin, rmax) = (200f32, 300f32);
    let p = [sora[0], sora[1] + 100., sora[2]];
    let s1 = draw(seed);
    let s2 = draw(seed);
    let mut r = s1 as f32 * 2.328_306_4e-10 * (rmax - rmin) + rmin;
    let c = *sector as u32;
    *sector = (*sector + 1) & 7;
    let a0 =
        ((c * 2) | ((c >> 2) & 1)) as f32 * FRAC_PI_4 + s2 as f32 * 2.328_306_4e-10 * FRAC_PI_4;
    let face = |q: crate::Vec3| (sora[0] - q[0]).atan2(sora[2] - q[2]);
    let mut k = 0;
    while rmin <= r {
        k += 1;
        let a = k as f32 * FRAC_PI_4 + a0;
        let cand = [p[0] + a.sin() * r, p[1], p[2] + a.cos() * r];
        if let Some(y) = world.floor(cand[0], cand[2], cand[1] + 200.)
            && y >= cand[1] - 1200.
        {
            let q = [cand[0], y, cand[2]];
            return (q, face(q));
        }
        if k > 7 {
            r -= 100.;
            k = 0;
        }
    }
    (sora, face(sora))
}

/// Limit Form's EXP commands (FUN_1403d8b40 → FUN_1403e8730).
pub const LIMIT_EXP_COMMANDS: [u16; 4] = [0x2ae, 0x2bc, 0x2bf, 0x2c2];

#[cfg(test)]
mod tests {
    use super::*;

    fn fmlv() -> Vec<FormLevel> {
        let rows: [(u8, u8, u16, u32); 10] = [
            (0x11, 1, 0, 0x50),
            (0x12, 1, 0x181, 0xa0),
            (0x13, 2, 0, 0x118),
            (0x14, 2, 0xa2, 0x1c0),
            (0x15, 3, 0, 0x230),
            (0x16, 3, 0xa2, 0x2a0),
            (0x17, 4, 0, 0),
            (0x61, 0x19, 0, 10),
            (0x62, 0x0a, 0, 5),
            (0x63, 0, 0, 0),
        ];
        rows.iter()
            .map(|&(id, growth, ability, exp)| FormLevel {
                id,
                growth,
                ability,
                exp,
            })
            .collect()
    }

    #[test]
    fn the_party_requirement_follows_each_form() {
        let goofy = Some(Member {
            character: 3,
            usable: true,
        });
        let donald = Some(Member {
            character: 2,
            usable: false,
        });
        assert_eq!(party_requirement(LIMIT, &[None, None], 1.), 0);
        assert_eq!(party_requirement(VALOR, &[None, None], 1.), 2);
        assert_eq!(party_requirement(VALOR, &[donald, goofy], 1.), 0);
        assert_eq!(party_requirement(WISDOM, &[donald, goofy], 1.), 1);
        assert_eq!(party_requirement(MASTER, &[goofy, None], 1.), 2);
        assert_eq!(party_requirement(MASTER, &[donald, goofy], 1.), 1);
        assert_eq!(party_requirement(MASTER, &[None, None], 2.), 0);
    }

    #[test]
    fn drive_needs_the_cost_in_full_bars() {
        let mut c = DriveCheck {
            form: LIMIT,
            cost: 4,
            level: 3,
            busy: false,
            player_form: 0,
            party: 0,
            limit_barred: false,
            locked: false,
        };
        assert!(!c.selectable());
        c.level = 4;
        assert!(c.selectable());
        c.busy = true;
        assert!(!c.selectable());
    }

    #[test]
    fn levels_rise_with_growth_swaps_and_stop_at_the_cap() {
        let mut f = Forms::default();
        f.owned[1] = true;
        f.records[0].add_ability(0x805e);
        let mut sora = [0x8001u16, 0, 0];
        // Cap 3 with one form owned: 0x50 to level 2, 0xa0 to level 3.
        let ups = f.add_exp(VALOR, 0x50 + 0xa0 + 7, &fmlv(), &mut sora, false);
        assert_eq!(ups.len(), 2);
        assert_eq!(ups[0].ability, Some(0x181));
        assert_eq!(ups[1].form_growth, Some(0x5f));
        assert_eq!(f.records[0].abilities[0], 0x805f);
        assert_eq!(sora, [0x8001, 0x5e, 0]);
        // At the cap the rest is dropped.
        assert_eq!((f.records[0].level, f.records[0].exp), (3, 0));
        f.owned[2] = true;
        let ups = f.add_exp(VALOR, 0x117, &fmlv(), &mut sora, false);
        assert!(ups.is_empty());
        assert_eq!(f.records[0].exp, 0x117);
    }

    #[test]
    fn the_roll_turns_anti_by_the_counter_and_forces_final() {
        let abilities = Abilities::default();
        let mut ctx = RollContext {
            disabled: false,
            abilities: &abilities,
            area_anti: 1,
            battle: true,
            anti_party: 0,
            final_party: 0,
            final_owned: true,
            final_story: true,
            final_cost: 5,
            level: 5,
        };
        let mut f = Forms::default();
        // Counter 0: the 0 % row, no Antiform.
        let mut seed = 1;
        assert_eq!(f.roll(VALOR, &ctx, &fmlv(), &mut seed).form, VALOR);
        assert_eq!(f.records[5].exp, 1);
        // Counter 10: 25 %. Seed 1 × 69069 / 0x28f5c29 = 0 < 25 → Anti,
        // counter 11 − 6.
        f.records[5].exp = 10;
        let mut seed = 1;
        assert_eq!(f.roll(VALOR, &ctx, &fmlv(), &mut seed).form, ANTI);
        assert_eq!(f.records[5].exp, 5);
        // Out of battle no Antiform, but the counter still counts.
        ctx.battle = false;
        let mut seed = 1;
        f.records[5].exp = 10;
        assert_eq!(f.roll(VALOR, &ctx, &fmlv(), &mut seed).form, VALOR);
        assert_eq!(f.records[5].exp, 11);
        // Final not owned: the forced Final roll (3 % at n = 0); from seed
        // 217 the draws are 14987973 (0 < 25) and 117188801 (2.7 % < 3).
        ctx.battle = true;
        ctx.final_owned = false;
        let mut seed = 217;
        let r = f.roll(WISDOM, &ctx, &fmlv(), &mut seed);
        assert_eq!(
            r,
            Rolled {
                form: FINAL,
                final_obtained: true
            }
        );
        assert!(f.owned[FINAL as usize]);
        assert_eq!(f.records[5].exp, 0);
        // Choosing Final lowers the counter by 10.
        f.records[5].exp = 14;
        assert_eq!(f.roll(FINAL, &ctx, &fmlv(), &mut seed).form, FINAL);
        assert_eq!(f.records[5].exp, 4);
    }

    #[test]
    fn members_leave_by_form_and_return_on_the_ground() {
        assert!(absorbs(VALOR, 3) && !absorbs(VALOR, 2));
        assert!(absorbs(WISDOM, 2) && !absorbs(LIMIT, 2));
        assert!(absorbs(MASTER, 2) && absorbs(FINAL, 3));
        struct Flat;
        impl crate::World for Flat {
            fn floor(&self, _: f32, _: f32, top: f32) -> Option<f32> {
                (top >= 0.).then_some(0.)
            }
            fn resolve_walls(&self, p: [f32; 2], _: f32, _: f32, _: f32) -> [f32; 2] {
                p
            }
        }
        let (mut seed, mut sector) = (1, 0);
        let (q, yaw) = return_spot(&Flat, [10., 0., 20.], &mut seed, &mut sector);
        let d = ((q[0] - 10.).powi(2) + (q[2] - 20.).powi(2)).sqrt();
        assert!((200. ..=300.).contains(&d), "{d}");
        assert_eq!((q[1], sector), (0., 1));
        let back = [10. - q[0], 20. - q[2]];
        assert!((back[0].atan2(back[1]) - yaw).abs() < 1e-5);
    }

    #[test]
    fn kill_exp_counts_heartless_for_wisdom_and_nobodies_for_final() {
        assert_eq!(kill_exp(WISDOM, 1000), 1);
        assert_eq!(kill_exp(WISDOM, 1050), 0);
        assert_eq!(kill_exp(WISDOM, 3000), 0);
        assert_eq!(kill_exp(FINAL, 3000), 1);
        assert_eq!(kill_exp(VALOR, 1000), 0);
    }
}
