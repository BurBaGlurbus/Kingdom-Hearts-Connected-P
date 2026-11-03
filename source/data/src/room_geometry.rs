//! Map model and display geometry adapted from pinned OpenKH (NOTICE).
//! Raw draw flags and coordinates are retained; installed materials are separate.
use crate::{vif::Unpacker, vpu};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io;

#[derive(Debug, Serialize)]
pub struct RoomVertex {
    pub position_bits: [u32; 3],
    pub uv_bits: [u32; 2],
    pub color_bits: [u32; 4],
}
#[derive(Debug, Serialize)]
pub struct RoomPacket {
    pub program_counter: usize,
    pub memory_sha256: String,
    pub data: vpu::VpuData,
}
#[derive(Debug, Serialize)]
pub struct RoomChunk {
    pub header_words: [u32; 4],
    pub texture_index: i16,
    pub transparency_flag: i16,
    pub dma_tags: Vec<[u32; 4]>,
    pub vif_code_sha256: String,
    pub packets: Vec<RoomPacket>,
    pub vertices: Vec<RoomVertex>,
    pub indices: Vec<u32>,
}
#[derive(Debug, Serialize)]
pub struct RoomModel {
    pub header_words: [u32; 8],
    pub rendering_groups: Vec<Vec<u16>>,
    pub chain_remap: Vec<u16>,
    pub chunks: Vec<RoomChunk>,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn span(bytes: &[u8], offset: usize, len: usize) -> io::Result<&[u8]> {
    bytes
        .get(
            offset
                ..offset
                    .checked_add(len)
                    .ok_or_else(|| invalid("map span overflow"))?,
        )
        .ok_or_else(|| invalid("map span outside model"))
}
fn words<const N: usize>(bytes: &[u8]) -> [u32; N] {
    std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}
fn list(bytes: &[u8], offset: usize) -> io::Result<Vec<u16>> {
    let mut result = Vec::new();
    for i in 0..=65536 {
        let value = u16::from_le_bytes(span(bytes, offset + i * 2, 2)?.try_into().unwrap());
        if value == u16::MAX {
            return Ok(result);
        }
        result.push(value);
    }
    Err(invalid("unterminated map group list"))
}
pub fn read(bytes: &[u8]) -> io::Result<RoomModel> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(invalid("map model exceeds 32 MiB"));
    }
    let bytes = bytes
        .get(0x90..)
        .ok_or_else(|| invalid("missing model reserved area"))?;
    let header = words::<8>(span(bytes, 0, 32)?);
    if header[0] != 2 || header[3] != 0 {
        return Err(invalid("unsupported map model type or linked model"));
    }
    let count = (header[4] & 0xffff) as usize;
    if count > 4096 {
        return Err(invalid("map exceeds 4096 chunks"));
    }
    span(bytes, 32, count * 16)?;
    let group_table = header[6] as usize;
    let first_group = words::<1>(span(bytes, group_table, 4)?)[0] as usize;
    let group_bytes = first_group
        .checked_sub(group_table)
        .ok_or_else(|| invalid("map group table reversed"))?;
    if !group_bytes.is_multiple_of(4) || group_bytes / 4 > 32767 {
        return Err(invalid("invalid map group count"));
    }
    let mut rendering_groups = Vec::new();
    let mut group_items = 0;
    for entry in span(bytes, group_table, group_bytes)?.as_chunks::<4>().0 {
        let group = list(bytes, u32::from_le_bytes(*entry) as usize)?;
        group_items += group.len();
        if group_items > 262144 {
            return Err(invalid("map groups exceed bounded total"));
        }
        rendering_groups.push(group);
    }
    let remap_offset = words::<1>(span(bytes, header[7] as usize, 4)?)[0] as usize;
    let chain_remap = list(bytes, remap_offset)?;
    let mut chunks = Vec::new();
    let mut total_vertices = 0;
    let mut total_indices = 0;
    for chunk_index in 0..count {
        let raw = span(bytes, 32 + chunk_index * 16, 16)?;
        let header_words = words::<4>(raw);
        let texture_index = i16::from_le_bytes(raw[4..6].try_into().unwrap());
        let transparency_flag = i16::from_le_bytes(raw[10..12].try_into().unwrap());
        let mut cursor = header_words[0] as usize;
        let mut code = Vec::new();
        let mut dma_tags = Vec::new();
        loop {
            let tag = words::<4>(span(bytes, cursor, 16)?);
            let length = (tag[0] as usize & 0xffff) * 16;
            code.extend_from_slice(span(bytes, cursor + 8, 8 + length)?);
            dma_tags.push(tag);
            if code.len() > 1024 * 1024 || dma_tags.len() > 4096 {
                return Err(invalid("map DMA limits exceeded"));
            }
            cursor += 16 + length;
            if ((tag[0] >> 28) & 7) >= 2 {
                break;
            }
        }
        let mut unpacker = Unpacker::new(&code)?;
        let mut packets = Vec::new();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut recent = 0usize;
        let mut ring = [0u32; 4];
        while unpacker.run()? != "end" {
            let previous_indices = indices.len();
            let data = vpu::read_map_data(&unpacker.output.memory)?
                .ok_or_else(|| invalid("unsupported map VU packet type"))?;
            if !data.colors.is_empty() && data.colors.len() != data.strips.len() {
                return Err(invalid("map color/strip counts disagree"));
            }
            for (i, &[u, v, vertex, function]) in data.strips.iter().enumerate() {
                let position = usize::try_from(vertex)
                    .ok()
                    .and_then(|v| data.vertex_bits.get(v))
                    .ok_or_else(|| invalid("map strip coordinate reference invalid"))?;
                let color = data.colors.get(i).copied().unwrap_or([128; 4]);
                ring[recent & 3] = vertices.len() as u32;
                recent += 1;
                vertices.push(RoomVertex {
                    position_bits: [position[0], position[1], position[2]],
                    uv_bits: [
                        ((u as i16) as f32 / 4096.).to_bits(),
                        ((v as i16) as f32 / 4096.).to_bits(),
                    ],
                    color_bits: color.map(|c| (c as f32 / 128.).to_bits()),
                });
                if function != 0x10 && recent < 3 {
                    return Err(invalid("map draws before three strip vertices"));
                }
                let a = ring[(recent - 1) & 3];
                let b = ring[recent.wrapping_sub(3) & 3];
                let c = ring[recent.wrapping_sub(2) & 3];
                match function {
                    0 => indices.extend([a, b, c, a, c, b]),
                    0x10 => {}
                    0x20 => indices.extend([a, b, c]),
                    0x30 => indices.extend([a, c, b]),
                    _ => return Err(invalid("unknown map strip draw function")),
                }
            }
            total_vertices += data.strips.len();
            total_indices += indices.len() - previous_indices;
            if total_vertices > 2_000_000 || total_indices > 12_000_000 || packets.len() >= 4096 {
                return Err(invalid("map geometry limits exceeded"));
            }
            packets.push(RoomPacket {
                program_counter: unpacker.output.program_counter,
                memory_sha256: format!("{:x}", Sha256::digest(unpacker.output.memory)),
                data,
            });
        }
        chunks.push(RoomChunk {
            header_words,
            texture_index,
            transparency_flag,
            dma_tags,
            vif_code_sha256: format!("{:x}", Sha256::digest(&code)),
            packets,
            vertices,
            indices,
        });
    }
    Ok(RoomModel {
        header_words: header,
        rendering_groups,
        chain_remap,
        chunks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut code = Vec::new();
        let header = [0u32, 0, 0, 0, 4, 4, 0, 0, 0, 0, 0, 0, 3, 8, 0, 0];
        for word in std::iter::once(0x6c04_0000).chain(header) {
            code.extend(word.to_le_bytes());
        }
        code.extend(0x6c04_0004u32.to_le_bytes());
        for (i, function) in [0x10u32, 0x10, 0x20, 0x30].into_iter().enumerate() {
            for word in [(-4096i32) as u32, 70000, (i % 3) as u32, function] {
                code.extend(word.to_le_bytes());
            }
        }
        code.extend(0x6c03_0008u32.to_le_bytes());
        for i in 0..3 {
            for value in [i as f32, 0., -0., 1.] {
                code.extend(value.to_le_bytes());
            }
        }
        code.extend(0x1400_0000u32.to_le_bytes());
        assert_eq!(code.len(), 192);
        let mut bytes = vec![0; 0x90 + 64 + 16 + 192];
        let body = &mut bytes[0x90..];
        for (offset, value) in [
            (0, 2u32),
            (16, 1),
            (24, 48),
            (28, 56),
            (32, 64),
            (36, 7),
            (48, 52),
            (56, 60),
            (64, 0x6000_000c),
        ] {
            body[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        body[52..56].copy_from_slice(&[0, 0, 255, 255]);
        body[60..64].copy_from_slice(&[0, 0, 255, 255]);
        body[72..72 + code.len()].copy_from_slice(&code);
        bytes
    }
    #[test]
    fn decodes_dma_map_strip_order_signed_uv_and_default_color() {
        let map = read(&fixture()).unwrap();
        assert_eq!(map.rendering_groups, [vec![0]]);
        assert_eq!(map.chain_remap, [0]);
        let chunk = &map.chunks[0];
        assert_eq!(chunk.texture_index, 7);
        assert_eq!(chunk.vertices.len(), 4);
        assert_eq!(chunk.indices, [2, 0, 1, 3, 2, 1]);
        assert_eq!(
            chunk.vertices[0].uv_bits,
            [(-1f32).to_bits(), (4464f32 / 4096.).to_bits()]
        );
        assert_eq!(chunk.vertices[0].color_bits, [1f32.to_bits(); 4]);
        assert_eq!(chunk.vertices[0].position_bits[2], (-0f32).to_bits());
    }
    #[test]
    fn rejects_truncation_links_bad_tables_and_coordinate_references() {
        let good = fixture();
        for length in [0, 143, 160, good.len() - 1] {
            assert!(read(&good[..length]).is_err());
        }
        for (offset, value) in [
            (0x90, 3u32),
            (0x9c, 1),
            (0xa8, u32::MAX),
            (0xb0, u32::MAX),
            (0x90 + 72 + 68 + 4 + 8, 99),
        ] {
            let mut bad = good.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(read(&bad).is_err(), "field {offset}");
        }
    }
}
