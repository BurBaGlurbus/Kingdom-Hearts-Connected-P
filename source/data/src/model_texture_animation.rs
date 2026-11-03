//! Bounded TEXA reader observations; no runtime slot, timing or sprite choice.
//! Adapted from pinned OpenKH TextureFooterData/TextureFrame/BitsUtil (NOTICE).
use crate::{model_texture, model_texture_routing::TexaHeader};
use serde::Serialize;
use std::{collections::HashSet, io};

pub const PROFILE: &str = "openkh_texa_bounded_frame_graph_v1";
const FRAME_BYTES: usize = 8;
#[derive(Serialize)]
pub struct FrameRecord {
    pub index: i32,
    pub body_offset: usize,
    pub words: [u16; 4],
    pub control: u16,
    pub frame_index_delta: i32,
    pub next_index: i32,
}
#[derive(Serialize)]
pub struct FrameGroup {
    pub first_position: u32,
    pub records: Vec<FrameRecord>,
    pub repeated_index: i32,
}
#[derive(Serialize)]
pub struct AnimationRecords {
    #[serde(flatten)]
    pub header: TexaHeader,
    pub slot_table: Vec<i16>,
    pub frame_group_offsets: Vec<u32>,
    pub frame_groups: Vec<FrameGroup>,
}
#[derive(Serialize)]
pub struct TextureAnimationRecords {
    pub profile: &'static str,
    pub fixed_frame_size: usize,
    pub animations: Vec<AnimationRecords>,
    pub runtime_verified: bool,
}
#[derive(Default)]
struct Budget {
    slots: usize,
    groups: usize,
    frames: usize,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn range(body: &[u8], offset: usize, length: usize) -> io::Result<&[u8]> {
    body.get(
        offset
            ..offset
                .checked_add(length)
                .ok_or_else(|| invalid("TEXA record range overflow"))?,
    )
    .ok_or_else(|| invalid("TEXA record exceeds its footer body"))
}
fn table(body: &[u8], offset: i32, count: usize, stride: usize) -> io::Result<&[u8]> {
    range(
        body,
        usize::try_from(offset).map_err(|_| invalid("negative TEXA table pointer"))?,
        count
            .checked_mul(stride)
            .ok_or_else(|| invalid("TEXA table length overflow"))?,
    )
}
fn read_body(body: &[u8], header: TexaHeader, budget: &mut Budget) -> io::Result<AnimationRecords> {
    let slots = header.header_u16[5]
        .checked_sub(header.header_u16[4])
        .map(|n| n as usize + 1)
        .ok_or_else(|| invalid("reversed TEXA slot range"))?;
    budget.slots = budget
        .slots
        .checked_add(slots)
        .filter(|&n| n <= 262_144)
        .ok_or_else(|| invalid("cumulative TEXA slot budget exceeded"))?;
    let groups = header.header_u16[6] as usize;
    budget.groups = budget
        .groups
        .checked_add(groups)
        .filter(|&n| n <= 4096)
        .ok_or_else(|| invalid("cumulative TEXA group budget exceeded"))?;
    let slot_table = table(body, header.header_i32[0], slots, 2)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| i16::from_le_bytes(*v))
        .collect();
    let frame_group_offsets: Vec<_> = table(body, header.header_i32[1], groups, 4)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_le_bytes(*v))
        .collect();
    let mut frame_groups = Vec::with_capacity(groups);
    for &first_position in &frame_group_offsets {
        let mut visited = HashSet::new();
        let mut records = Vec::new();
        let mut index = 0i32;
        while visited.insert(index) {
            if records.len() >= 16_384 {
                return Err(invalid("TEXA frame group visit budget exceeded"));
            }
            budget.frames = budget
                .frames
                .checked_add(1)
                .filter(|&n| n <= 65_536)
                .ok_or_else(|| invalid("cumulative TEXA frame budget exceeded"))?;
            let position = i64::from(first_position) + i64::from(index) * FRAME_BYTES as i64;
            let body_offset = usize::try_from(position)
                .map_err(|_| invalid("TEXA relative frame points before body"))?;
            let bytes = range(body, body_offset, FRAME_BYTES)?;
            let words: [u16; 4] = std::array::from_fn(|i| {
                u16::from_le_bytes(bytes[i * 2..i * 2 + 2].try_into().unwrap())
            });
            let control = words[0] & 15;
            let frame_index_delta = i32::from((words[0] as i16) >> 4);
            let next_index = index
                .checked_add(if control == 2 || control == 3 {
                    frame_index_delta
                } else {
                    1
                })
                .ok_or_else(|| invalid("TEXA logical frame index overflow"))?;
            records.push(FrameRecord {
                index,
                body_offset,
                words,
                control,
                frame_index_delta,
                next_index,
            });
            index = next_index;
        }
        frame_groups.push(FrameGroup {
            first_position,
            records,
            repeated_index: index,
        });
    }
    Ok(AnimationRecords {
        header,
        slot_table,
        frame_group_offsets,
        frame_groups,
    })
}
/// Read only bounded recognized TEXA chunks, retaining every visited raw record.
/// Aliased groups/negative logical indices and unknown controls remain visible.
pub fn read_records(bytes: &[u8]) -> io::Result<TextureAnimationRecords> {
    let layout = model_texture::read_layout(bytes)?;
    // Reuse already qualified header/environment/sprite admission. Asset offset
    // zero is sufficient here: no base/replacement link is exposed or selected.
    let headers = crate::model_texture_routing::read_links(bytes, 0, &[])?.texa_headers;
    let mut budget = Budget::default();
    let mut animations = Vec::with_capacity(headers.len());
    for header in headers {
        let chunk = &layout.footer[header.footer_ordinal];
        let body = range(bytes, chunk.offset + 8, chunk.length)?;
        animations.push(read_body(body, header, &mut budget)?);
    }
    Ok(TextureAnimationRecords {
        profile: PROFILE,
        fixed_frame_size: FRAME_BYTES,
        animations,
        runtime_verified: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<u8>, TexaHeader) {
        let mut body = vec![0; 128];
        body[40..44].copy_from_slice(&[255, 255, 1, 128]);
        body[44..48].copy_from_slice(&56u32.to_le_bytes());
        body[48..52].copy_from_slice(&80u32.to_le_bytes());
        frame(&mut body, 56, [0, 1, 2, 0]);
        frame(&mut body, 64, [0xfff2, 0xffff, 3, 0xffff]);
        frame(&mut body, 80, [3, 7, 5, 2]);
        (
            body,
            TexaHeader {
                footer_ordinal: 0,
                header_u16: [3, 0, 0, 8, 3, 4, 2, 1, 0, 0, 1, 1],
                header_i32: [40, 44, 120, -1],
                sprite_data_offset: 120,
                sprite_data_length: 1,
                sprite_data_sha256: "authored".into(),
            },
        )
    }
    fn frame(body: &mut [u8], offset: usize, words: [u16; 4]) {
        for (i, word) in words.iter().enumerate() {
            body[offset + i * 2..offset + i * 2 + 2].copy_from_slice(&word.to_le_bytes());
        }
    }
    #[test]
    fn slots_raw_words_relative_jump_and_stop_repeat_are_retained() {
        let (body, header) = fixture();
        let result = read_body(&body, header, &mut Budget::default()).unwrap();
        assert_eq!(result.slot_table, [-1, -32767]);
        let g = &result.frame_groups[0];
        assert_eq!(g.records.len(), 2);
        assert_eq!(g.records[1].frame_index_delta, -1);
        assert_eq!(g.records[1].words, [0xfff2, 0xffff, 3, 0xffff]);
        assert_eq!(g.repeated_index, 0);
        assert_eq!(result.frame_groups[1].records.len(), 1);
        assert_eq!(result.frame_groups[1].records[0].next_index, 0);
    }
    #[test]
    fn negative_logical_indices_unknown_control_and_aliases_are_not_hidden() {
        let (mut body, header) = fixture();
        body[44..48].copy_from_slice(&80u32.to_le_bytes());
        frame(&mut body, 80, [0xffe2, 1, 1, 0]);
        frame(&mut body, 64, [9, 5, 3, 60000]);
        frame(&mut body, 72, [0x0013, 0, 0, 0]);
        let result = read_body(&body, header, &mut Budget::default()).unwrap();
        let g = &result.frame_groups[0];
        assert_eq!(
            g.records.iter().map(|r| r.index).collect::<Vec<_>>(),
            [0, -2, -1]
        );
        assert_eq!(g.records[1].control, 9);
        assert_eq!(g.records[1].words[3], 60000);
        assert_eq!(g.repeated_index, 0);
        assert_eq!(result.frame_groups[1].first_position, g.first_position);
    }
    #[test]
    fn malformed_tables_frames_and_cumulative_budgets_reject() {
        let (body, mut header) = fixture();
        header.header_i32[0] = -1;
        assert!(read_body(&body, header, &mut Budget::default()).is_err());
        let (body, mut header) = fixture();
        header.header_u16[5] = 2;
        assert!(read_body(&body, header, &mut Budget::default()).is_err());
        let (mut body, header) = fixture();
        body[44..48].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_body(&body, header, &mut Budget::default()).is_err());
        let (mut body, header) = fixture();
        frame(&mut body, 56, [0x8003, 0, 0, 0]);
        assert!(read_body(&body, header, &mut Budget::default()).is_err());
        for (slots, groups, frames) in [(262_144, 0, 0), (0, 4096, 0), (0, 0, 65_536)] {
            let (body, header) = fixture();
            assert!(
                read_body(
                    &body,
                    header,
                    &mut Budget {
                        slots,
                        groups,
                        frames
                    }
                )
                .is_err()
            );
        }
    }
    #[test]
    fn sequential_unterminated_graph_has_a_finite_visit_limit() {
        let (mut body, mut header) = fixture();
        header.header_u16[6] = 1;
        body.resize(56 + 16_385 * FRAME_BYTES, 0);
        body[56..].fill(0);
        assert_eq!(
            read_body(&body, header, &mut Budget::default())
                .err()
                .unwrap()
                .to_string(),
            "TEXA frame group visit budget exceeded"
        );
    }
}
