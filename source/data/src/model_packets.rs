//! Bounded legacy DMA selection and first-run VIF memory observations.
use crate::{model_skeleton::read_model_structure, vif};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io;

pub const PROFILE: &str = "openkh_legacy_dma_vif_first_run_v1";
const MAX_ENTRIES: usize = 4096;
const MAX_PACKETS: usize = 4096;
const MAX_TAGS: usize = 65536;

#[derive(Serialize)]
pub struct DmaTag {
    pub offset: usize,
    pub words: [u32; 4],
}
#[derive(Serialize)]
pub struct Packet {
    pub offset: usize,
    pub code_offset: usize,
    pub code_length: usize,
    pub base_address: i32,
    pub code_sha256: String,
    pub tags: Vec<DmaTag>,
    pub memory_sha256: String,
    pub program_counter: usize,
    pub stop_state: &'static str,
    pub vpu_header_words: [i32; 16],
    pub vpu_data: Option<crate::vpu::VpuData>,
}
#[derive(Serialize)]
pub struct PacketGroup {
    pub block_offset: usize,
    pub group_ordinal: usize,
    pub matrix_entries: Vec<i32>,
    pub packets: Vec<Packet>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn range(bytes: &[u8], offset: usize, length: usize) -> io::Result<&[u8]> {
    bytes
        .get(
            offset
                ..offset
                    .checked_add(length)
                    .ok_or_else(|| invalid("DMA range overflow"))?,
        )
        .ok_or_else(|| invalid("DMA range exceeds model"))
}
fn position(base: usize, offset: i32) -> io::Result<usize> {
    if offset < 0 {
        return Err(invalid("negative DMA offset"));
    }
    base.checked_add(offset as usize)
        .ok_or_else(|| invalid("DMA offset overflow"))
}
pub fn read_packet_groups(bytes: &[u8]) -> io::Result<Vec<PacketGroup>> {
    let structure = read_model_structure(bytes)?;
    let mut groups = Vec::new();
    let mut total_packets = 0;
    let mut total_tags = 0;
    let mut total_code_bytes = 0;
    for block in &structure.blocks {
        for (group_ordinal, header) in block.group_headers.iter().enumerate() {
            let list = position(block.offset, header[5])?;
            let count = i32::from_le_bytes(range(bytes, list, 4)?.try_into().unwrap());
            if count < 0 || count as usize > MAX_ENTRIES {
                return Err(invalid("invalid DMA matrix-list count"));
            }
            let matrix_entries: Vec<_> = range(bytes, list + 4, count as usize * 4)?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|w| i32::from_le_bytes(*w))
                .collect();
            let dma = position(block.offset, header[4])? - block.offset;
            let mut starts = vec![dma];
            let mut cursor = dma + 16;
            for &entry in &matrix_entries {
                if entry == -1 {
                    starts.push(cursor + 16);
                    cursor += 32;
                } else {
                    cursor += 16;
                }
            }
            total_packets += starts.len();
            if total_packets > MAX_PACKETS {
                return Err(invalid("DMA exceeds 4096 cumulative packets"));
            }
            let mut packets = Vec::new();
            for offset in starts {
                let mut tags = Vec::new();
                let mut tag_cursor = offset;
                loop {
                    total_tags += 1;
                    if total_tags > MAX_TAGS || tags.len() == MAX_ENTRIES {
                        return Err(invalid("DMA tag count exceeds bounded limits"));
                    }
                    let raw = range(bytes, block.offset + tag_cursor, 16)?;
                    let words = std::array::from_fn(|i| {
                        u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap())
                    });
                    tags.push(DmaTag {
                        offset: tag_cursor,
                        words,
                    });
                    if words[0] & 0xffff == 0 {
                        break;
                    }
                    tag_cursor += 16;
                }
                let code_offset = (tags[0].words[1] & 0x7fff_ffff) as usize;
                let code_length = (tags[0].words[0] & 0xffff) as usize * 16;
                total_code_bytes += code_length;
                if total_code_bytes > 32 * 1024 * 1024 {
                    return Err(invalid("DMA exceeds 32 MiB cumulative VIF reads"));
                }
                let code = range(bytes, block.offset + code_offset, code_length)?;
                let base_address = if code_length == 0 {
                    0
                } else {
                    tags.get(1)
                        .ok_or_else(|| invalid("DMA payload has no following tag"))?
                        .words[3] as i32
                };
                let unpacked = vif::unpack_first_run(code)?;
                let vpu_data = crate::vpu::read_type1_data(&unpacked.memory)?;
                packets.push(Packet {
                    offset,
                    code_offset,
                    code_length,
                    base_address,
                    code_sha256: format!("{:x}", Sha256::digest(code)),
                    tags,
                    memory_sha256: format!("{:x}", Sha256::digest(unpacked.memory)),
                    program_counter: unpacked.program_counter,
                    stop_state: unpacked.stop_state,
                    vpu_data,
                    vpu_header_words: std::array::from_fn(|i| {
                        i32::from_le_bytes(unpacked.memory[i * 4..i * 4 + 4].try_into().unwrap())
                    }),
                });
            }
            groups.push(PacketGroup {
                block_offset: block.offset,
                group_ordinal,
                matrix_entries,
                packets,
            });
        }
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 1024];
        for (offset, value) in [
            (0x90, 3),
            (0x90 + 24, 64),
            (0x90 + 28, 1),
            (0xb0 + 16, 352),
            (0xb0 + 20, 432),
            (0x90 + 432, 3),
            (0x90 + 436, 7),
            (0x90 + 440, -1),
            (0x90 + 444, 9),
        ] {
            put(&mut bytes, offset, value);
        }
        // Two terminated runs selected around a separator. Payload address uses
        // the public high-bit mask; matrix references are not byte addresses.
        for offset in [352, 400] {
            put(&mut bytes, 0x90 + offset, 1);
            put(&mut bytes, 0x90 + offset + 4, i32::MIN | 336);
            put(&mut bytes, 0x90 + offset + 28, 123);
        }
        bytes
    }
    #[test]
    fn selects_split_packets_and_retains_raw_tags_and_matrix_entries() {
        let bytes = fixture();
        let groups = read_packet_groups(&bytes).unwrap();
        assert_eq!(groups[0].matrix_entries, [7, -1, 9]);
        assert_eq!(groups[0].packets.len(), 2);
        assert_eq!(groups[0].packets[1].offset, 400);
        assert_eq!(groups[0].packets[0].base_address, 123);
        assert_eq!(groups[0].packets[0].code_length, 16);
        assert_eq!(groups[0].packets[0].program_counter, 16);
    }
    #[test]
    fn rejects_bad_matrix_counts_offsets_and_unterminated_tags() {
        for (offset, value) in [
            (0x90 + 432, -1),
            (0x90 + 432, 4097),
            (0xb0 + 16, -1),
            (0xb0 + 20, i32::MAX),
            (0x90 + 352 + 4, 1024),
        ] {
            let mut bytes = fixture();
            put(&mut bytes, offset, value);
            assert!(read_packet_groups(&bytes).is_err());
        }
        let mut bytes = fixture();
        for raw in bytes[0x90 + 352..].as_chunks_mut::<16>().0 {
            raw[..4].copy_from_slice(&1i32.to_le_bytes());
        }
        assert!(read_packet_groups(&bytes).is_err());
    }
}
