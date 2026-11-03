//! SKLT weapon-attachment table from the system archive, following the
//! public OpenKH `03system.md` layout at
//! 7a3b945c538d32c6a285128c98aefba093f52ceb: a type word (1), an entry count,
//! then per character an id and the primary and secondary weapon bones.
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkltEntry {
    pub character_id: u32,
    pub primary_bone: i16,
    pub secondary_bone: i16,
}

pub fn read(bytes: &[u8]) -> io::Result<Vec<SkltEntry>> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    let word = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
    };
    if word(0) != Some(1) {
        return Err(invalid("SKLT type word is not 1"));
    }
    let count = word(4).ok_or_else(|| invalid("truncated SKLT header"))? as usize;
    if count > 4096 {
        return Err(invalid("SKLT entry count outside inspection bounds"));
    }
    let records = bytes
        .get(8..8 + count * 8)
        .ok_or_else(|| invalid("truncated SKLT entries"))?;
    Ok(records
        .as_chunks::<8>()
        .0
        .iter()
        .map(|r| SkltEntry {
            character_id: u32::from_le_bytes(r[..4].try_into().unwrap()),
            primary_bone: i16::from_le_bytes([r[4], r[5]]),
            secondary_bone: i16::from_le_bytes([r[6], r[7]]),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_synthetic_entries_and_rejects_bad_headers() {
        let mut bytes = vec![1, 0, 0, 0, 2, 0, 0, 0];
        bytes.extend([7, 0, 0, 0, 10, 0, 255, 255]);
        bytes.extend([9, 0, 0, 0, 3, 0, 4, 0]);
        let entries = read(&bytes).unwrap();
        assert_eq!(
            entries[0],
            SkltEntry {
                character_id: 7,
                primary_bone: 10,
                secondary_bone: -1
            }
        );
        assert_eq!(entries[1].secondary_bone, 4);
        assert!(read(&bytes[..12]).is_err());
        bytes[0] = 2;
        assert!(read(&bytes).is_err());
    }
}
