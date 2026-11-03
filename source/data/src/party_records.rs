//! Raw PRTY offset records, following OpenKH PrtyFile.Read at
//! 7a3b945c538d32c6a285128c98aefba093f52ceb. No character mapping or units inferred.
use serde::Serialize;
use std::{collections::BTreeSet, io};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PrtyRecord {
    pub offset: u32,
    pub overlaps_table: bool,
    pub words: [u32; 13],
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PrtyTable {
    pub offsets: Vec<u32>,
    pub records: Vec<PrtyRecord>,
}

pub fn read_prty(bytes: &[u8]) -> io::Result<PrtyTable> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    if !(4..=1_048_576).contains(&bytes.len()) {
        return Err(invalid("PRTY size outside inspection bounds"));
    }
    let count = i32::from_le_bytes(bytes[..4].try_into().unwrap());
    if !(1..=4096).contains(&count) {
        return Err(invalid("PRTY offset count outside inspection bounds"));
    }
    let table_end = 4 + count as usize * 4;
    let table = bytes
        .get(4..table_end)
        .ok_or_else(|| invalid("truncated PRTY offset table"))?;
    let mut offsets = Vec::with_capacity(count as usize);
    for word in table.as_chunks::<4>().0 {
        let offset = i32::from_le_bytes(*word);
        if offset < 0 {
            return Err(invalid("negative PRTY offset"));
        }
        offsets.push(offset as u32);
    }
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    // The public reader skips the first offset but retains a repeated zero
    // later in the table. Preserve its raw words and expose the overlap.
    for &offset in offsets.iter().skip(1) {
        if !seen.insert(offset) {
            continue;
        }
        let start = offset as usize;
        let data = bytes
            .get(start..start + 52)
            .ok_or_else(|| invalid("PRTY record exceeds payload"))?;
        let mut words = [0; 13];
        for (output, input) in words.iter_mut().zip(data.as_chunks::<4>().0) {
            *output = u32::from_le_bytes(*input);
        }
        records.push(PrtyRecord {
            offset,
            overlaps_table: start < table_end,
            words,
        });
    }
    Ok(PrtyTable { offsets, records })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(offsets: &[i32]) -> Vec<u8> {
        let mut bytes = (offsets.len() as i32).to_le_bytes().to_vec();
        for offset in offsets {
            bytes.extend(offset.to_le_bytes());
        }
        bytes.resize(128, 0);
        for i in 0..13u32 {
            bytes[64 + i as usize * 4..68 + i as usize * 4]
                .copy_from_slice(&(0x7fc00000 + i).to_le_bytes());
        }
        bytes
    }

    #[test]
    fn preserves_aliases_header_overlap_and_nonfinite_raw_words() {
        let result = read_prty(&fixture(&[127, 64, 64, 0])).unwrap();
        assert_eq!(result.offsets, [127, 64, 64, 0]);
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].offset, 64);
        assert!(!result.records[0].overlaps_table);
        assert_eq!(
            result.records[0].words,
            std::array::from_fn(|i| 0x7fc00000 + i as u32)
        );
        assert!(result.records[1].overlaps_table);
        assert_eq!(result.records[1].words[..5], [4, 127, 64, 64, 0]);
    }

    #[test]
    fn rejects_unbounded_counts_signed_offsets_and_truncated_records() {
        for count in [-1i32, 0, 4097, i32::MAX] {
            assert!(read_prty(&count.to_le_bytes()).is_err());
        }
        assert!(read_prty(&[2, 0, 0, 0]).is_err());
        for offset in [-1, 77, i32::MAX] {
            assert!(read_prty(&fixture(&[0, offset])).is_err());
        }
        assert!(read_prty(&vec![0; 1_048_577]).is_err());
    }
}
