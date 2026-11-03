//! Source-bound image research. Projection modes inspect stored channels;
//! they do not infer font banks, filtering, glyph meaning or gameplay rules.
use crate::{
    hd_image::{DecodedHdImage, HdPixelDecodeProfile, decode_hd_image_with_profile},
    probe_source::ProbeSource,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::File,
    io::{self, Read},
    path::Path,
};

#[derive(Deserialize)]
struct SelectionFile {
    schema_version: u32,
    source_content_identity: String,
    selections: Vec<Selection>,
    inspect_hd_images: bool,
    #[serde(default)]
    dds_pixel_decode_profile: HdPixelDecodeProfile,
}
#[derive(Deserialize)]
struct Selection {
    index: String,
    ordinal: usize,
    name_hash: [u8; 16],
}

pub struct InspectionImage {
    pub index: String,
    pub ordinal: usize,
    pub replacement_ordinal: u32,
    pub decoded: DecodedHdImage,
}

/// Fresh full validation precedes any image decode; bounded data stays in
/// memory. This accepts only the quarantined research candidate binding.
pub fn read_selected_images(
    source: &ProbeSource,
    path: &Path,
) -> Result<Vec<InspectionImage>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err("image selection exceeds 1 MiB".into());
    }
    let selection: SelectionFile = serde_json::from_slice(&bytes)?;
    if selection.schema_version != 1
        || selection.source_content_identity != source.content_identity()
        || !selection.inspect_hd_images
        || selection.selections.is_empty()
        || selection.selections.len() > 64
    {
        return Err("image selection identity/schema/count/opt-in is invalid".into());
    }
    let mut seen = HashSet::new();
    for item in &selection.selections {
        if !validation::required_files()
            .iter()
            .any(|(name, _)| name == &item.index && name.ends_with(".hed"))
            || !seen.insert((&item.index, item.ordinal))
        {
            return Err("image selection contains invalid/duplicate source identities".into());
        }
    }
    source.verify_full_source()?;
    let mut images = Vec::new();
    let mut storage_bytes = 0usize;
    let mut rgba_bytes = 0usize;
    for (relative, _) in validation::required_files()
        .into_iter()
        .filter(|(name, _)| name.ends_with(".hed"))
    {
        let selected: Vec<_> = selection
            .selections
            .iter()
            .filter(|item| item.index == relative)
            .collect();
        if selected.is_empty() {
            continue;
        }
        let mut checked = source.open_index(&relative)?;
        for item in selected {
            let entry = checked
                .inventory
                .entries
                .get(item.ordinal)
                .filter(|entry| entry.name_hash == item.name_hash)
                .ok_or("image entry identity differs from HED")?;
            let replacements =
                crate::replacement_payload::read_all_replacements(&mut checked.reader, entry)?;
            for replacement in replacements {
                storage_bytes += replacement.bytes.len();
                if storage_bytes > 128 * 1024 * 1024 {
                    return Err("image inspection exceeds storage-byte budget".into());
                }
                let decoded = decode_hd_image_with_profile(
                    &replacement.bytes,
                    replacement.metadata.declared_decompressed_length as usize,
                    selection.dds_pixel_decode_profile,
                )?;
                rgba_bytes += decoded.pixels_rgba.len();
                if rgba_bytes > 128 * 1024 * 1024 {
                    return Err("image inspection exceeds RGBA budget".into());
                }
                images.push(InspectionImage {
                    index: relative.clone(),
                    ordinal: item.ordinal,
                    replacement_ordinal: replacement.metadata.ordinal,
                    decoded,
                });
            }
        }
        checked.verify_unchanged()?;
    }
    if images.is_empty() {
        return Err("selected records contain no admitted HD images".into());
    }
    Ok(images)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    Composite,
    StoredRgb,
    Red,
    Green,
    Blue,
    Alpha,
}
impl Projection {
    pub const ALL: [Self; 6] = [
        Self::Composite,
        Self::StoredRgb,
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Alpha,
    ];
    pub fn pixels(self, image: &DecodedHdImage) -> io::Result<Vec<u8>> {
        let (width, height) = (image.width as usize, image.height as usize);
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || image.pixels_rgba.len() != width * height * 4
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "projection dimensions/pixels differ",
            ));
        }
        let mut pixels = Vec::with_capacity(image.pixels_rgba.len());
        for (i, rgba) in image.pixels_rgba.as_chunks::<4>().0.iter().enumerate() {
            let rgb = match self {
                Self::StoredRgb => [rgba[0], rgba[1], rgba[2]],
                Self::Composite => {
                    let background = if (i % width / 8 + i / width / 8) % 2 == 0 {
                        24
                    } else {
                        40
                    };
                    let alpha = rgba[3] as u32;
                    [0, 1, 2].map(|c| {
                        ((rgba[c] as u32 * alpha + background * (255 - alpha) + 127) / 255) as u8
                    })
                }
                _ => {
                    let channel = match self {
                        Self::Red => 0,
                        Self::Green => 1,
                        Self::Blue => 2,
                        Self::Alpha => 3,
                        _ => unreachable!(),
                    };
                    [rgba[channel]; 3]
                }
            };
            pixels.extend([rgb[0], rgb[1], rgb[2], 255]);
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_channels_survive_zero_alpha_and_composite_uses_owned_checkerboard() {
        let image = DecodedHdImage {
            format: crate::hd_image::HdImageFormat::Png,
            pixel_decode_profile: HdPixelDecodeProfile::default(),
            width: 2,
            height: 1,
            encoded_length: 0,
            storage_padding: 0,
            pixels_rgba: vec![10, 20, 30, 0, 200, 100, 50, 128],
        };
        assert_eq!(
            Projection::StoredRgb.pixels(&image).unwrap(),
            [10, 20, 30, 255, 200, 100, 50, 255]
        );
        assert_eq!(
            Projection::Alpha.pixels(&image).unwrap(),
            [0, 0, 0, 255, 128, 128, 128, 255]
        );
        assert_eq!(
            Projection::Red.pixels(&image).unwrap(),
            [10, 10, 10, 255, 200, 200, 200, 255]
        );
        assert_eq!(
            Projection::Composite.pixels(&image).unwrap(),
            [24, 24, 24, 255, 112, 62, 37, 255]
        );
    }
}
