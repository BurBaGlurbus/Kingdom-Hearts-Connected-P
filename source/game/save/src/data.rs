//! One save file: the 0x10fc0-byte image of the game's save structure
//! (`DAT_1409a98b0`, FUN_140152380). The file menu copies it out of the
//! structure on save (FUN_1402ed110) and into it on load (FUN_1402ed0e0).
//!
//! Header: `u32` magic "KH2J" (FUN_140152360), `u32` format version 0x3a,
//! `u32` checksum (FUN_1402eca00 / FUN_1402ecc00).

use crate::container::SLOT_BYTES;

pub const MAGIC: u32 = 0x4a32_484b;
pub const VERSION: u32 = 0x3a;

/// The checksum table of FUN_140325e20: the usual MSB-first CRC-32 step with
/// polynomial 0x04c11db7, except that the original loops 255 times per
/// entry instead of 8 (`lVar3 = 0xff`), which gives different table values.
fn table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut v = (i as u32) << 24;
        for _ in 0..255 {
            let top = v & 0x8000_0000 != 0;
            v <<= 1;
            if top {
                v ^= 0x04c1_1db7;
            }
        }
        *t = v;
    }
    table
}

/// FUN_140325d70 over the header words before the checksum and the body
/// after it: `crc = table[(crc >> 24) ^ byte] ^ (crc << 8)`, start
/// 0xffffffff, result inverted.
pub fn checksum(image: &[u8]) -> u32 {
    let table = table();
    let mut crc = 0xffff_ffffu32;
    for &b in image[..8].iter().chain(&image[12..]) {
        crc = table[((crc >> 24) as u8 ^ b) as usize] ^ (crc << 8);
    }
    !crc
}

#[derive(Debug, PartialEq, Eq)]
pub enum Problem {
    Length(usize),
    Magic(u32),
    Version(u32),
    Checksum { stored: u32, computed: u32 },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length(n) => write!(f, "a save is {SLOT_BYTES} bytes, this one {n}"),
            Self::Magic(m) => write!(f, "unknown save signature {m:#010x}"),
            Self::Version(v) => write!(f, "unsupported save version {v:#x}"),
            Self::Checksum { stored, computed } => {
                write!(f, "checksum {stored:#010x} does not match {computed:#010x}")
            }
        }
    }
}

impl std::error::Error for Problem {}

/// The save image. Fields the engine does not model stay as loaded, so a
/// file the original wrote is written back with only the changes made.
#[derive(Clone, PartialEq, Eq)]
pub struct SaveData(Vec<u8>);

impl std::fmt::Debug for SaveData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SaveData({} bytes, checksum {:#010x})",
            self.0.len(),
            self.u32_at(8)
        )
    }
}

impl SaveData {
    /// A zeroed image with the header filled in (FUN_1401523a0's first steps).
    pub fn blank() -> Self {
        let mut image = vec![0u8; SLOT_BYTES];
        image[..4].copy_from_slice(&MAGIC.to_le_bytes());
        image[4..8].copy_from_slice(&VERSION.to_le_bytes());
        let mut data = Self(image);
        data.seal();
        data
    }

    /// FUN_1402ecc00: accepts an image whose signature, version and
    /// checksum are right.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Problem> {
        if bytes.len() != SLOT_BYTES {
            return Err(Problem::Length(bytes.len()));
        }
        let data = Self(bytes.to_vec());
        let stored = data.u32_at(8);
        let computed = checksum(&data.0);
        if stored != computed {
            return Err(Problem::Checksum { stored, computed });
        }
        if data.u32_at(0) != MAGIC {
            return Err(Problem::Magic(data.u32_at(0)));
        }
        if data.u32_at(4) != VERSION {
            return Err(Problem::Version(data.u32_at(4)));
        }
        Ok(data)
    }

    /// FUN_1402eca00: stores the checksum.
    pub fn seal(&mut self) {
        let sum = checksum(&self.0);
        self.0[8..12].copy_from_slice(&sum.to_le_bytes());
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn u8_at(&self, offset: usize) -> u8 {
        self.0[offset]
    }

    pub fn u16_at(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.0[offset], self.0[offset + 1]])
    }

    pub fn u32_at(&self, offset: usize) -> u32 {
        u32::from_le_bytes([
            self.0[offset],
            self.0[offset + 1],
            self.0[offset + 2],
            self.0[offset + 3],
        ])
    }

    pub fn set_u8(&mut self, offset: usize, value: u8) {
        self.0[offset] = value;
    }

    pub fn set_u16(&mut self, offset: usize, value: u16) {
        self.0[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn set_u32(&mut self, offset: usize, value: u32) {
        self.0[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    pub fn slice(&self, offset: usize, len: usize) -> &[u8] {
        &self.0[offset..offset + len]
    }

    pub fn set_slice(&mut self, offset: usize, bytes: &[u8]) {
        self.0[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_image_validates_and_edits_break_the_checksum() {
        let mut data = SaveData::blank();
        assert!(SaveData::from_bytes(data.bytes()).is_ok());
        data.set_u32(0x2440, 1234);
        assert!(matches!(
            SaveData::from_bytes(data.bytes()),
            Err(Problem::Checksum { .. })
        ));
        data.seal();
        let back = SaveData::from_bytes(data.bytes()).unwrap();
        assert_eq!(back.u32_at(0x2440), 1234);
    }

    #[test]
    fn header_is_checked() {
        let mut data = SaveData::blank();
        data.set_u32(0, 0x1234_5678);
        data.seal();
        assert_eq!(
            SaveData::from_bytes(data.bytes()).err(),
            Some(Problem::Magic(0x1234_5678))
        );
        let mut data = SaveData::blank();
        data.set_u32(4, 0x39);
        data.seal();
        assert_eq!(
            SaveData::from_bytes(data.bytes()).err(),
            Some(Problem::Version(0x39))
        );
        assert_eq!(
            SaveData::from_bytes(&[0; 10]).err(),
            Some(Problem::Length(10))
        );
    }

    #[test]
    fn table_uses_the_originals_255_steps() {
        // Entry 1 after 255 shifts is not the standard 0x04c11db7 entry.
        let t = table();
        assert_eq!(t[0], 0);
        assert_ne!(t[1], 0x04c1_1db7);
    }
}
