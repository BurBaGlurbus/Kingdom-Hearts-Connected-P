//! 00battle.bin tables used by combat: ATKP (attack parameters), ENMP (enemy
//! parameters), LVPM (enemy level multipliers) and LVUP (player levels).
//! Layouts follow the public OpenKH 00battle.md
//! (7a3b945c538d32c6a285128c98aefba093f52ceb). Field meanings marked
//! "executable" were checked against the PC damage routine FUN_1403c12f0.
use std::io;

fn invalid(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_owned())
}
fn u16_at(b: &[u8], at: usize) -> io::Result<u16> {
    b.get(at..at + 2)
        .map(|w| u16::from_le_bytes([w[0], w[1]]))
        .ok_or_else(|| invalid("battle table truncated"))
}
fn i16_at(b: &[u8], at: usize) -> io::Result<i16> {
    u16_at(b, at).map(|v| v as i16)
}
fn u32_at(b: &[u8], at: usize) -> io::Result<u32> {
    b.get(at..at + 4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .ok_or_else(|| invalid("battle table truncated"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attack {
    pub id: u16,
    pub kind: u8,
    /// Attacker stat used (executable): 0 strength, 1 magic, else none.
    pub stat: u8,
    pub power: u16,
    pub element: u8,
    pub reaction: u8,
    pub knockback: [i16; 2],
    pub flags: u8,
    /// Sound id played on a hit when non-negative (executable: +0x1C read
    /// by the hit routine FUN_1403d12c0).
    pub hit_sound: i32,
    /// Hit effect number (+0xb, 0 = none) and random-pick count (+0x2d)
    /// (executable: FUN_1403d1b40).
    pub hit_effect: u8,
    pub hit_effect_random: u8,
    /// Clash fields (executable: FUN_1403d2140, FUN_1403d61d0,
    /// FUN_140411060, FUN_1403d1f40): RefactSelf +0x13, RefactOther +0x14,
    /// ReflectMotion +0x15, ReflectHitBack +0x16 (i8), ReflectRange +0x22
    /// (degrees, 0 = no cone), ReflectAngle +0x23 (i8).
    pub refact_self: u8,
    pub refact_other: u8,
    pub reflect_motion: u8,
    pub reflect_hit_back: i8,
    pub reflect_range: u8,
    pub reflect_angle: i8,
    /// Level-variant selection (executable FUN_1403eb230): the row applies
    /// from this consecutive-hit count (+0), with Switch (+0x25) choosing
    /// odd (1) or even (2) hits past it, 0 for every hit.
    pub step: u8,
    pub switch: u8,
    /// Interval (+0x26): ticks before the same attack may hit again.
    pub interval: u8,
    /// ComboGroup (+0x2c).
    pub combo_group: u8,
    /// DriveDrain (+0x29), RevengeDamage (+0x2a), HpDrain (+0x2f).
    pub drive_drain: u8,
    pub revenge: u8,
    pub hp_drain: u8,
    /// Kind (+0x2e): 1 ground finisher, 2 air finisher, 4 reaction command
    /// (the damage routine's combo-boost terms, FUN_1403c12f0).
    pub combo_kind: u8,
    /// +0x10 (i16): Magnet's spin, units per tick around its centre.
    pub spin: i16,
    /// +0x28: the extra downward reach of the volume's environment test
    /// (FUN_1403d0bb0; 0 = none). Flags bit 0 (+0x12) enables the test.
    pub env_reach: u8,
}

fn parse_row(rec: &[u8]) -> io::Result<Attack> {
    Ok(Attack {
        id: u16_at(rec, 2)?,
        kind: rec[4],
        stat: rec[5],
        power: u16_at(rec, 6)?,
        element: rec[9],
        reaction: rec[10],
        knockback: [i16_at(rec, 12)?, i16_at(rec, 14)?],
        flags: rec[18],
        hit_sound: u32_at(rec, 0x1c)? as i32,
        hit_effect: rec[0xb],
        hit_effect_random: rec[0x2d],
        refact_self: rec[0x13],
        refact_other: rec[0x14],
        reflect_motion: rec[0x15],
        reflect_hit_back: rec[0x16] as i8,
        reflect_range: rec[0x22],
        reflect_angle: rec[0x23] as i8,
        step: rec[0],
        switch: rec[0x25],
        interval: rec[0x26],
        combo_group: rec[0x2c],
        drive_drain: rec[0x29],
        revenge: rec[0x2a],
        hp_drain: rec[0x2f],
        combo_kind: rec[0x2e],
        spin: i16_at(rec, 0x10)?,
        env_reach: rec[0x28],
    })
}

/// The ATKP row for `id` at sub-table 0, hit step 0 (record bytes 0 and 1
/// zero), as the executable's search key ((id << 8) | sub) << 8 selects.
pub fn attack(atkp: &[u8], id: u16) -> io::Result<Attack> {
    attack_steps(atkp, id)?
        .into_iter()
        .next()
        .ok_or_else(|| invalid("ATKP id absent"))
}

/// Every sub-table 0 row of `id` in file order: step 0 first, then the
/// level variants that follow it (FUN_1403eafc0 finds the first, FUN_1403eb230
/// walks the rest while Id and sub-table match).
pub fn attack_steps(atkp: &[u8], id: u16) -> io::Result<Vec<Attack>> {
    attack_steps_sub(atkp, id, 0)
}

/// The rows of `id` in sub-table `sub` (the attacker's stats byte +0x258;
/// spells pass their `magi` row index).
pub fn attack_steps_sub(atkp: &[u8], id: u16, sub: u8) -> io::Result<Vec<Attack>> {
    let count = u32_at(atkp, 4)? as usize;
    let mut out = Vec::new();
    for k in 0..count {
        let r = 8 + k * 48;
        let rec = atkp
            .get(r..r + 48)
            .ok_or_else(|| invalid("ATKP truncated"))?;
        if u16_at(rec, 2)? == id && rec[1] == sub {
            if out.is_empty() && rec[0] != 0 {
                continue;
            }
            out.push(parse_row(rec)?);
        } else if !out.is_empty() {
            break;
        }
    }
    if out.is_empty() {
        return Err(invalid("ATKP id absent"));
    }
    Ok(out)
}

/// FUN_1403eb230: the row used for the `count`-th consecutive hit of a
/// combo group (0-based).
pub fn step_row(rows: &[Attack], count: u32) -> &Attack {
    &rows[step_index(rows, count)]
}

/// The index of [`step_row`]'s row.
pub fn step_index(rows: &[Attack], count: u32) -> usize {
    let mut best = 0;
    for (i, r) in rows.iter().enumerate().skip(1) {
        if count < r.step as u32 {
            break;
        }
        if r.switch == 0 || ((count - r.step as u32) & 1) + 1 == r.switch as u32 {
            best = i;
        }
    }
    best
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enemy {
    pub id: u16,
    /// 0 uses the world battle level.
    pub level: u16,
    /// Main HP pool, percent of the LVPM HP.
    pub hp_percent: u16,
    /// Damage cap and minimum, per 10000 of max HP (executable).
    pub damage_cap: u16,
    pub damage_min: u16,
    /// Physical, fire, blizzard, thunder, dark, light, general (percent).
    pub weakness: [u16; 7],
    /// +0x56: the EXP multiplier of the kill.
    pub exp: u16,
    /// +0x58: the przt id scattered on death (0: none).
    pub prize: u16,
    /// +0x5a: the bonus id the kill grants (0: none).
    pub bonus: u16,
    /// The 32 HP pools as percent of the LVPM HP: +4 + 2k for pools 0..15,
    /// +0x24 + 2k for 16..31 (FUN_1403c2240). Pool 0 is `hp_percent`.
    pub pools: [u16; 32],
}

pub fn enemy(enmp: &[u8], id: u16) -> io::Result<Enemy> {
    let count = u32_at(enmp, 4)? as usize;
    for k in 0..count {
        let r = 8 + k * 0x5c;
        let rec = enmp
            .get(r..r + 0x5c)
            .ok_or_else(|| invalid("ENMP truncated"))?;
        if u16_at(rec, 0)? == id {
            let mut weakness = [0; 7];
            for (i, w) in weakness.iter_mut().enumerate() {
                *w = u16_at(rec, 72 + i * 2)?;
            }
            let mut pools = [0; 32];
            for (k, p) in pools.iter_mut().enumerate() {
                *p = u16_at(rec, 4 + k * 2)?;
            }
            return Ok(Enemy {
                id,
                level: u16_at(rec, 2)?,
                hp_percent: u16_at(rec, 4)?,
                damage_cap: u16_at(rec, 68)?,
                damage_min: u16_at(rec, 70)?,
                weakness,
                exp: u16_at(rec, 0x56)?,
                prize: u16_at(rec, 0x58)?,
                bonus: u16_at(rec, 0x5a)?,
                pools,
            });
        }
    }
    Err(invalid("ENMP id absent"))
}

/// The ENMP row a sheet binds (FUN_1403ebf30): `id`, else row 69 (0x45).
pub fn enemy_or_fallback(enmp: &[u8], id: u16) -> io::Result<Enemy> {
    enemy(enmp, id).or_else(|_| enemy(enmp, 0x45))
}

/// The `patn` table (DAT_142ae59d0, read by FUN_1403ee9b0): u32 version,
/// u32 count, then 32-byte rows: byte 0 the state pattern bit, byte 1 + r
/// the reaction that replaces original reaction r (0xff keeps it).
pub fn reaction_patterns(patn: &[u8]) -> io::Result<Vec<[u8; 32]>> {
    let count = u32_at(patn, 4)? as usize;
    (0..count)
        .map(|k| {
            let r = 8 + k * 32;
            patn.get(r..r + 32)
                .and_then(|row| row.try_into().ok())
                .ok_or_else(|| invalid("PATN truncated"))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelRow {
    pub hp: u16,
    pub strength: u16,
    pub defense: u16,
    pub max_strength: u16,
    pub min_strength: u16,
    pub exp: u16,
}

/// LVPM row for an enemy level 1..=99 (99 x 12 bytes, no header).
pub fn enemy_level(lvpm: &[u8], level: u16) -> io::Result<LevelRow> {
    if !(1..=99).contains(&level) {
        return Err(invalid("LVPM level outside 1..=99"));
    }
    let r = (level as usize - 1) * 12;
    Ok(LevelRow {
        hp: u16_at(lvpm, r)?,
        strength: u16_at(lvpm, r + 2)?,
        defense: u16_at(lvpm, r + 4)?,
        max_strength: u16_at(lvpm, r + 6)?,
        min_strength: u16_at(lvpm, r + 8)?,
        exp: u16_at(lvpm, r + 10)?,
    })
}

/// One przt record: the nine prize counts and three item drops with their
/// chances in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prize {
    pub counts: [u8; 9],
    pub items: [(u16, u16); 3],
}

/// The przt record of an id; an absent id falls back to 0x48
/// (FUN_1403ecc70). Rows are 0x18 bytes after u32 version, u32 count.
pub fn prize(przt: &[u8], id: u16) -> io::Result<Prize> {
    let count = u32_at(przt, 4)? as usize;
    let find = |want: u16| -> io::Result<Option<Prize>> {
        for k in 0..count {
            let r = 8 + k * 0x18;
            let rec = przt
                .get(r..r + 0x18)
                .ok_or_else(|| invalid("PRZT truncated"))?;
            if u16_at(rec, 0)? == want {
                let mut counts = [0u8; 9];
                counts.copy_from_slice(&rec[2..11]);
                let mut items = [(0u16, 0u16); 3];
                for (i, it) in items.iter_mut().enumerate() {
                    *it = (u16_at(rec, 0xc + i * 4)?, u16_at(rec, 0xe + i * 4)?);
                }
                return Ok(Some(Prize { counts, items }));
            }
        }
        Ok(None)
    };
    match find(id)? {
        Some(p) => Ok(p),
        None => find(0x48)?.ok_or_else(|| invalid("PRZT has no fallback row")),
    }
}

/// One `bons` row: what a bonus level gives a character (`FUN_1403edf90`).
/// Rows are 0x10 bytes after u32 version, u32 count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bonus {
    /// +0: the bonus id a battle's record or an enemy names.
    pub id: u8,
    /// +1: the character the row is for (1 Sora, 2 Donald, 3 Goofy).
    pub character: u8,
    /// +2 / +3: Maximum HP and MP gained (halved for the party on Critical).
    pub hp: u8,
    pub mp: u8,
    /// +4: Drive gauge bars gained (the gauge stops at 9).
    pub drive: u8,
    /// +5 / +6 / +7: Item, Accessory and Armor slots gained.
    pub item_slots: u8,
    pub accessory_slots: u8,
    pub armor_slots: u8,
    /// +8 / +0xa: abilities learned.
    pub abilities: [u16; 2],
}

/// Every row of bonus `id` in table order (`FUN_1403edca0` walks the rows
/// that follow the first match).
pub fn bonus_rows(bons: &[u8], id: u8) -> io::Result<Vec<Bonus>> {
    let count = u32_at(bons, 4)? as usize;
    let mut out = Vec::new();
    for k in 0..count {
        let r = 8 + k * 0x10;
        let rec = bons
            .get(r..r + 0x10)
            .ok_or_else(|| invalid("BONS truncated"))?;
        if rec[0] == id {
            out.push(Bonus {
                id: rec[0],
                character: rec[1],
                hp: rec[2],
                mp: rec[3],
                drive: rec[4],
                item_slots: rec[5],
                accessory_slots: rec[6],
                armor_slots: rec[7],
                abilities: [u16_at(rec, 8)?, u16_at(rec, 10)?],
            });
        }
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerLevel {
    /// EXP of the row (u32 at +0; the Status "Next LV" subtracts the
    /// save's EXP from the current level's row, FUN_1403a14b0).
    pub exp: u32,
    pub strength: u8,
    pub magic: u8,
    pub defense: u8,
    pub ap: u8,
    /// +8 / +0xa / +0xc: the ability items learned at this level, one per
    /// growth choice (FUN_1403ed7d0 reads +8 + choice x 2).
    pub abilities: [u16; 3],
}

/// LVUP row for a character id and level: the character block is at the
/// u32 word (x4) in the pointer table at 8 + id*4; row L is at block + 4 +
/// (L - 1) * 16 (executable FUN_1403ed2e0).
pub fn player_level(lvup: &[u8], character: usize, level: usize) -> io::Result<PlayerLevel> {
    if !(1..=99).contains(&level) {
        return Err(invalid("LVUP level outside 1..=99"));
    }
    let block = u32_at(lvup, 8 + character * 4)? as usize * 4;
    let r = block + 4 + (level - 1) * 16;
    let row = lvup
        .get(r..r + 16)
        .ok_or_else(|| invalid("LVUP truncated"))?;
    Ok(PlayerLevel {
        exp: u32::from_le_bytes([row[0], row[1], row[2], row[3]]),
        strength: row[4],
        magic: row[5],
        defense: row[6],
        ap: row[7],
        abilities: [
            u16::from_le_bytes([row[8], row[9]]),
            u16::from_le_bytes([row[10], row[11]]),
            u16::from_le_bytes([row[12], row[13]]),
        ],
    })
}

/// One fmlv row (8 bytes): id = form << 4 | level, the growth step,
/// the ability granted on reaching the level and the EXP this level needs
/// to reach the next (FUN_1403e8380; bsearch on the id byte,
/// comparator FUN_1403e87e0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormLevel {
    pub id: u8,
    pub growth: u8,
    pub ability: u16,
    pub exp: u32,
}

/// The fmlv table (u32 version, u32 count, 8-byte rows).
pub fn form_levels(fmlv: &[u8]) -> io::Result<Vec<FormLevel>> {
    let count = u32_at(fmlv, 4)? as usize;
    if count > 0x100 {
        return Err(invalid("FMLV count implausible"));
    }
    (0..count)
        .map(|k| {
            let r = 8 + k * 8;
            let row = fmlv
                .get(r..r + 8)
                .ok_or_else(|| invalid("FMLV truncated"))?;
            Ok(FormLevel {
                id: row[0],
                growth: row[1],
                ability: u16_at(row, 2)?,
                exp: u32_at(row, 4)?,
            })
        })
        .collect()
}

/// The ability lists of the plrp records whose key is `key` and whose
/// character byte has bit 7 set (a Drive form record: index `byte & 0x7f`),
/// in table order: FUN_1403e4530 adds each non-zero u16 of the 32 at +0xc
/// to the form's record with FUN_1403a1680.
pub fn form_starting_abilities(plrp: &[u8], key: u16) -> io::Result<Vec<(u8, Vec<u16>)>> {
    let count = u32_at(plrp, 4)? as usize;
    if count > 0x400 {
        return Err(invalid("PLRP count implausible"));
    }
    let mut out = Vec::new();
    for k in 0..count {
        let r = 8 + k * 0x80;
        let rec = plrp
            .get(r..r + 0x80)
            .ok_or_else(|| invalid("PLRP truncated"))?;
        if u16_at(rec, 0)? != key || rec[2] & 0x80 == 0 {
            continue;
        }
        let mut list = Vec::new();
        for i in 0..32 {
            let v = u16_at(rec, 0xc + i * 2)?;
            if v != 0 {
                list.push(v);
            }
        }
        out.push((rec[2] & 0x7f, list));
    }
    Ok(out)
}

/// The new-game row of a character: the plrp row of key 0 that
/// FUN_1403e4530(0) applies (key 1 rows are applied by no code): its
/// item-slot count (+0xb) and the 32 ids at +0xc (bit 15 = equipped).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewGameRow {
    /// Row bytes 5..8: the AP, Strength, Magic and Defense bonuses (record
    /// +8..+0xb), 9 / 0xa the armour and accessory slots (+0x10 / +0x11).
    pub bonus: [u8; 4],
    pub armor_slots: u8,
    pub accessory_slots: u8,
    pub item_slots: u8,
    pub ids: Vec<u16>,
}

pub fn new_game_row(plrp: &[u8], character: u8) -> io::Result<Option<NewGameRow>> {
    let count = u32_at(plrp, 4)? as usize;
    if count > 0x400 {
        return Err(invalid("PLRP count implausible"));
    }
    for k in 0..count {
        let rec = plrp
            .get(8 + k * 0x80..8 + (k + 1) * 0x80)
            .ok_or_else(|| invalid("PLRP truncated"))?;
        if u16_at(rec, 0)? == 0 && rec[2] == character {
            return Ok(Some(NewGameRow {
                bonus: [rec[5], rec[6], rec[7], rec[8]],
                armor_slots: rec[9],
                accessory_slots: rec[0xa],
                item_slots: rec[0xb],
                ids: (0..32)
                    .map(|i| u16_at(rec, 0xc + i * 2))
                    .collect::<io::Result<_>>()?,
            }));
        }
    }
    Ok(None)
}

/// Raw ability slots from every PLRP template for a member character.
/// Includes ordinary, developer and difficulty templates; callers choose policy.
pub fn member_ability_lists(plrp: &[u8], character: u8) -> io::Result<Vec<Vec<u16>>> {
    let count = u32_at(plrp, 4)? as usize;
    if count > 0x400 {
        return Err(invalid("PLRP count implausible"));
    }
    let mut lists = Vec::new();
    for k in 0..count {
        let rec = plrp
            .get(8 + k * 0x80..8 + (k + 1) * 0x80)
            .ok_or_else(|| invalid("PLRP truncated"))?;
        if rec[2] == character {
            lists.push(
                (0..32)
                    .map(|i| u16_at(rec, 0xc + i * 2))
                    .collect::<io::Result<_>>()?,
            );
        }
    }
    Ok(lists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_game_row_is_the_key_0_one() {
        let mut bytes = vec![0u8; 8 + 2 * 0x80];
        bytes[4..8].copy_from_slice(&2u32.to_le_bytes());
        for (k, key) in [1u16, 0].into_iter().enumerate() {
            let at = 8 + k * 0x80;
            bytes[at..at + 2].copy_from_slice(&key.to_le_bytes());
            bytes[at + 2] = 2;
            bytes[at + 9] = 1;
            bytes[at + 0xa] = 2;
            bytes[at + 0xb] = 2 + k as u8;
            bytes[at + 0xc..at + 0xe].copy_from_slice(&(0x8000 | (7 + k as u16)).to_le_bytes());
        }
        let row = new_game_row(&bytes, 2).unwrap().unwrap();
        assert_eq!((row.item_slots, row.ids[0], row.ids.len()), (3, 0x8008, 32));
        assert_eq!(
            (row.armor_slots, row.accessory_slots, row.bonus),
            (1, 2, [0; 4])
        );
        assert_eq!(new_game_row(&bytes, 3).unwrap(), None);
    }

    #[test]
    fn member_templates_preserve_slots_and_exclude_other_characters_and_forms() {
        let mut bytes = vec![0u8; 8 + 4 * 0x80];
        bytes[4..8].copy_from_slice(&4u32.to_le_bytes());
        for (k, character) in [1u8, 2, 0x81, 1].into_iter().enumerate() {
            let at = 8 + k * 0x80;
            bytes[at..at + 2].copy_from_slice(&(k as u16).to_le_bytes());
            bytes[at + 2] = character;
            bytes[at + 0xc..at + 0xe].copy_from_slice(&(0x8000 | (1000 + k as u16)).to_le_bytes());
        }
        let lists = member_ability_lists(&bytes, 1).unwrap();
        assert_eq!(lists.len(), 2);
        assert!(lists.iter().all(|l| l.len() == 32));
        assert_eq!((lists[0][0], lists[1][0]), (0x8000 | 1000, 0x8000 | 1003));
        assert!(lists[0][1..].iter().all(|&s| s == 0));
        assert!(member_ability_lists(&bytes[..bytes.len() - 1], 1).is_err());
    }

    #[test]
    fn reads_form_levels_and_form_records() {
        let mut fmlv = vec![2, 0, 0, 0, 2, 0, 0, 0];
        fmlv.extend([0x11, 1, 0, 0, 0x50, 0, 0, 0]);
        fmlv.extend([0x12, 1, 0x81, 0x01, 0xa0, 0, 0, 0]);
        let rows = form_levels(&fmlv).unwrap();
        assert_eq!(rows[1].ability, 0x181);
        assert_eq!((rows[0].id, rows[0].exp), (0x11, 0x50));
        let mut plrp = vec![1, 0, 0, 0, 2, 0, 0, 0];
        let mut rec = [0u8; 0x80];
        rec[2] = 0x81;
        rec[0xc..0x10].copy_from_slice(&[0x5e, 0x80, 0xa2, 0x80]);
        plrp.extend(rec);
        let mut sora = [0u8; 0x80];
        sora[2] = 1;
        plrp.extend(sora);
        let forms = form_starting_abilities(&plrp, 0).unwrap();
        assert_eq!(forms, vec![(1, vec![0x805e, 0x80a2])]);
    }

    #[test]
    fn reads_synthetic_rows() {
        let mut atkp = vec![6, 0, 0, 0, 1, 0, 0, 0];
        let mut rec = [0u8; 48];
        rec[2] = 126;
        rec[6] = 100;
        rec[10] = 3;
        rec[12] = 125;
        rec[0x1c..0x20].copy_from_slice(&(-1i32).to_le_bytes());
        atkp.extend(rec);
        let a = attack(&atkp, 126).unwrap();
        assert_eq!((a.power, a.reaction, a.knockback), (100, 3, [125, 0]));
        assert_eq!((a.hit_sound, a.hit_effect), (-1, 0));
        assert!(attack(&atkp, 127).is_err());

        // Level variants: step 0 Power 150, step 1 Power 25 (a switch
        // row applies on every other hit past its step).
        let mut atkp = vec![6, 0, 0, 0, 3, 0, 0, 0];
        for (step, power, switch) in [(0u8, 150u16, 0u8), (1, 25, 0), (3, 40, 2)] {
            let mut rec = [0u8; 48];
            rec[0] = step;
            rec[2] = 19;
            rec[6..8].copy_from_slice(&power.to_le_bytes());
            rec[0x25] = switch;
            atkp.extend(rec);
        }
        let rows = attack_steps(&atkp, 19).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(step_row(&rows, 0).power, 150);
        assert_eq!(step_row(&rows, 1).power, 25);
        assert_eq!(step_row(&rows, 3).power, 25);
        assert_eq!(step_row(&rows, 4).power, 40);
        assert_eq!(step_row(&rows, 5).power, 25);

        let mut lvpm = vec![0u8; 24];
        lvpm[12..14].copy_from_slice(&21u16.to_le_bytes());
        assert_eq!(enemy_level(&lvpm, 2).unwrap().hp, 21);
        assert!(enemy_level(&lvpm, 0).is_err());

        let mut lvup = vec![0u8; 64];
        lvup[12..16].copy_from_slice(&8u32.to_le_bytes());
        lvup[32 + 4 + 4] = 2;
        assert_eq!(player_level(&lvup, 1, 1).unwrap().strength, 2);
    }

    #[test]
    fn bonus_rows_by_id() {
        let mut t = vec![2, 0, 0, 0, 3, 0, 0, 0];
        for (id, ch, hp) in [(2u8, 1u8, 5u8), (2, 2, 3), (3, 1, 9)] {
            let mut r = [0u8; 16];
            r[0] = id;
            r[1] = ch;
            r[2] = hp;
            r[8] = 0x11;
            t.extend(r);
        }
        let rows = bonus_rows(&t, 2).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].character, rows[0].hp), (1, 5));
        assert_eq!(rows[1].abilities, [0x11, 0]);
        assert!(bonus_rows(&t, 9).unwrap().is_empty());
        assert!(bonus_rows(&t[..t.len() - 1], 3).is_err());
    }
}
