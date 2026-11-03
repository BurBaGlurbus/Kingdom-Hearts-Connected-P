//! Bounded original-payload decoding. Package transform adapted from OpenKH
//! EgsEncryption.cs, commit 7a3b945c538d32c6a285128c98aefba093f52ceb.
//! Copyright 2019-2022 OpenKH Team and Contributors; Apache-2.0, see NOTICE.
//! Changes: Rust byte-wise transform, explicit range/output limits and zlib checks.
use crate::{AssetHeader, HedEntry, read_asset_header};
use flate2::{Decompress, FlushDecompress, Status};
use std::io::{self, Read, Seek, SeekFrom};

pub const MAX_ORIGINAL_BYTES: u32 = 32 * 1024 * 1024;

const MASTER: [u8; 256] = [
    0x7E, 0x88, 0x97, 0x55, 0x0B, 0x06, 0xF1, 0x08, 0xEB, 0xBB, 0x14, 0x1C, 0xD8, 0x7A, 0xEC, 0x41,
    0x34, 0xB2, 0xA3, 0x46, 0xEF, 0x6B, 0xFE, 0xE1, 0xCF, 0x53, 0xA5, 0x05, 0x12, 0xD2, 0x8E, 0x52,
    0x4A, 0x80, 0xE9, 0x81, 0xB0, 0xF0, 0xB4, 0x9C, 0xFF, 0x0F, 0x15, 0x13, 0xDA, 0x73, 0x4E, 0x77,
    0xBE, 0xD7, 0x30, 0xE5, 0xF6, 0x5A, 0x11, 0x37, 0x67, 0xBC, 0x83, 0x6F, 0x27, 0x76, 0xD0, 0xCD,
    0x69, 0x0D, 0x2E, 0x51, 0x42, 0x90, 0xB8, 0xB6, 0x4C, 0xAD, 0xCE, 0x5B, 0x1A, 0x1F, 0xF5, 0xAF,
    0x01, 0xF8, 0x5E, 0x3A, 0x6E, 0x68, 0x8B, 0xE8, 0x9F, 0xC9, 0xD9, 0x26, 0x92, 0x29, 0xC8, 0x33,
    0x98, 0x32, 0x54, 0xD4, 0x44, 0x25, 0x66, 0xAC, 0x5F, 0x99, 0x21, 0xE4, 0x8F, 0x1D, 0xC2, 0xD5,
    0xA4, 0x62, 0xF9, 0x02, 0x61, 0xDE, 0x59, 0xE7, 0x07, 0x9A, 0xFA, 0x2F, 0x95, 0x3F, 0x86, 0xD3,
    0x78, 0xA7, 0x75, 0xED, 0xD6, 0x2D, 0x64, 0x87, 0xBD, 0xC7, 0xC1, 0xAA, 0xF2, 0x8C, 0x17, 0xCB,
    0x31, 0x8A, 0xC3, 0xCC, 0x04, 0xEE, 0x6A, 0xAB, 0x5C, 0x22, 0x70, 0xCA, 0x9E, 0x71, 0x6D, 0x85,
    0x45, 0x5D, 0xB9, 0xA9, 0xA6, 0x10, 0x47, 0xFB, 0x82, 0x7D, 0x84, 0x7B, 0xC6, 0xE2, 0x38, 0xFC,
    0x2B, 0x0E, 0x20, 0x9D, 0xC5, 0xF3, 0x39, 0xA8, 0xA0, 0x65, 0x58, 0x43, 0x7C, 0xE3, 0x36, 0x18,
    0x72, 0x49, 0x79, 0xAE, 0xD1, 0x74, 0x40, 0xC4, 0x91, 0x4F, 0x24, 0x63, 0xBF, 0xBA, 0x23, 0x96,
    0x50, 0xB3, 0x57, 0xDF, 0x1E, 0x03, 0x48, 0x7F, 0x35, 0x4D, 0x3E, 0xE6, 0xA1, 0xDD, 0x09, 0x3C,
    0x3D, 0x3B, 0x56, 0x8D, 0x93, 0x2A, 0x9B, 0x4B, 0x0C, 0x28, 0xB1, 0xE0, 0x60, 0x89, 0x19, 0xDB,
    0x2C, 0xF7, 0x6C, 0xB5, 0x1B, 0x94, 0xC0, 0xDC, 0xEA, 0xB7, 0x0A, 0xF4, 0x16, 0xFD, 0xA2, 0x00,
];
const SCRAMBLE: [u8; 10] = [1, 2, 4, 8, 16, 32, 64, 128, 27, 54];

/// The upstream format transform is symmetric; it touches at most 256 bytes.
pub fn transform_prefix(seed: &[u8; 16], data: &mut [u8]) {
    let mut key = [0u8; 176];
    for (i, &value) in seed.iter().enumerate() {
        key[i] = if value == 0 { i as u8 } else { value };
    }
    for i in 0..40 {
        let start = 12 + i * 4;
        let mut frame: [u8; 4] = key[start..start + 4].try_into().unwrap();
        if i % 4 == 0 {
            frame = [
                MASTER[frame[1] as usize] ^ SCRAMBLE[i / 4],
                MASTER[frame[2] as usize],
                MASTER[frame[3] as usize],
                MASTER[frame[0] as usize],
            ];
        }
        for j in 0..4 {
            key[16 + i * 4 + j] = key[i * 4 + j] ^ frame[j];
        }
    }
    for (i, byte) in data.iter_mut().take(256).enumerate() {
        for round in 0..11 {
            *byte ^= key[round * 16 + i % 16];
        }
    }
}

pub struct DecodedOriginal {
    pub header: AssetHeader,
    pub bytes: Vec<u8>,
    /// Bytes consumed by zlib, excluding any padding; None for plain storage.
    pub compressed_bytes_consumed: Option<u64>,
}

pub fn read_original<R: Read + Seek>(
    reader: &mut R,
    entry: &HedEntry,
) -> io::Result<DecodedOriginal> {
    read_original_limited(reader, entry, MAX_ORIGINAL_BYTES)
}

/// `read_original` with an explicit payload cap, for entries that are
/// legitimately larger than the inspection limit (the ambient sound banks).
pub fn read_original_limited<R: Read + Seek>(
    reader: &mut R,
    entry: &HedEntry,
    limit: u32,
) -> io::Result<DecodedOriginal> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    let package_end = reader.seek(SeekFrom::End(0))?;
    if entry.stored_length < 16
        || entry
            .package_offset
            .checked_add(entry.stored_length as u64)
            .is_none_or(|end| end > package_end)
    {
        return Err(invalid("original indexed range exceeds package"));
    }
    reader.seek(SeekFrom::Start(entry.package_offset))?;
    let mut seed = [0; 16];
    reader.read_exact(&mut seed)?;
    let header = read_asset_header(&seed, entry.stored_length)?;
    let stored = if header.compression_tag >= 0 {
        header.compression_tag as u32
    } else {
        header.decompressed_length
    };
    if stored > limit || header.decompressed_length > limit {
        return Err(invalid(
            "original stored/output size exceeds the read limit",
        ));
    }
    reader.seek(SeekFrom::Start(
        entry.package_offset + 16 + header.remastered_asset_count as u64 * 48,
    ))?;
    let mut bytes = vec![0; stored as usize];
    reader.read_exact(&mut bytes)?;
    let (bytes, compressed_bytes_consumed) = decode_payload_limited(
        &seed,
        header.compression_tag,
        header.decompressed_length,
        bytes,
        limit,
    )?;
    Ok(DecodedOriginal {
        header,
        bytes,
        compressed_bytes_consumed,
    })
}

pub(crate) fn decode_payload(
    seed: &[u8; 16],
    tag: i32,
    expected: u32,
    bytes: Vec<u8>,
) -> io::Result<(Vec<u8>, Option<u64>)> {
    decode_payload_limited(seed, tag, expected, bytes, MAX_ORIGINAL_BYTES)
}

fn decode_payload_limited(
    seed: &[u8; 16],
    tag: i32,
    expected: u32,
    mut bytes: Vec<u8>,
    limit: u32,
) -> io::Result<(Vec<u8>, Option<u64>)> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    if tag < -2 || expected > limit || bytes.len() > limit as usize {
        return Err(invalid("payload tag or size exceeds inspection bounds"));
    }
    if tag > -2 {
        transform_prefix(seed, &mut bytes);
    }
    let compressed_bytes_consumed = if tag >= 0 {
        // One extra output byte detects understated lengths without an unbounded stream.
        let mut output = vec![0; expected as usize + 1];
        let mut decoder = Decompress::new(true);
        let status = decoder
            .decompress(&bytes, &mut output, FlushDecompress::Finish)
            .map_err(|_| invalid("invalid original zlib stream/checksum"))?;
        if status != Status::StreamEnd || decoder.total_out() != expected as u64 {
            return Err(invalid(
                "original zlib output length/termination differs from header",
            ));
        }
        let consumed = decoder.total_in();
        output.truncate(expected as usize);
        bytes = output;
        Some(consumed)
    } else {
        None
    };
    Ok((bytes, compressed_bytes_consumed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::{Cursor, Write};

    fn fixture(tag: i32, original: &[u8], expected: u32) -> (Cursor<Vec<u8>>, HedEntry) {
        let mut stored = if tag >= 0 {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(original).unwrap();
            encoder.finish().unwrap()
        } else {
            original.to_vec()
        };
        let tag = if tag >= 0 { stored.len() as i32 } else { tag };
        let mut seed = [0; 16];
        seed[..4].copy_from_slice(&expected.to_le_bytes());
        seed[4..8].copy_from_slice(&1i32.to_le_bytes());
        seed[8..12].copy_from_slice(&tag.to_le_bytes());
        seed[12..].copy_from_slice(&123i32.to_le_bytes());
        if tag > -2 {
            transform_prefix(&seed, &mut stored);
        }
        let mut bytes = seed.to_vec();
        bytes.extend([0; 48]);
        bytes.extend(stored);
        let entry = HedEntry {
            name_hash: [0; 16],
            package_offset: 0,
            stored_length: bytes.len() as u32,
            original_length: expected,
        };
        (Cursor::new(bytes), entry)
    }

    #[test]
    fn decodes_all_storage_modes_and_retains_exact_lengths() {
        let payload = vec![b'x'; 513];
        for tag in [-2, -1, 0] {
            let (mut reader, entry) = fixture(tag, &payload, payload.len() as u32);
            let decoded = read_original(&mut reader, &entry).unwrap();
            assert_eq!(decoded.bytes, payload);
            assert_eq!(decoded.compressed_bytes_consumed.is_some(), tag >= 0);
        }
        for length in [0, 1, 3, 15, 16, 17, 255, 256, 257] {
            let payload = vec![0x67; length];
            let (mut reader, entry) = fixture(-1, &payload, length as u32);
            assert_eq!(read_original(&mut reader, &entry).unwrap().bytes, payload);
        }
    }

    #[test]
    fn rejects_bad_checksum_output_lengths_ranges_and_limits() {
        for expected in [2, 4, MAX_ORIGINAL_BYTES + 1] {
            let (mut reader, entry) = fixture(0, b"abc", expected);
            assert!(read_original(&mut reader, &entry).is_err());
        }
        let (mut reader, mut entry) = fixture(0, b"abc", 3);
        let last = reader.get_mut().last_mut().unwrap();
        *last ^= 1; // The encrypted checksum is deliberately corrupted.
        assert!(read_original(&mut reader, &entry).is_err());
        entry.stored_length += 1;
        assert!(read_original(&mut reader, &entry).is_err());
        let (mut reader, entry) = fixture(-3, b"abc", 3);
        assert!(read_original(&mut reader, &entry).is_err());
    }

    #[test]
    fn transform_is_bounded_and_handles_partial_words() {
        let seed = [0x42; 16];
        let original: Vec<_> = (0..=255).cycle().take(513).collect();
        let mut bytes = original.clone();
        transform_prefix(&seed, &mut bytes);
        assert_ne!(bytes[..256], original[..256]);
        assert_eq!(bytes[256..], original[256..]);
        transform_prefix(&seed, &mut bytes);
        assert_eq!(bytes, original);
    }
}
