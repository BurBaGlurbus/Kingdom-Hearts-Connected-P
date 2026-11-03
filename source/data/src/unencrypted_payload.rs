//! Inspection of originals explicitly tagged as unencrypted and uncompressed.
use crate::{AssetHeader, HedEntry, read_asset_header};
use std::io::{self, Read, Seek, SeekFrom};

pub const MAX_PLAIN_BYTES: u32 = 32 * 1024 * 1024;

pub struct PlainOriginal {
    pub header: AssetHeader,
    pub bytes: Vec<u8>,
}

/// No transform is applied. All other storage tags are rejected.
pub fn read_plain_original<R: Read + Seek>(
    reader: &mut R,
    entry: &HedEntry,
) -> io::Result<PlainOriginal> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    let end = reader.seek(SeekFrom::End(0))?;
    if entry.stored_length < 16
        || entry
            .package_offset
            .checked_add(entry.stored_length as u64)
            .is_none_or(|limit| limit > end)
    {
        return Err(invalid("plain original range exceeds package"));
    }
    reader.seek(SeekFrom::Start(entry.package_offset))?;
    let mut header_bytes = [0; 16];
    reader.read_exact(&mut header_bytes)?;
    let header = read_asset_header(&header_bytes, entry.stored_length)?;
    if header.compression_tag != -2 {
        return Err(invalid(
            "plain inspection requires unencrypted, uncompressed tag -2",
        ));
    }
    if header.decompressed_length > MAX_PLAIN_BYTES {
        return Err(invalid("plain original exceeds 32 MiB inspection limit"));
    }
    let start = entry.package_offset + 16 + header.remastered_asset_count as u64 * 48;
    reader.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; header.decompressed_length as usize];
    reader.read_exact(&mut bytes)?;
    Ok(PlainOriginal { header, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn fixture(tag: i32, count: u32, payload: &[u8]) -> (Cursor<Vec<u8>>, HedEntry) {
        let mut bytes = vec![0; 32];
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(count.to_le_bytes());
        bytes.extend(tag.to_le_bytes());
        bytes.extend(0i32.to_le_bytes());
        bytes.resize(bytes.len() + count as usize * 48, 0);
        bytes.extend(payload);
        let entry = HedEntry {
            name_hash: [0; 16],
            package_offset: 32,
            stored_length: (bytes.len() - 32) as u32,
            original_length: payload.len() as u32,
        };
        (Cursor::new(bytes), entry)
    }

    #[test]
    fn reads_exact_original_after_replacement_metadata_without_transform() {
        let (mut reader, entry) = fixture(-2, 2, b"synthetic original");
        let result = read_plain_original(&mut reader, &entry).unwrap();
        assert_eq!(result.bytes, b"synthetic original");
        assert_eq!(result.header.remastered_asset_count, 2);
    }

    #[test]
    fn rejects_other_tags_truncated_ranges_and_oversized_originals() {
        for tag in [-1, 0, 1] {
            let (mut reader, entry) = fixture(tag, 0, b"synthetic original");
            assert!(read_plain_original(&mut reader, &entry).is_err());
        }
        let (mut reader, mut entry) = fixture(-2, 0, b"synthetic original");
        entry.stored_length += 1;
        assert!(read_plain_original(&mut reader, &entry).is_err());
        let (mut reader, mut entry) = fixture(-2, 0, b"");
        reader.get_mut()[32..36].copy_from_slice(&(MAX_PLAIN_BYTES + 1).to_le_bytes());
        // A sparse virtual package avoids allocating an oversized test payload.
        struct LargePackage {
            prefix: Cursor<Vec<u8>>,
            end: u64,
        }
        impl Read for LargePackage {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                self.prefix.read(buffer)
            }
        }
        impl Seek for LargePackage {
            fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
                match from {
                    SeekFrom::End(0) => Ok(self.end),
                    _ => self.prefix.seek(from),
                }
            }
        }
        entry.stored_length = MAX_PLAIN_BYTES + 17;
        let end = entry.package_offset + entry.stored_length as u64;
        assert!(
            read_plain_original(
                &mut LargePackage {
                    prefix: reader,
                    end
                },
                &entry
            )
            .is_err()
        );
    }
}
