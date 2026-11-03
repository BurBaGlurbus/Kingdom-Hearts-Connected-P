//! The slot list the file menu shows: 99 slots named by their two digits
//! (`BISLPM-66675FM-%02d`), each empty, holding a valid save, or holding
//! something the checksum refuses. The summary is what FUN_1402ee350 /
//! FUN_1402ee4a0 copy out of a save into the menu's 0x28-byte slot record.

use crate::{
    container::Container,
    data::{Problem, SaveData},
    slot_name,
};

/// Slots in the menu (`FUN_1402ed980` and the scan loops stop at 99).
pub const SLOT_COUNT: usize = 99;

/// Offsets inside the save image that the summary and the menu read.
pub mod field {
    /// Current world index (`u8`; above 0x12 shows as none), area, entrance.
    pub const WORLD: usize = 0xc;
    pub const AREA: usize = 0xd;
    pub const ENTRANCE: usize = 0xe;
    /// Bit 8 of the `u32` here is the flag FUN_140152390 reads.
    pub const FINAL_FLAG: usize = 0x1ed8;
    pub const MUNNY: usize = 0x2440;
    /// Play time in frames (216000 per hour: FUN_140157390).
    pub const PLAY_FRAMES: usize = 0x2444;
    /// Difficulty (0 Beginner, 1 Standard, 2 Proud, 3 Critical).
    pub const DIFFICULTY: usize = 0x2498;
    /// Bit 0 set while the player is Roxas (FUN_1402ee350 asks for 0xe).
    pub const ROXAS: usize = 0x249c;
    /// Party records, 0x114 bytes each; Sora and Roxas share record 1.
    pub const PARTY: usize = 0x24f0;
    pub const PARTY_RECORD: usize = 0x114;
    /// Level inside a party record.
    pub const RECORD_LEVEL: usize = 0xf;
    /// Background picked when the file was written (`u8`) and its flag bit.
    pub const BACKGROUND: usize = 0x419c;
}

/// What a slot's row shows (FUN_1402ee4a0 with flag 0x10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub world: u8,
    pub area: u8,
    pub frames: u32,
    pub munny: u32,
    pub difficulty: u8,
    pub level: u8,
    pub background: u8,
    pub background_flag: bool,
    pub final_flag: bool,
}

impl Summary {
    pub fn of(data: &SaveData) -> Self {
        // FUN_1403e60c0 maps both Sora (1) and Roxas (0xe) to record 1, so
        // the level read is the same whichever the player is.
        let level = data.u8_at(field::PARTY + field::RECORD_LEVEL);
        let world = data.u8_at(field::WORLD);
        Self {
            world: if world > 0x12 { 0xff } else { world },
            area: data.u8_at(field::AREA),
            frames: data.u32_at(field::PLAY_FRAMES),
            munny: data.u32_at(field::MUNNY),
            difficulty: data.u8_at(field::DIFFICULTY),
            level,
            background: data.u8_at(field::BACKGROUND),
            background_flag: data.u8_at(field::BACKGROUND + 1) & 1 != 0,
            final_flag: data.u32_at(field::FINAL_FLAG) >> 8 & 1 != 0,
        }
    }

    /// Hours and minutes as the menu shows them (216000 frames an hour,
    /// 3600 a minute; seconds are not shown).
    pub fn hours_minutes(&self) -> (u32, u32) {
        (self.frames / 216_000, self.frames % 216_000 / 3600)
    }
}

/// One slot of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    /// No file (the original's flag 0x20: "There is no data").
    Empty,
    /// A save whose checksum, signature and version are right, and when
    /// its file was last written (`time_t`).
    Valid { summary: Summary, modified: i64 },
    /// A file the checks refuse ("Save data is corrupt.").
    Corrupt,
}

impl Slot {
    /// A save the menu can load or overwrite after asking.
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }
}

/// The 99 slots of `container`, read the way the menu's scan reads them.
pub fn list(container: &Container) -> Vec<Slot> {
    (0..SLOT_COUNT)
        .map(|i| match container.read(&slot_name(i)) {
            None => Slot::Empty,
            Some((entry, bytes)) => match SaveData::from_bytes(bytes) {
                Ok(data) => Slot::Valid {
                    summary: Summary::of(&data),
                    modified: entry.modified,
                },
                Err(
                    Problem::Length(_)
                    | Problem::Magic(_)
                    | Problem::Version(_)
                    | Problem::Checksum { .. },
                ) => Slot::Corrupt,
            },
        })
        .collect()
}

/// The save in `slot`, if it is a valid one.
pub fn load(container: &Container, slot: usize) -> Option<SaveData> {
    let (_, bytes) = container.read(&slot_name(slot))?;
    SaveData::from_bytes(bytes).ok()
}

/// Writes `data` (checksum already sealed) into `slot`.
pub fn store(
    container: &mut Container,
    slot: usize,
    data: &SaveData,
    now: i64,
) -> Result<(), crate::container::Error> {
    container.write(&slot_name(slot), data.bytes(), now)
}

/// Removes `slot`'s file.
pub fn delete(container: &mut Container, slot: usize) {
    container.remove(&slot_name(slot));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cipher::key_from_steam_id;

    fn save_with(level: u8, frames: u32) -> SaveData {
        let mut d = SaveData::blank();
        d.set_u8(field::WORLD, 2);
        d.set_u8(field::AREA, 8);
        d.set_u8(field::PARTY + field::RECORD_LEVEL, level);
        d.set_u32(field::PLAY_FRAMES, frames);
        d.set_u32(field::MUNNY, 1234);
        d.seal();
        d
    }

    #[test]
    fn lists_empty_valid_and_corrupt_slots() {
        let mut c = Container::empty(&key_from_steam_id(1));
        store(&mut c, 3, &save_with(12, 216_000 * 2 + 3600 * 5), 10).unwrap();
        let mut broken = save_with(1, 0);
        broken.set_u32(field::MUNNY, 9);
        c.write(&slot_name(4), broken.bytes(), 11).unwrap();
        let slots = list(&c);
        assert_eq!(slots.len(), SLOT_COUNT);
        assert_eq!(slots[0], Slot::Empty);
        assert_eq!(slots[4], Slot::Corrupt);
        let Slot::Valid { summary: s, .. } = &slots[3] else {
            panic!("slot 3 should be valid")
        };
        assert_eq!((s.level, s.world, s.area, s.munny), (12, 2, 8, 1234));
        assert_eq!(s.hours_minutes(), (2, 5));
        assert!(load(&c, 3).is_some());
        assert!(load(&c, 4).is_none());
        delete(&mut c, 3);
        assert_eq!(list(&c)[3], Slot::Empty);
    }

    #[test]
    fn world_outside_the_table_shows_as_none() {
        let mut d = SaveData::blank();
        d.set_u8(field::WORLD, 0x40);
        d.seal();
        assert_eq!(Summary::of(&d).world, 0xff);
    }
}
