//! Explicit candidate geometry following OpenKH Constants/Kh2MessageRenderer at
//! 7a3b945c538d32c6a285128c98aefba093f52ceb (Apache-2.0).
//! This is not language selection, Unicode inversion or installed rendering.
use crate::{font::DecodedFont, font::FontSlot, message::SystemMessageToken};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CellProfile {
    InternationalSystem,
    InternationalEvent,
    JapaneseSystem,
    JapaneseEvent,
    Icon,
}

impl CellProfile {
    pub fn slot(self) -> FontSlot {
        match self {
            Self::InternationalSystem | Self::JapaneseSystem => FontSlot::System,
            Self::InternationalEvent | Self::JapaneseEvent => FontSlot::Event,
            Self::Icon => FontSlot::Icon,
        }
    }

    fn geometry(self) -> (usize, usize, usize) {
        match self {
            Self::InternationalSystem => (18, 24, 256),
            Self::InternationalEvent => (24, 32, 512),
            Self::JapaneseSystem => (18, 18, 256),
            Self::JapaneseEvent => (24, 24, 512),
            Self::Icon => (24, 24, 160),
        }
    }
}

/// Candidate only: public PrintText/PrintComplex drawing re-encodes its commands.
/// Unknown source slots may have coordinates without being known/drawn glyphs.
pub fn raw_text_candidate_index(token: &SystemMessageToken<'_>) -> Option<usize> {
    match (token.opcode, token.parameters) {
        (0x20..=0xff, []) => Some(token.opcode as usize - 0x20),
        (0x19..=0x1f, [parameter]) => {
            Some(*parameter as usize + (token.opcode as usize - 0x19) * 256 + 224)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct FontCell {
    pub index: usize,
    pub palette_lane: usize,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    /// An original table byte, not a measured advance or a default font width.
    pub spacing_byte: u8,
}

pub fn project_font_cell(
    profile: CellProfile,
    atlas_width: usize,
    atlas_height: usize,
    spacing: &[u8],
    index: usize,
) -> io::Result<FontCell> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    let (width, height, table_height) = profile.geometry();
    let icon = profile == CellProfile::Icon;
    if (icon && (atlas_width != 256 || atlas_height != 160))
        || (!icon
            && (atlas_width != 512 || ![table_height, 2 * table_height].contains(&atlas_height)))
    {
        return Err(invalid(
            "atlas dimensions do not fit the explicit cell profile",
        ));
    }
    let spacing_byte = *spacing
        .get(index)
        .ok_or_else(|| invalid("glyph candidate has no source spacing byte"))?;
    let columns = atlas_width / width;
    let rows = table_height / height;
    // The reference masks two bank bits. Reject further banks rather than aliasing.
    let bank = index / (columns * rows);
    if (icon && bank != 0) || (!icon && bank >= 4) {
        return Err(invalid("glyph candidate exceeds non-aliased profile banks"));
    }
    let x = index % columns * width;
    let y = (index / columns % rows) * height + (bank / 2) * table_height;
    if x + width > atlas_width || y + height > atlas_height {
        return Err(invalid("glyph candidate exceeds complete atlas cells"));
    }
    Ok(FontCell {
        index,
        palette_lane: if icon { 0 } else { bank % 2 },
        x,
        y,
        width,
        height,
        spacing_byte,
    })
}

impl FontCell {
    /// Row-major straight RGBA. The caller must select the recorded palette lane.
    pub fn crop_rgba(self, image: &DecodedFont) -> io::Result<Vec<u8>> {
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid font cell/image span");
        let end_x = self.x.checked_add(self.width).ok_or_else(invalid)?;
        let end_y = self.y.checked_add(self.height).ok_or_else(invalid)?;
        let image_bytes = image
            .width
            .checked_mul(image.height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(invalid)?;
        if self.width == 0
            || self.height == 0
            || end_x > image.width
            || end_y > image.height
            || image.pixels_rgba.len() != image_bytes
        {
            return Err(invalid());
        }
        let mut pixels = Vec::with_capacity(self.width * self.height * 4);
        for y in self.y..end_y {
            let start = (y * image.width + self.x) * 4;
            pixels.extend_from_slice(&image.pixels_rgba[start..start + self.width * 4]);
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{font::PaletteLane, font::decode_font, message::system_message_tokens};

    #[test]
    fn preserves_raw_codes_instead_of_inverting_unicode_or_controls() {
        let tokens =
            system_message_tokens(&[0x20, 0x72, 0xff, 0x19, 0, 0x1f, 255, 1, 2, 0]).unwrap();
        assert_eq!(
            tokens
                .iter()
                .map(raw_text_candidate_index)
                .collect::<Vec<_>>(),
            [
                Some(0),
                Some(82),
                Some(223),
                Some(224),
                Some(2015),
                None,
                None,
                None
            ]
        );
        assert_eq!(
            raw_text_candidate_index(&SystemMessageToken {
                source_offset: 0,
                opcode: 0x19,
                parameters: &[],
            }),
            None
        );
    }

    #[test]
    fn japanese_banks_switch_lanes_and_preserve_vertical_padding() {
        let spacing = vec![7; 1765];
        for (profile, atlas_height, bank_size, cell_height, bottom) in [
            (CellProfile::JapaneseSystem, 512, 392, 18, 234),
            (CellProfile::JapaneseEvent, 1024, 441, 24, 480),
        ] {
            let last =
                project_font_cell(profile, 512, atlas_height, &spacing, bank_size - 1).unwrap();
            assert_eq!(
                (last.palette_lane, last.y, last.height),
                (0, bottom, cell_height)
            );
            let second =
                project_font_cell(profile, 512, atlas_height, &spacing, bank_size).unwrap();
            assert_eq!((second.palette_lane, second.x, second.y), (1, 0, 0));
            let third =
                project_font_cell(profile, 512, atlas_height, &spacing, 2 * bank_size).unwrap();
            assert_eq!(
                (third.palette_lane, third.x, third.y),
                (0, 0, atlas_height / 2)
            );
            let fourth =
                project_font_cell(profile, 512, atlas_height, &spacing, 3 * bank_size).unwrap();
            assert_eq!((fourth.palette_lane, fourth.y), (1, atlas_height / 2));
            assert!(
                project_font_cell(profile, 512, atlas_height, &spacing, 4 * bank_size).is_err()
            );
        }
    }

    #[test]
    fn rejects_missing_spacing_partial_icons_wrong_dimensions_and_absent_banks() {
        let spacing = vec![255; 2000];
        let system = CellProfile::InternationalSystem;
        let event = CellProfile::InternationalEvent;
        assert_eq!(
            project_font_cell(system, 512, 256, &spacing, 279)
                .unwrap()
                .y,
            216
        );
        assert_eq!(
            project_font_cell(event, 512, 512, &spacing, 335).unwrap().y,
            480
        );
        assert!(project_font_cell(system, 512, 256, &spacing, 560).is_err());
        assert!(project_font_cell(system, 512, 256, &[], 0).is_err());
        assert!(project_font_cell(system, 511, 256, &spacing, 0).is_err());
        assert!(project_font_cell(event, 512, 256, &spacing, 0).is_err());
        let icon = project_font_cell(CellProfile::Icon, 256, 160, &spacing, 59).unwrap();
        assert_eq!((icon.x, icon.y, icon.spacing_byte), (216, 120, 255));
        assert!(project_font_cell(CellProfile::Icon, 256, 160, &spacing, 60).is_err());
        assert!(project_font_cell(system, 512, 512, &spacing, usize::MAX).is_err());
    }

    #[test]
    fn cell_crop_preserves_rows_and_rejects_malformed_image_or_span() {
        let mut image = decode_font(&vec![0; 65536], FontSlot::System, PaletteLane::First).unwrap();
        for y in 0..image.height {
            for x in 0..image.width {
                image.pixels_rgba[(y * image.width + x) * 4..][..4]
                    .copy_from_slice(&[x as u8, y as u8, 3, 4]);
            }
        }
        let mut cell =
            project_font_cell(CellProfile::InternationalSystem, 512, 256, &[5; 30], 29).unwrap();
        let pixels = cell.crop_rgba(&image).unwrap();
        assert_eq!(pixels.len(), 18 * 24 * 4);
        assert_eq!(&pixels[..4], &[18, 24, 3, 4]);
        assert_eq!(&pixels[pixels.len() - 4..], &[35, 47, 3, 4]);
        image.pixels_rgba.pop();
        assert!(cell.crop_rgba(&image).is_err());
        cell.x = usize::MAX;
        assert!(cell.crop_rgba(&image).is_err());
    }
}
