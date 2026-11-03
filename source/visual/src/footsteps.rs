//! Footsteps: the sound id Sora's motion frame trigger 2 plays.
//!
//! A step plays `bank * 1000 + p0 + material * 16` (research footstep-01).
//! The bank is the Sora entry (+0x0E) of the room's 12soundinfo row (one
//! 24-byte row per room after a `u32` count, one sub-file per world); the
//! material is byte 0 of the floor polygon (`collision::FloorInfo::
//! ground`, 0 with no polygon). p0 0 and 4 (run and walk) pick one of four
//! variants and p0 13 one of two, by a counter that advances once per such
//! step; a step with no material (or no row) plays nothing and leaves the
//! counter alone.
//!
//! The exe also forces material 0x12 in three physics modes (10, 20, 21)
//! that the test room does not enter; they are not modelled.

/// A row is 24 bytes: reverb, 3D rate, environment wave, ambient bank and
/// number, spot, one unread word, then the Sora, Donald, Goofy, friend and
/// other footstep banks (`i16` each).
const ROW: usize = 24;
const SORA_BANK: usize = 0x0e;
/// The room's ambient bed: its bank (+0x06) and sound number (+0x08, -1
/// for none).
const AMBIENT_BANK: usize = 0x06;
const AMBIENT_NUMBER: usize = 0x08;

/// One world's 12soundinfo rows.
pub struct WorldInfo {
    rows: Vec<[u8; ROW]>,
}

impl WorldInfo {
    /// Reads `u32 count` then `count` rows; None when the data is short.
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let count = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
        let rows = bytes
            .get(4..)?
            .as_chunks::<ROW>()
            .0
            .iter()
            .take(count)
            .copied()
            .collect::<Vec<[u8; ROW]>>();
        (rows.len() == count).then_some(Self { rows })
    }

    /// Sora's footstep bank in `room`; None without a row (the exe's NULL
    /// row), and a negative value where the row names no bank.
    pub fn sora_bank(&self, room: u8) -> Option<i16> {
        let row = self.rows.get(room as usize)?;
        Some(i16::from_le_bytes([row[SORA_BANK], row[SORA_BANK + 1]]))
    }

    fn word(&self, room: u8, at: usize) -> Option<i16> {
        let row = self.rows.get(room as usize)?;
        Some(i16::from_le_bytes([row[at], row[at + 1]]))
    }

    /// The ambient bed's request id (`bank * 1000 + number`) in `room`;
    /// None without a row or with no bed (number -1, or no bank).
    pub fn ambient(&self, room: u8) -> Option<u32> {
        let bank = self.word(room, AMBIENT_BANK)?;
        let number = self.word(room, AMBIENT_NUMBER)?;
        (bank > 0 && number >= 0).then(|| bank as u32 * 1000 + number as u32)
    }

    /// Every distinct bank of an ambient bed in the world.
    pub fn ambient_banks(&self) -> Vec<u16> {
        let mut banks: Vec<u16> = (0..self.rows.len())
            .filter_map(|r| self.word(r as u8, AMBIENT_BANK))
            .filter(|&b| b > 0)
            .map(|b| b as u16)
            .collect();
        banks.sort_unstable();
        banks.dedup();
        banks
    }

    /// Every distinct positive Sora bank in the world.
    pub fn sora_banks(&self) -> Vec<u16> {
        let mut banks: Vec<u16> = (0..self.rows.len())
            .filter_map(|r| self.sora_bank(r as u8))
            .filter(|&b| b > 0)
            .map(|b| b as u16)
            .collect();
        banks.sort_unstable();
        banks.dedup();
        banks
    }
}

/// The actor's step counter (`u8`, zero at spawn).
#[derive(Default)]
pub struct Steps {
    counter: u8,
}

impl Steps {
    /// The request id for a step with trigger parameter `p0` on a floor of
    /// `material` in a room whose Sora bank is `bank` (None: no row).
    pub fn id(&mut self, bank: Option<i16>, p0: i16, material: u8) -> Option<u32> {
        let bank = bank?;
        if material == 0 {
            return None;
        }
        let mut p = i32::from(p0);
        match p0 {
            0 | 4 => {
                p += i32::from(self.counter & 3);
                self.counter = self.counter.wrapping_add(1);
            }
            13 => {
                p = 13 + i32::from(self.counter & 1);
                self.counter = self.counter.wrapping_add(1);
            }
            _ => {}
        }
        let id = i32::from(bank) * 1000 + p + i32::from(material) * 16;
        u32::try_from(id).ok().filter(|&id| id > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(banks: &[i16]) -> Vec<u8> {
        let mut out = (banks.len() as u32).to_le_bytes().to_vec();
        for &b in banks {
            let mut row = [0u8; ROW];
            row[SORA_BANK..SORA_BANK + 2].copy_from_slice(&b.to_le_bytes());
            out.extend_from_slice(&row);
        }
        out
    }

    #[test]
    fn reads_the_sora_bank_by_room() {
        let info = WorldInfo::read(&world(&[688, 691, -1])).unwrap();
        assert_eq!(info.sora_bank(0), Some(688));
        assert_eq!(info.sora_bank(1), Some(691));
        assert_eq!(info.sora_bank(2), Some(-1));
        assert_eq!(info.sora_bank(3), None);
        assert_eq!(info.sora_banks(), vec![688, 691]);
    }

    #[test]
    fn reads_the_ambient_bed_by_room() {
        let mut bytes = world(&[688, 688, 688]);
        let put = |bytes: &mut Vec<u8>, room: usize, at: usize, v: i16| {
            let o = 4 + room * ROW + at;
            bytes[o..o + 2].copy_from_slice(&v.to_le_bytes());
        };
        put(&mut bytes, 0, AMBIENT_BANK, 3);
        put(&mut bytes, 0, AMBIENT_NUMBER, 12);
        put(&mut bytes, 1, AMBIENT_BANK, 3);
        put(&mut bytes, 1, AMBIENT_NUMBER, -1);
        let info = WorldInfo::read(&bytes).unwrap();
        assert_eq!(info.ambient(0), Some(3012));
        assert_eq!(info.ambient(1), None);
        assert_eq!(info.ambient(2), None);
        assert_eq!(info.ambient(3), None);
        assert_eq!(info.ambient_banks(), vec![3]);
    }

    #[test]
    fn short_data_is_rejected() {
        let mut bytes = world(&[688, 691]);
        bytes.truncate(bytes.len() - 1);
        assert!(WorldInfo::read(&bytes).is_none());
        assert!(WorldInfo::read(&[1, 0]).is_none());
    }

    #[test]
    fn run_and_walk_cycle_four_variants_of_the_floor_group() {
        let mut s = Steps::default();
        // Material 1 in bank 688: run is 688016..688019, walk 688020..688023.
        let run: Vec<_> = (0..5).map(|_| s.id(Some(688), 0, 1)).collect();
        assert_eq!(
            run,
            [688016, 688017, 688018, 688019, 688016].map(Some).to_vec()
        );
        // The counter is shared: five run steps leave it at 5, so a walk
        // step takes variant 1 of the walk group.
        assert_eq!(s.id(Some(688), 4, 1), Some(688021));
    }

    #[test]
    fn jump_and_land_do_not_advance_the_counter() {
        let mut s = Steps::default();
        assert_eq!(s.id(Some(688), 9, 1), Some(688025));
        assert_eq!(s.id(Some(688), 10, 1), Some(688026));
        assert_eq!(s.id(Some(688), 0, 1), Some(688016));
    }

    #[test]
    fn no_material_or_no_row_plays_nothing_and_keeps_the_counter() {
        let mut s = Steps::default();
        assert_eq!(s.id(Some(688), 0, 0), None);
        assert_eq!(s.id(None, 0, 1), None);
        assert_eq!(s.id(Some(688), 0, 1), Some(688016));
    }

    #[test]
    fn a_row_without_a_bank_still_advances_the_counter() {
        let mut s = Steps::default();
        assert_eq!(s.id(Some(-1), 0, 1), None);
        assert_eq!(s.id(Some(688), 0, 1), Some(688017));
    }

    #[test]
    fn p0_13_alternates_two_variants() {
        let mut s = Steps::default();
        assert_eq!(s.id(Some(688), 13, 2), Some(688045));
        assert_eq!(s.id(Some(688), 13, 2), Some(688046));
        assert_eq!(s.id(Some(688), 13, 2), Some(688045));
    }
}
