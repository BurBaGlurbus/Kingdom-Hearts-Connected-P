//! Bounded structural observations. These assign no glyph/shader semantics.
use serde::Serialize;
use std::io;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ChannelStatistics {
    pub pixels: u64,
    pub histograms_rgba: [Vec<u64>; 4],
    pub bit_one_counts_rgba: [[u64; 8]; 4],
    /// R=G, R=B, G=B, respectively.
    pub equal_rgb_pairs: [u64; 3],
    pub zero_alpha_pixels: u64,
    pub zero_alpha_nonzero_rgb_pixels: u64,
}

pub fn summarize_rgba(bytes: &[u8]) -> io::Result<ChannelStatistics> {
    if bytes.is_empty() || bytes.len() > 64 * 1024 * 1024 || !bytes.len().is_multiple_of(4) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "RGBA observation bounds",
        ));
    }
    let mut result = ChannelStatistics {
        pixels: (bytes.len() / 4) as u64,
        histograms_rgba: std::array::from_fn(|_| vec![0; 256]),
        bit_one_counts_rgba: [[0; 8]; 4],
        equal_rgb_pairs: [0; 3],
        zero_alpha_pixels: 0,
        zero_alpha_nonzero_rgb_pixels: 0,
    };
    for pixel in bytes.as_chunks::<4>().0 {
        for (channel, value) in pixel.iter().enumerate() {
            result.histograms_rgba[channel][*value as usize] += 1;
            for bit in 0..8 {
                result.bit_one_counts_rgba[channel][bit] += u64::from(value & (1 << bit) != 0);
            }
        }
        for (pair, (a, b)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
            result.equal_rgb_pairs[pair] += u64::from(pixel[a] == pixel[b]);
        }
        if pixel[3] == 0 {
            result.zero_alpha_pixels += 1;
            result.zero_alpha_nonzero_rgb_pixels += u64::from(pixel[..3] != [0; 3]);
        }
    }
    Ok(result)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ImgdHeader {
    pub version: i32,
    pub bitmap_offset: u32,
    pub bitmap_length: u32,
    pub clut_offset: u32,
    pub clut_length: u32,
    pub width: u16,
    pub height: u16,
    pub gs_psm: u16,
    pub flags: i32,
    pub swizzled_flag: bool,
    pub bytes_after_declared_regions: usize,
}

/// Header/ranges only, based on pinned OpenKH Imgd.Header. No format-specific
/// pixel size, CLUT interpretation, swizzle or alpha expansion is asserted.
pub fn read_imgd_header(bytes: &[u8]) -> io::Result<ImgdHeader> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    if bytes.len() < 64 || bytes.len() > 32 * 1024 * 1024 || !bytes.starts_with(b"IMGD") {
        return Err(invalid("IMGD signature/header/bounds"));
    }
    let i32_at = |offset| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let i16_at = |offset| i16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let region = |offset, length| -> io::Result<(u32, u32, usize)> {
        let (start, size) = (i32_at(offset), i32_at(length));
        if start < 0 || size < 0 || (size != 0 && start < 64) {
            return Err(invalid("IMGD negative/header-overlapping region"));
        }
        let end = start as u64 + size as u64;
        if end > bytes.len() as u64 {
            return Err(invalid("IMGD region exceeds container"));
        }
        Ok((start as u32, size as u32, end as usize))
    };
    let (bitmap_offset, bitmap_length, bitmap_end) = region(8, 12)?;
    let (clut_offset, clut_length, clut_end) = region(16, 20)?;
    let (width, height) = (i16_at(28), i16_at(30));
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) || bitmap_length == 0 {
        return Err(invalid(
            "IMGD dimensions/empty bitmap outside observation bounds",
        ));
    }
    if clut_length != 0 && bitmap_offset < clut_end as u32 && clut_offset < bitmap_end as u32 {
        return Err(invalid("IMGD bitmap/CLUT regions overlap"));
    }
    let flags = i32_at(60);
    Ok(ImgdHeader {
        version: i32_at(4),
        bitmap_offset,
        bitmap_length,
        clut_offset,
        clut_length,
        width: width as u16,
        height: height as u16,
        gs_psm: u16::from_le_bytes(bytes[38..40].try_into().unwrap()),
        flags,
        swizzled_flag: flags & 4 != 0,
        bytes_after_declared_regions: bytes.len() - bitmap_end.max(clut_end).max(64),
    })
}

pub const IMGD_INDEXED8_LINEAR_PROFILE: &str = "imgd_indexed8_linear_ps2_palette_v1";
pub struct DecodedIndexed8Imgd {
    pub header: ImgdHeader,
    pub indices: Vec<u8>,
    pub clut_rgba: Vec<u8>,
    pub pixels_rgba: Vec<u8>,
}

/// Explicit finite CPU profile. Other IMGD routes remain structural evidence;
/// unsupported formats/flags never fall back to a guessed interpretation.
pub fn decode_imgd_indexed8_linear(bytes: &[u8]) -> io::Result<DecodedIndexed8Imgd> {
    let header = read_imgd_header(bytes)?;
    let count = header.width as usize * header.height as usize;
    if header.version != 256
        || header.gs_psm != 19
        || header.flags & !3 != 0
        || header.bitmap_length as usize != count
        || header.clut_length != 1024
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IMGD is outside the explicit indexed8 linear CPU profile",
        ));
    }
    let indices =
        bytes[header.bitmap_offset as usize..header.bitmap_offset as usize + count].to_vec();
    let stored_clut = &bytes[header.clut_offset as usize..header.clut_offset as usize + 1024];
    let mut clut_rgba = Vec::with_capacity(1024);
    for index in 0..256usize {
        // OpenKH Ps2.Repl: interchange palette index bits 3 and 4.
        let source = (index & 0xe7) | ((index & 0x10) >> 1) | ((index & 0x08) << 1);
        clut_rgba.extend_from_slice(&stored_clut[source * 4..source * 4 + 3]);
        clut_rgba.push(stored_clut[source * 4 + 3].saturating_mul(2));
    }
    let mut pixels_rgba = Vec::with_capacity(count * 4);
    for index in &indices {
        let offset = *index as usize * 4;
        pixels_rgba.extend_from_slice(&clut_rgba[offset..offset + 4]);
    }
    Ok(DecodedIndexed8Imgd {
        header,
        indices,
        clut_rgba,
        pixels_rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channels_preserve_bits_and_hidden_rgb_without_semantic_guessing() {
        let result = summarize_rgba(&[1, 2, 4, 0, 255, 255, 255, 128]).unwrap();
        assert_eq!(result.pixels, 2);
        assert_eq!(result.histograms_rgba[0][1], 1);
        assert_eq!(result.bit_one_counts_rgba[0], [2, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(result.bit_one_counts_rgba[3], [0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(result.equal_rgb_pairs, [1; 3]);
        assert_eq!(result.zero_alpha_nonzero_rgb_pixels, 1);
        assert!(summarize_rgba(&[1, 2, 3]).is_err());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 72];
        bytes[..4].copy_from_slice(b"IMGD");
        for (offset, value) in [(4, 256i32), (8, 64), (12, 4), (16, 68), (20, 4), (60, 4)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[28..30].copy_from_slice(&2i16.to_le_bytes());
        bytes[30..32].copy_from_slice(&2i16.to_le_bytes());
        bytes[38..40].copy_from_slice(&19u16.to_le_bytes());
        bytes
    }
    #[test]
    fn imgd_header_keeps_format_separate_from_bounded_ranges() {
        let mut bytes = fixture();
        bytes.extend([7, 8]);
        let result = read_imgd_header(&bytes).unwrap();
        assert_eq!(result.gs_psm, 19);
        assert!(result.swizzled_flag);
        assert_eq!(result.bytes_after_declared_regions, 2);
        bytes[38..40].copy_from_slice(&65535u16.to_le_bytes());
        assert_eq!(read_imgd_header(&bytes).unwrap().gs_psm, 65535);
    }
    #[test]
    fn rejects_truncation_signed_ranges_header_overlap_and_region_overlap() {
        for length in 0..72 {
            assert!(read_imgd_header(&fixture()[..length]).is_err());
        }
        for (offset, value) in [(8, -1i32), (8, 63), (12, i32::MAX), (16, 66), (20, -1)] {
            let mut bytes = fixture();
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(read_imgd_header(&bytes).is_err());
        }
        let mut bytes = fixture();
        bytes[28..30].copy_from_slice(&(-1i16).to_le_bytes());
        assert!(read_imgd_header(&bytes).is_err());
    }

    fn indexed_fixture() -> Vec<u8> {
        let mut bytes = vec![0; 64 + 4 + 1024];
        bytes[..64].copy_from_slice(&fixture()[..64]);
        bytes[20..24].copy_from_slice(&1024i32.to_le_bytes());
        bytes[60..64].copy_from_slice(&3i32.to_le_bytes());
        bytes[64..68].copy_from_slice(&[8, 16, 0, 255]);
        for index in 0..256usize {
            bytes[68 + index * 4..72 + index * 4].copy_from_slice(&[
                index as u8,
                7,
                11,
                index as u8,
            ]);
        }
        bytes
    }
    #[test]
    fn linear_imgd_qualifies_palette_swap_alpha_saturation_and_hidden_rgb() {
        let decoded = decode_imgd_indexed8_linear(&indexed_fixture()).unwrap();
        assert_eq!(decoded.indices, [8, 16, 0, 255]);
        assert_eq!(
            decoded.pixels_rgba,
            [16, 7, 11, 32, 8, 7, 11, 16, 0, 7, 11, 0, 255, 7, 11, 255]
        );
        assert_eq!(&decoded.clut_rgba[127 * 4..128 * 4], &[127, 7, 11, 254]);
        assert_eq!(&decoded.clut_rgba[128 * 4..129 * 4], &[128, 7, 11, 255]);
    }
    #[test]
    fn linear_imgd_rejects_unqualified_formats_flags_and_incomplete_planes() {
        for (offset, value) in [(4, 257i32), (12, 3), (20, 1020), (60, 7), (60, 8)] {
            let mut bytes = indexed_fixture();
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(decode_imgd_indexed8_linear(&bytes).is_err());
        }
        let mut bytes = indexed_fixture();
        bytes[38..40].copy_from_slice(&20u16.to_le_bytes());
        assert!(decode_imgd_indexed8_linear(&bytes).is_err());
        bytes.truncate(100);
        assert!(decode_imgd_indexed8_linear(&bytes).is_err());
    }
}
