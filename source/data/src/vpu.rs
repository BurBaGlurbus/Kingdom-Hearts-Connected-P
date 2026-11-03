//! Lossless type-1 VU data views following the pinned public VpuPacket reader.
//! No bind transform, vertex skinning, triangles or type-2 shadow layout inferred.
use serde::Serialize;
use std::io;

#[derive(Debug, Serialize)]
pub struct VpuData {
    pub normal_words: Option<[i32; 2]>,
    pub matrix_counts: Vec<i32>,
    pub strips: Vec<[i32; 4]>,
    pub vertex_bits: Vec<[u32; 4]>,
    pub colors: Vec<[i32; 4]>,
    pub normal_bits: Vec<[u32; 4]>,
    pub weight_group_counts: Vec<i32>,
    pub weighted_indices: Vec<Vec<Vec<i32>>>,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn scalar_words(memory: &[u8], offset: usize, count: usize) -> io::Result<Vec<i32>> {
    let end = offset
        .checked_add(
            count
                .checked_mul(4)
                .ok_or_else(|| invalid("VU count overflow"))?,
        )
        .ok_or_else(|| invalid("VU range overflow"))?;
    let bytes = memory
        .get(offset..end)
        .ok_or_else(|| invalid("VU range exceeds memory"))?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| i32::from_le_bytes(*w))
        .collect())
}
fn area(memory: &[u8], offset: i32, count: i32, stride: usize) -> io::Result<Vec<i32>> {
    if offset < 0 || count < 0 {
        return Err(invalid("negative VU range"));
    }
    scalar_words(memory, offset as usize * 16, count as usize * stride)
}
fn records(words: Vec<i32>) -> Vec<[i32; 4]> {
    words.as_chunks::<4>().0.to_vec()
}
fn bits(words: Vec<i32>) -> Vec<[u32; 4]> {
    records(words)
        .into_iter()
        .map(|w| w.map(|n| n as u32))
        .collect()
}
pub fn read_type1_data(memory: &[u8]) -> io::Result<Option<VpuData>> {
    read_data(memory, false)
}

/// Includes the pinned map packet layout (type zero) when explicitly requested.
pub fn read_map_data(memory: &[u8]) -> io::Result<Option<VpuData>> {
    read_data(memory, true)
}

fn read_data(memory: &[u8], map: bool) -> io::Result<Option<VpuData>> {
    if memory.len() > crate::vif::MEMORY_BYTES {
        return Err(invalid("VU input exceeds 16 KiB"));
    }
    let header = scalar_words(memory, 0, 16)?;
    if header[0] != if map { 0 } else { 1 } {
        return Ok(None);
    }
    let has_normals =
        i64::from(header[13]) != i64::from(header[5]) + i64::from(header[4]) + i64::from(header[8]);
    let normal_words: Option<[i32; 2]> = if has_normals {
        Some(scalar_words(memory, 64, 2)?.try_into().unwrap())
    } else {
        None
    };
    let normal_bits = if let Some([count, offset]) = normal_words {
        bits(area(memory, offset, count, 4)?)
    } else {
        Vec::new()
    };
    let matrix_counts = area(memory, header[6], header[15], 1)?;
    let strips = records(area(memory, header[5], header[4], 4)?);
    let colors = records(area(memory, header[9], header[8], 4)?);
    let vertex_bits = bits(area(memory, header[13], header[12], 4)?);
    let weight_group_counts = area(memory, header[11], header[10], 1)?;
    if weight_group_counts.len() > 32
        || weight_group_counts
            .iter()
            .any(|&n| !(0..=4096).contains(&n))
    {
        return Err(invalid("VU weight groups exceed bounded profile"));
    }
    // Literal public reader route: continue after the weight-count array and
    // align each group. Its VertexIndexOffset field is preserved, not substituted.
    let mut cursor = header[11] as usize * 16 + weight_group_counts.len() * 4;
    let mut weighted_indices = Vec::new();
    for (group, &count) in weight_group_counts.iter().enumerate() {
        cursor = (cursor + 15) & !15;
        let width = group + 1;
        let values = scalar_words(memory, cursor, count as usize * width)?;
        cursor += values.len() * 4;
        weighted_indices.push(
            values
                .chunks_exact(width)
                .map(|entry| entry.to_vec())
                .collect(),
        );
    }
    Ok(Some(VpuData {
        normal_words,
        matrix_counts,
        strips,
        vertex_bits,
        colors,
        normal_bits,
        weight_group_counts,
        weighted_indices,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 256];
        for (offset, value) in [
            (0, 1),
            (16, 1),
            (20, 4),
            (24, 7),
            (40, 2),
            (44, 8),
            (48, 2),
            (52, 5),
            (56, 999),
            (60, 1),
            (64, -16),
            (68, 7),
            (72, 1),
            (76, 0x20),
            (80, 0x7fc0_1234),
            (84, i32::MIN),
            (112, 2),
            (128, 1),
            (132, 1),
            (144, 0),
            (160, 0),
            (164, 1),
        ] {
            put(&mut bytes, offset, value);
        }
        bytes
    }
    #[test]
    fn retains_strips_float_bits_and_public_aligned_weight_groups() {
        let data = read_type1_data(&fixture()).unwrap().unwrap();
        assert_eq!(data.strips, [[-16, 7, 1, 0x20]]);
        assert_eq!(data.vertex_bits[0][..2], [0x7fc0_1234, 0x8000_0000]);
        assert_eq!(data.matrix_counts, [2]);
        assert_eq!(data.weighted_indices, vec![vec![vec![0]], vec![vec![0, 1]]]);
        assert!(data.normal_words.is_none());
        let mut bytes = fixture();
        put(&mut bytes, 0, 2);
        assert!(read_type1_data(&bytes).unwrap().is_none());
    }
    #[test]
    fn preserves_extended_normal_header_and_float_bit_patterns() {
        let mut bytes = vec![0; 192];
        for (offset, value) in [
            (0, 1),
            (16, 1),
            (20, 5),
            (24, 8),
            (48, 1),
            (52, 7),
            (60, 1),
            (64, 1),
            (68, 6),
            (96, 0x7fc0_beef),
            (100, i32::MIN),
            (128, 1),
        ] {
            put(&mut bytes, offset, value);
        }
        let data = read_type1_data(&bytes).unwrap().unwrap();
        assert_eq!(data.normal_words, Some([1, 6]));
        assert_eq!(data.normal_bits[0][..2], [0x7fc0_beef, 0x8000_0000]);
        assert_eq!(data.vertex_bits.len(), 1);
        assert_eq!(data.strips.len(), 1);
        put(&mut bytes, 64, -1);
        assert!(read_type1_data(&bytes).is_err());
    }
    #[test]
    fn rejects_negative_truncated_and_overlong_weight_ranges() {
        for (offset, value) in [
            (16, -1),
            (20, -1),
            (40, 33),
            (44, i32::MAX),
            (48, i32::MAX),
            (128, -1),
            (132, 4097),
        ] {
            let mut bytes = fixture();
            put(&mut bytes, offset, value);
            assert!(read_type1_data(&bytes).is_err());
        }
        assert!(read_type1_data(&fixture()[..163]).is_err());
        assert!(read_type1_data(&[]).is_err());
        assert!(read_type1_data(&vec![0; crate::vif::MEMORY_BYTES + 1]).is_err());
    }
}
