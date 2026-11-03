//! Localized font RawBitmap planes, following pinned OpenKH RawBitmap/FontContext.
//! Slot selection belongs to the source BAR record, not to a guessed filename.
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSlot {
    System,
    Event,
    Icon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteLane {
    First,
    Second,
}

pub struct DecodedFont {
    pub width: usize,
    pub height: usize,
    pub bits_per_pixel: u8,
    pub embedded_palette: bool,
    /// Indexed4 uses high nibble first, as in OpenKH IImageRead.
    pub packed_pixels: Vec<u8>,
    pub clut_rgba: Vec<u8>,
    /// Straight RGBA; alpha is expanded from PS2's range with saturation.
    pub pixels_rgba: Vec<u8>,
}

pub fn decode_font(bytes: &[u8], slot: FontSlot, lane: PaletteLane) -> io::Result<DecodedFont> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    let (width, height, bits_per_pixel) = match slot {
        FontSlot::Icon => {
            if lane != PaletteLane::First {
                return Err(invalid("icon font has only one palette lane"));
            }
            (256, 160, 8)
        }
        FontSlot::System | FontSlot::Event => {
            // This is the pinned loader's size deduction, not an edition detector.
            let height = match bytes.len() {
                0..131_072 => 256,
                131_072..262_144 => 512,
                262_144..524_288 => 1024,
                _ => return Err(invalid("unsupported localized font plane size")),
            };
            (512, height, 4)
        }
    };
    let plane_size = width * height * bits_per_pixel as usize / 8;
    let embedded_palette = bytes.len() == plane_size + 1024;
    if bytes.len() != plane_size && !embedded_palette {
        return Err(invalid(
            "truncated font plane/palette or unsupported trailing bytes",
        ));
    }
    if bits_per_pixel == 8 && !embedded_palette {
        return Err(invalid("icon font requires its complete 256-color palette"));
    }
    let mut packed_pixels = bytes[..plane_size].to_vec();
    if bits_per_pixel == 4 {
        for byte in &mut packed_pixels {
            *byte = byte.rotate_left(4);
        }
    }
    let color_count = if bits_per_pixel == 8 { 256 } else { 16 };
    let mut clut_rgba = Vec::with_capacity(color_count * 4);
    for index in 0..color_count {
        let mut color = if embedded_palette {
            let source_index = if bits_per_pixel == 8 {
                // Ps2.Repl interchanges CLUT index bits 3 and 4.
                (index & 0xe7) | ((index & 0x10) >> 1) | ((index & 0x08) << 1)
            } else {
                index
            };
            bytes[plane_size + source_index * 4..plane_size + source_index * 4 + 4]
                .try_into()
                .unwrap()
        } else {
            let colors: [u32; 4] = [0x00000000, 0x80a0a0a0, 0xe4cccccc, 0xfff0f0f0];
            let color_index = match lane {
                PaletteLane::First => index & 3,
                PaletteLane::Second => index / 4,
            };
            let mut color = colors[color_index].to_le_bytes();
            color[3] = (color[3] as u16).div_ceil(2) as u8;
            color
        };
        color[3] = (color[3] as u16 * 2).min(255) as u8;
        clut_rgba.extend(color);
    }
    let mut pixels_rgba = Vec::with_capacity(width * height * 4);
    for pixel in 0..width * height {
        let index = if bits_per_pixel == 8 {
            packed_pixels[pixel] as usize
        } else {
            let shift = if pixel % 2 == 0 { 4 } else { 0 };
            ((packed_pixels[pixel / 2] >> shift) & 15) as usize
        };
        pixels_rgba.extend_from_slice(&clut_rgba[index * 4..index * 4 + 4]);
    }
    Ok(DecodedFont {
        width,
        height,
        bits_per_pixel,
        embedded_palette,
        packed_pixels,
        clut_rgba,
        pixels_rgba,
    })
}

impl DecodedFont {
    /// Private visual inspection only: 24-bit BMP composited onto a checkerboard.
    /// The authoritative decoded RGBA bytes are fingerprinted separately.
    pub fn write_preview(&self, writer: &mut impl Write) -> io::Result<()> {
        write_rgba_preview(writer, self.width, self.height, &self.pixels_rgba)
    }
}

/// Private 24-bit inspection BMP, composited on a checkerboard. Input pixels
/// remain separate and authoritative; this does not implement game rendering.
pub fn write_rgba_preview(
    writer: &mut impl Write,
    width: usize,
    height: usize,
    pixels: &[u8],
) -> io::Result<()> {
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || pixels.len() != width * height * 4
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid preview dimensions/pixels",
        ));
    }
    let stride = (width * 3).next_multiple_of(4);
    let data_size = stride * height;
    writer.write_all(b"BM")?;
    writer.write_all(&((54 + data_size) as u32).to_le_bytes())?;
    writer.write_all(&[0; 4])?;
    writer.write_all(&54u32.to_le_bytes())?;
    writer.write_all(&40u32.to_le_bytes())?;
    writer.write_all(&(width as i32).to_le_bytes())?;
    writer.write_all(&(-(height as i32)).to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&24u16.to_le_bytes())?;
    writer.write_all(&0u32.to_le_bytes())?;
    writer.write_all(&(data_size as u32).to_le_bytes())?;
    writer.write_all(&[0; 16])?;
    let mut row = vec![0; stride];
    for y in 0..height {
        for x in 0..width {
            let rgba = &pixels[(y * width + x) * 4..][..4];
            let background = if (x / 8 + y / 8) % 2 == 0 { 24 } else { 40 };
            for (dst, src) in [2, 1, 0].into_iter().enumerate() {
                let alpha = rgba[3] as u32;
                row[x * 3 + dst] =
                    ((rgba[src] as u32 * alpha + background * (255 - alpha) + 127) / 255) as u8;
            }
        }
        writer.write_all(&row)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_packed_palette_lanes_preserve_nibble_order_and_alpha() {
        let mut bytes = vec![0; 65536];
        bytes[..2].copy_from_slice(&[0x21, 0xc3]);
        let first = decode_font(&bytes, FontSlot::System, PaletteLane::First).unwrap();
        let second = decode_font(&bytes, FontSlot::System, PaletteLane::Second).unwrap();
        assert_eq!(
            (first.width, first.height, first.bits_per_pixel),
            (512, 256, 4)
        );
        assert_eq!(&first.packed_pixels[..2], &[0x12, 0x3c]);
        assert_eq!(
            &first.pixels_rgba[..16],
            &[
                160, 160, 160, 128, 204, 204, 204, 228, 240, 240, 240, 255, 0, 0, 0, 0
            ]
        );
        assert_eq!(
            &second.pixels_rgba[..16],
            &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 240, 240, 240, 255]
        );
        for (size, height) in [(131072, 512), (262144, 1024)] {
            assert_eq!(
                decode_font(&vec![0; size], FontSlot::Event, PaletteLane::First)
                    .unwrap()
                    .height,
                height
            );
        }
    }

    #[test]
    fn embedded_palettes_swizzle_only_eight_bit_indices_and_saturate_alpha() {
        let mut icon = vec![0; 40960 + 1024];
        icon[..3].copy_from_slice(&[8, 16, 255]);
        for i in 0..256 {
            icon[40960 + 4 * i..40964 + 4 * i].copy_from_slice(&[i as u8, 1, 2, i as u8]);
        }
        let result = decode_font(&icon, FontSlot::Icon, PaletteLane::First).unwrap();
        assert_eq!(
            &result.pixels_rgba[..12],
            &[16, 1, 2, 32, 8, 1, 2, 16, 255, 1, 2, 255]
        );
        let mut four_bit = vec![0; 65536 + 1024];
        four_bit[0] = 0x98;
        four_bit[65536 + 32..65536 + 40].copy_from_slice(&[8, 4, 2, 127, 9, 5, 3, 128]);
        let first = decode_font(&four_bit, FontSlot::Event, PaletteLane::First).unwrap();
        let second = decode_font(&four_bit, FontSlot::Event, PaletteLane::Second).unwrap();
        assert_eq!(&first.pixels_rgba[..8], &[8, 4, 2, 254, 9, 5, 3, 255]);
        assert_eq!(first.pixels_rgba, second.pixels_rgba);
    }

    #[test]
    fn rejects_ambiguous_lengths_and_writes_top_down_private_preview() {
        for size in [0, 65535, 65537, 65536 + 64, 131071, 524288] {
            assert!(decode_font(&vec![0; size], FontSlot::System, PaletteLane::First).is_err());
        }
        assert!(decode_font(&vec![0; 40960], FontSlot::Icon, PaletteLane::First).is_err());
        assert!(decode_font(&vec![0; 41984], FontSlot::Icon, PaletteLane::Second).is_err());
        let decoded = decode_font(&vec![0; 65536], FontSlot::System, PaletteLane::First).unwrap();
        let mut bmp = Vec::new();
        decoded.write_preview(&mut bmp).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(bmp.len(), 54 + 512 * 256 * 3);
        assert_eq!(i32::from_le_bytes(bmp[22..26].try_into().unwrap()), -256);
        assert_eq!(&bmp[54..57], &[24, 24, 24]);
    }
}
