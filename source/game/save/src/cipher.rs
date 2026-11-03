//! The save container's header cipher. The original code (FUN_1401003e0 /
//! FUN_140100810) derives a 16-byte key from the Steam id, expands it with
//! AES's key schedule over a substitution table stored in the executable
//! (`DAT_140713540`, loaded at run time from the user's install), and
//! "encrypts" each 16-byte block of the first 0x100 container bytes with
//! FUN_140143f90 / FUN_140144280. Those two routines only XOR the block with
//! the round keys one after another, so the whole cipher is one 16-byte XOR
//! mask, the XOR of all eleven round keys.

use crate::md5::md5;

/// Virtual address of the substitution table in the executable.
pub const SBOX_ADDRESS: u64 = 0x1_4071_3540;
const IMAGE_BASE: u64 = 0x1_4000_0000;

#[derive(Debug, PartialEq, Eq)]
pub enum CipherError {
    NotAnExecutable,
    TableOutsideImage,
    TableNotAPermutation,
}

impl std::fmt::Display for CipherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotAnExecutable => "the file is not a 64-bit Windows executable",
            Self::TableOutsideImage => "the cipher table lies outside the executable image",
            Self::TableNotAPermutation => "the cipher table is not a byte permutation",
        })
    }
}

impl std::error::Error for CipherError {}

/// The 256-byte substitution table the key schedule uses.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sbox(pub [u8; 256]);

impl Sbox {
    /// Reads the table from the executable's bytes (the section that holds
    /// `SBOX_ADDRESS`).
    pub fn from_executable(exe: &[u8]) -> Result<Self, CipherError> {
        let u16_at = |o: usize| exe.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| {
            exe.get(o..o + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        if exe.get(..2) != Some(b"MZ") {
            return Err(CipherError::NotAnExecutable);
        }
        let pe = u32_at(0x3c).ok_or(CipherError::NotAnExecutable)? as usize;
        if exe.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(CipherError::NotAnExecutable);
        }
        let sections = u16_at(pe + 6).ok_or(CipherError::NotAnExecutable)? as usize;
        let optional = u16_at(pe + 20).ok_or(CipherError::NotAnExecutable)? as usize;
        let rva = (SBOX_ADDRESS - IMAGE_BASE) as u32;
        for i in 0..sections {
            let s = pe + 24 + optional + i * 40;
            let (Some(size), Some(va), Some(raw_size), Some(raw)) = (
                u32_at(s + 8),
                u32_at(s + 12),
                u32_at(s + 16),
                u32_at(s + 20),
            ) else {
                return Err(CipherError::NotAnExecutable);
            };
            if rva >= va && rva + 256 <= va + size.min(raw_size) {
                let start = (raw + (rva - va)) as usize;
                let table = exe
                    .get(start..start + 256)
                    .ok_or(CipherError::TableOutsideImage)?;
                return Self::from_table(table.try_into().expect("256 bytes"));
            }
        }
        Err(CipherError::TableOutsideImage)
    }

    /// The table must be a permutation of all 256 byte values.
    pub fn from_table(table: [u8; 256]) -> Result<Self, CipherError> {
        let mut seen = [false; 256];
        for b in table {
            seen[b as usize] = true;
        }
        if seen.iter().all(|&s| s) {
            Ok(Self(table))
        } else {
            Err(CipherError::TableNotAPermutation)
        }
    }
}

/// The container key: MD5 of the Steam id's decimal digits followed by "1"
/// (`snprintf("%llu%d", steam_id, 1)` at 0x1400ff3a2, then FUN_140145930).
pub fn key_from_steam_id(steam_id: u64) -> [u8; 16] {
    md5(format!("{steam_id}1").as_bytes())
}

/// The 16-byte XOR mask for `key`. Bytes of the key that are zero are
/// replaced by their index before the schedule (FUN_1401003e0).
pub fn mask(key: &[u8; 16], sbox: &Sbox) -> [u8; 16] {
    let mut k = *key;
    for (i, b) in k.iter_mut().enumerate() {
        if *b == 0 {
            *b = i as u8;
        }
    }
    // FUN_140144350: AES-128 key expansion (Nk 4, Nr 10) over `sbox`.
    let mut words = [[0u8; 4]; 44];
    for (i, w) in words.iter_mut().take(4).enumerate() {
        w.copy_from_slice(&k[i * 4..i * 4 + 4]);
    }
    let mut rcon = 1u8;
    for i in 4..44 {
        let mut t = words[i - 1];
        if i % 4 == 0 {
            t = [t[1], t[2], t[3], t[0]];
            for b in &mut t {
                *b = sbox.0[*b as usize];
            }
            t[0] ^= rcon;
            rcon = if rcon & 0x80 != 0 {
                (rcon << 1) ^ 0x1b
            } else {
                rcon << 1
            };
        }
        for j in 0..4 {
            words[i][j] = words[i - 4][j] ^ t[j];
        }
    }
    let mut out = [0u8; 16];
    for (i, w) in words.iter().enumerate() {
        for j in 0..4 {
            out[(i % 4) * 4 + j] ^= w[j];
        }
    }
    out
}

/// Applies the mask to the blocks the original touches: 16-byte blocks of
/// the first 0x100 bytes, only for buffers longer than 16 bytes. The same
/// call encrypts and decrypts.
pub fn apply(buffer: &mut [u8], mask: &[u8; 16]) {
    if buffer.len() <= 0x10 {
        return;
    }
    let whole = buffer.len() - buffer.len() % 16;
    let mut at = 0;
    while at < whole && at < 0x100 {
        for (b, m) in buffer[at..at + 16].iter_mut().zip(mask) {
            *b ^= m;
        }
        at += 16;
    }
}

#[cfg(test)]
pub(crate) fn aes_sbox() -> Sbox {
    // Rijndael's own table, for tests with a known-answer vector.
    let mut table = [0u8; 256];
    let (mut p, mut q) = (1u8, 1u8);
    loop {
        p = p ^ (p << 1) ^ if p & 0x80 != 0 { 0x1b } else { 0 };
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let x = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4);
        table[p as usize] = x ^ 0x63;
        if p == 1 {
            break;
        }
    }
    table[0] = 0x63;
    Sbox::from_table(table).expect("AES table is a permutation")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(d: &[u8]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn mask_is_the_xor_of_the_round_keys() {
        let key: [u8; 16] = std::array::from_fn(|i| i as u8);
        // The first key byte is zero and is replaced by its index, 0.
        let m = mask(&key, &aes_sbox());
        assert_eq!(hex(&m), "e7904e13ff0bab87492f34c1ffbed90d");
    }

    #[test]
    fn key_is_md5_of_the_id_and_a_one() {
        assert_eq!(
            hex(&key_from_steam_id(76561198000000001)),
            "9c038ad2c9a26d628229e4a5c6f2e563"
        );
    }

    #[test]
    fn apply_covers_sixteen_blocks_and_round_trips() {
        let m = [0xa5u8; 16];
        let original: Vec<u8> = (0..0x400u32).map(|i| i as u8).collect();
        let mut buffer = original.clone();
        apply(&mut buffer, &m);
        assert!(
            buffer[..0x100]
                .iter()
                .zip(&original)
                .all(|(a, b)| a ^ b == 0xa5)
        );
        assert_eq!(buffer[0x100..], original[0x100..]);
        apply(&mut buffer, &m);
        assert_eq!(buffer, original);
        let mut tiny = [1u8; 16];
        apply(&mut tiny, &m);
        assert_eq!(tiny, [1u8; 16]);
    }

    #[test]
    fn table_must_be_a_permutation() {
        assert_eq!(
            Sbox::from_table([7; 256]),
            Err(CipherError::TableNotAPermutation)
        );
        assert!(Sbox::from_executable(b"not a program").is_err());
    }
}
