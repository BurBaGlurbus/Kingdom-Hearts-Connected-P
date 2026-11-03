//! Raw PTYA party attack sets from 00battle, following the pinned OpenKH
//! `docs/kh2/file/type/00battle.md` layout (7a3b945c538d32c6a285128c98aefba093f52ceb).
//! Fields keep their raw values; combo selection rules are not inferred here.
use serde::Serialize;
use std::io;

pub const ENTRY_SIZE: usize = 68;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PtyaEntry {
    pub id: u8,
    pub kind: u8,
    pub sub: i8,
    pub combo_offset: i8,
    pub flag: u32,
    pub motion_id: u16,
    pub next_motion_id: u16,
    /// Jump, jump max, jump min, speed min/max, near, far, low, high,
    /// inner min/max, blend time, distance adjust (public labels only).
    pub float_bits: [u32; 13],
    pub ability: u16,
    pub score: u16,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PtyaTable {
    pub pointers: Vec<u32>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn u32_at(bytes: &[u8], at: usize) -> io::Result<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| invalid("truncated PTYA word"))
}

pub fn read_table(bytes: &[u8]) -> io::Result<PtyaTable> {
    if bytes.len() > 1_048_576 {
        return Err(invalid("PTYA exceeds inspection bounds"));
    }
    let count = u32_at(bytes, 4)? as usize;
    if !(1..=4096).contains(&count) {
        return Err(invalid("PTYA pointer count outside inspection bounds"));
    }
    let pointers = (0..count)
        .map(|i| u32_at(bytes, 8 + i * 4))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(PtyaTable { pointers })
}

/// Reads the set addressed by one pointer slot. Pointer offsets are relative
/// to the PTYA payload start, as in the public documentation.
pub fn read_set(bytes: &[u8], table: &PtyaTable, slot: usize) -> io::Result<Vec<PtyaEntry>> {
    let offset = *table
        .pointers
        .get(slot)
        .ok_or_else(|| invalid("PTYA pointer slot absent"))? as usize;
    let count = u32_at(bytes, offset)? as usize;
    if count > 1024 {
        return Err(invalid("PTYA set count outside inspection bounds"));
    }
    let body = bytes
        .get(offset + 4..offset + 4 + count * ENTRY_SIZE)
        .ok_or_else(|| invalid("PTYA set exceeds payload"))?;
    Ok(body
        .as_chunks::<ENTRY_SIZE>()
        .0
        .iter()
        .map(|e| PtyaEntry {
            id: e[0],
            kind: e[1],
            sub: e[2] as i8,
            combo_offset: e[3] as i8,
            flag: u32::from_le_bytes(e[4..8].try_into().unwrap()),
            motion_id: u16::from_le_bytes(e[8..10].try_into().unwrap()),
            next_motion_id: u16::from_le_bytes(e[10..12].try_into().unwrap()),
            float_bits: std::array::from_fn(|i| {
                u32::from_le_bytes(e[12 + i * 4..16 + i * 4].try_into().unwrap())
            }),
            ability: u16::from_le_bytes(e[64..66].try_into().unwrap()),
            score: u16::from_le_bytes(e[66..68].try_into().unwrap()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 8 + 8 + 4 + 2 * ENTRY_SIZE];
        bytes[0..4].copy_from_slice(&2u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&2u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&16u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&16u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&2u32.to_le_bytes());
        let first = 20;
        bytes[first] = 6;
        bytes[first + 1] = 1;
        bytes[first + 2] = 0xff;
        bytes[first + 8..first + 10].copy_from_slice(&153u16.to_le_bytes());
        bytes[first + 12..first + 16].copy_from_slice(&17f32.to_le_bytes());
        let second = first + ENTRY_SIZE;
        bytes[second] = 5;
        bytes[second + 1] = 3;
        bytes[second + 2] = 6;
        bytes[second + 66..second + 68].copy_from_slice(&7u16.to_le_bytes());
        bytes
    }

    #[test]
    fn reads_aliased_set_and_raw_fields() {
        let bytes = fixture();
        let table = read_table(&bytes).unwrap();
        assert_eq!(table.pointers, vec![16, 16]);
        let set = read_set(&bytes, &table, 1).unwrap();
        assert_eq!(set.len(), 2);
        assert_eq!((set[0].id, set[0].kind, set[0].sub), (6, 1, -1));
        assert_eq!(set[0].motion_id, 153);
        assert_eq!(f32::from_bits(set[0].float_bits[0]), 17.);
        assert_eq!((set[1].sub, set[1].score), (6, 7));
    }

    #[test]
    fn rejects_truncation_and_absent_slots() {
        let bytes = fixture();
        let table = read_table(&bytes).unwrap();
        assert!(read_set(&bytes, &table, 2).is_err());
        assert!(read_set(&bytes[..bytes.len() - 1], &table, 0).is_err());
        assert!(read_table(&bytes[..6]).is_err());
    }
}
