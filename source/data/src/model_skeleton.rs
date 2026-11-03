//! Bounded structural observations, not a mesh or animation decoder.
//! Layout: OpenKH Mdlx.Model.cs at 7a3b945c538d32c6a285128c98aefba093f52ceb.
//! ModelCommon.cs at that pin interprets the first 16 bone bytes differently;
//! both raw views survive. No transform convention or installed meaning is chosen.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io};

pub const PROFILE: &str = "mdlx_legacy_3_4_structure_v1";
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_BLOCKS: usize = 16;
const MAX_BONES: usize = 2048;
const MAX_GROUPS: usize = 4096;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct BoneRecord {
    pub header_i32: [i32; 4],
    pub header_i16: [i16; 8],
    /// Twelve IEEE-754 words, retained without float normalization.
    pub transform_bits: [u32; 12],
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LegacyHierarchyObservations {
    pub duplicate_indices: usize,
    pub roots: usize,
    pub missing_parent_indices: usize,
    pub cyclic_parent_chains: Option<usize>,
    pub parents_at_or_after_child: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ModelBlock {
    pub offset: usize,
    pub header_words: [i32; 8],
    pub declared_bone_count: usize,
    pub header_word_16_high_i16: i16,
    pub group_headers: Vec<[i32; 8]>,
    pub unknown_data_sha256: String,
    pub bones: Vec<BoneRecord>,
    /// The legacy Index/Parent interpretation only; no runtime rule asserted.
    pub legacy_hierarchy: LegacyHierarchyObservations,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ModelStructure {
    pub profile: &'static str,
    pub blocks: Vec<ModelBlock>,
    pub runtime_verified: bool,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn range(bytes: &[u8], offset: usize, length: usize) -> io::Result<&[u8]> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| invalid("model range overflow"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| invalid("model range exceeds payload"))
}

fn relative(base: usize, offset: i32) -> io::Result<usize> {
    if offset < 0 {
        return Err(invalid("negative model offset"));
    }
    base.checked_add(offset as usize)
        .ok_or_else(|| invalid("model offset overflow"))
}

fn words<const N: usize>(bytes: &[u8]) -> [i32; N] {
    std::array::from_fn(|i| i32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}

fn hierarchy(bones: &[BoneRecord]) -> LegacyHierarchyObservations {
    let mut indices = HashMap::new();
    let mut result = LegacyHierarchyObservations {
        duplicate_indices: 0,
        roots: 0,
        missing_parent_indices: 0,
        cyclic_parent_chains: Some(0),
        parents_at_or_after_child: 0,
    };
    for (ordinal, bone) in bones.iter().enumerate() {
        if indices.insert(bone.header_i32[0], ordinal).is_some() {
            result.duplicate_indices += 1;
            result.cyclic_parent_chains = None;
        }
    }
    for (ordinal, bone) in bones.iter().enumerate() {
        let parent = bone.header_i32[1];
        if parent == -1 {
            result.roots += 1;
        } else if let Some(&position) = indices.get(&parent) {
            result.parents_at_or_after_child += usize::from(position >= ordinal);
        } else {
            result.missing_parent_indices += 1;
        }
    }
    // Duplicate indices make the parent graph ambiguous. Otherwise memoize each
    // chain in linear time: 0 unknown, 1 current chain, 2 terminal, 3 cyclic.
    if result.duplicate_indices == 0 {
        let mut states = vec![0; bones.len()];
        for start in 0..bones.len() {
            if states[start] != 0 {
                continue;
            }
            let mut chain = Vec::new();
            let mut current = start;
            let outcome = loop {
                match states[current] {
                    1 | 3 => break 3,
                    2 => break 2,
                    _ => {}
                }
                states[current] = 1;
                chain.push(current);
                let parent = bones[current].header_i32[1];
                if parent == -1 {
                    break 2;
                }
                let Some(&next) = indices.get(&parent) else {
                    break 2;
                };
                current = next;
            };
            for position in chain {
                states[position] = outcome;
            }
        }
        result.cyclic_parent_chains = Some(states.iter().filter(|&&state| state == 3).count());
    }
    result
}

pub fn read_model_structure(bytes: &[u8]) -> io::Result<ModelStructure> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid("model exceeds 32 MiB inspection limit"));
    }
    let mut offset = 0x90;
    let mut blocks = Vec::new();
    loop {
        if blocks.len() == MAX_BLOCKS {
            return Err(invalid("model exceeds 16 linked blocks"));
        }
        let header = range(bytes, offset, 32)?;
        let header_words = words::<8>(header);
        if (blocks.is_empty() && header_words[0] != 3) || !matches!(header_words[0], 3 | 4) {
            return Err(invalid("unqualified model type for explicit profile"));
        }
        let count = i16::from_le_bytes(header[16..18].try_into().unwrap());
        let group_count = header_words[7];
        if count < 0 || count as usize > MAX_BONES {
            return Err(invalid("invalid model bone count"));
        }
        if group_count < 0 || group_count as usize > MAX_GROUPS {
            return Err(invalid("invalid model group count"));
        }
        let groups = range(bytes, offset + 32, group_count as usize * 32)?;
        // Legacy ReadSubModel reads this fixed region, including when offset=0.
        // Its SHA preserves the observation without assigning skeleton semantics.
        let unknown = range(bytes, relative(offset, header_words[6])?, 0x120)?;
        let mut bones = Vec::new();
        if header_words[5] != 0 {
            let bone_bytes = range(
                bytes,
                relative(offset, header_words[5])?,
                count as usize * 64,
            )?;
            for record in bone_bytes.as_chunks::<64>().0 {
                bones.push(BoneRecord {
                    header_i32: words::<4>(&record[..16]),
                    header_i16: std::array::from_fn(|i| {
                        i16::from_le_bytes(record[i * 2..i * 2 + 2].try_into().unwrap())
                    }),
                    transform_bits: std::array::from_fn(|i| {
                        u32::from_le_bytes(record[16 + i * 4..20 + i * 4].try_into().unwrap())
                    }),
                });
            }
        }
        blocks.push(ModelBlock {
            offset,
            header_words,
            declared_bone_count: count as usize,
            header_word_16_high_i16: i16::from_le_bytes(header[18..20].try_into().unwrap()),
            group_headers: groups
                .as_chunks::<32>()
                .0
                .iter()
                .map(|r| words::<8>(r))
                .collect(),
            unknown_data_sha256: format!("{:x}", Sha256::digest(unknown)),
            legacy_hierarchy: hierarchy(&bones),
            bones,
        });
        if header_words[3] == 0 {
            break;
        }
        if header_words[3] < 32 {
            return Err(invalid("model link is not forward past current header"));
        }
        offset = relative(offset, header_words[3])?;
        // The pinned legacy reader ends when the next substream is empty.
        // Preserve the link word, allowing only exact EOF; a partial header
        // or an out-of-range link must still fail on the next iteration.
        if offset == bytes.len() {
            break;
        }
    }
    Ok(ModelStructure {
        profile: PROFILE,
        blocks,
        runtime_verified: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0x90 + 32 + 0x120 + 3 * 64 + 0x120];
        put(&mut bytes, 0x90, 3);
        put(&mut bytes, 0x90 + 16, 3);
        put(&mut bytes, 0x90 + 20, 32 + 0x120);
        put(&mut bytes, 0x90 + 24, 32);
        for (ordinal, parent) in [-1, 0, 1].into_iter().enumerate() {
            let base = 0x90 + 32 + 0x120 + ordinal * 64;
            put(&mut bytes, base, ordinal as i32);
            put(&mut bytes, base + 4, parent);
            put(&mut bytes, base + 16, 0x7fc0_1234); // authored NaN payload
            put(&mut bytes, base + 20, i32::MIN); // negative zero
        }
        bytes
    }
    #[test]
    fn retains_both_headers_exact_float_bits_and_pointerless_shadow() {
        let mut bytes = fixture();
        let shadow = 32 + 0x120 + 3 * 64;
        put(&mut bytes, 0x90 + 12, shadow as i32);
        put(&mut bytes, 0x90 + shadow, 4);
        put(&mut bytes, 0x90 + shadow + 16, 3);
        let result = read_model_structure(&bytes).unwrap();
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].bones[0].header_i16[2..4], [-1, -1]);
        assert_eq!(
            result.blocks[0].bones[0].transform_bits[..2],
            [0x7fc0_1234, 0x8000_0000]
        );
        assert_eq!(result.blocks[0].legacy_hierarchy.roots, 1);
        assert_eq!(
            result.blocks[0].legacy_hierarchy.cyclic_parent_chains,
            Some(0)
        );
        assert_eq!(result.blocks[1].declared_bone_count, 3);
        assert!(result.blocks[1].bones.is_empty());
        assert!(!result.runtime_verified);
    }
    #[test]
    fn exact_eof_link_terminates_but_partial_or_outside_links_fail() {
        let mut bytes = fixture();
        let end = (bytes.len() - 0x90) as i32;
        put(&mut bytes, 0x90 + 12, end);
        let model = read_model_structure(&bytes).unwrap();
        assert_eq!(model.blocks.len(), 1);
        assert_eq!(model.blocks[0].header_words[3], end);
        for link in [end - 1, end + 1] {
            put(&mut bytes, 0x90 + 12, link);
            assert!(read_model_structure(&bytes).is_err());
        }
    }
    #[test]
    fn observes_invalid_and_ambiguous_parent_graphs_without_rewriting_records() {
        let mut bytes = fixture();
        let bone = 0x90 + 32 + 0x120;
        put(&mut bytes, bone + 4, 1);
        let cyclic = read_model_structure(&bytes).unwrap();
        assert_eq!(
            cyclic.blocks[0].legacy_hierarchy.cyclic_parent_chains,
            Some(3)
        );
        put(&mut bytes, bone + 4, 99);
        assert_eq!(
            read_model_structure(&bytes).unwrap().blocks[0]
                .legacy_hierarchy
                .missing_parent_indices,
            1
        );
        put(&mut bytes, bone + 64, 0);
        assert_eq!(
            read_model_structure(&bytes).unwrap().blocks[0]
                .legacy_hierarchy
                .duplicate_indices,
            1
        );
        assert_eq!(
            read_model_structure(&bytes).unwrap().blocks[0]
                .legacy_hierarchy
                .cyclic_parent_chains,
            None
        );
    }
    #[test]
    fn rejects_unqualified_types_bad_ranges_counts_and_links() {
        for (offset, value) in [
            (0, 2),
            (12, -1),
            (12, 1),
            (12, i32::MAX),
            (16, -1),
            (16, 2049),
            (20, -1),
            (20, i32::MAX),
            (24, i32::MAX),
            (28, -1),
            (28, 4097),
        ] {
            let mut bytes = fixture();
            put(&mut bytes, 0x90 + offset, value);
            assert!(
                read_model_structure(&bytes).is_err(),
                "offset={offset} value={value}"
            );
        }
        for len in [0, 0x90 + 31, 0x90 + 32 + 0x120 + 3 * 64 - 1] {
            assert!(read_model_structure(&fixture()[..len]).is_err());
        }
        let mut bytes = vec![0; 0x90 + 17 * 0x120];
        for block in 0..17 {
            let base = 0x90 + block * 0x120;
            put(&mut bytes, base, 3);
            put(&mut bytes, base + 12, if block == 16 { 0 } else { 0x120 });
        }
        assert!(read_model_structure(&bytes).is_err());
        assert!(read_model_structure(&vec![0; MAX_BYTES + 1]).is_err());
    }
}
