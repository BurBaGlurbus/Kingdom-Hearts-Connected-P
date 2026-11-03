//! A player's identity: a random secret made once per install, and the id
//! that comes from it (WP-23, KHC-044).
//!
//! [hub choice, KHC-044] The id is the first 128 bits of the secret's
//! SHA-256. Nothing proves yet that a player holds the secret behind the id
//! they send: that needs a signature, and the signature crate is the owner's
//! decision (the one Brief 3's encryption waits on too). Until then every id
//! is unverified (`Config::verified_ids`), and a public hub keeps levels and
//! friends off.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const ID_BYTES: usize = 16;
pub const SECRET_BYTES: usize = 32;

/// A player id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct PlayerId(pub [u8; ID_BYTES]);

impl PlayerId {
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(text: &str) -> Option<PlayerId> {
        let text = text.trim();
        if text.len() != ID_BYTES * 2 || !text.is_ascii() {
            return None;
        }
        let mut id = [0; ID_BYTES];
        for (i, b) in id.iter_mut().enumerate() {
            *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(PlayerId(id))
    }
}

impl fmt::Debug for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PlayerId({})", self.hex())
    }
}

/// The secret. Its debug output never shows it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(pub [u8; SECRET_BYTES]);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(redacted)")
    }
}

impl Secret {
    pub fn id(&self) -> PlayerId {
        let digest = Sha256::digest(self.0);
        let mut id = [0; ID_BYTES];
        id.copy_from_slice(&digest[..ID_BYTES]);
        PlayerId(id)
    }

    /// The recovery code: the secret and a two-byte check in base 32, in
    /// groups of five. Typed on a new PC it gives back the same id.
    pub fn recovery_code(&self) -> String {
        let mut bytes = self.0.to_vec();
        bytes.extend_from_slice(&check(&self.0));
        let text = base32(&bytes);
        text.as_bytes()
            .chunks(5)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect::<Vec<_>>()
            .join("-")
    }

    /// The secret from a recovery code, if it reads and its check matches.
    /// Case, dashes and spaces do not matter.
    pub fn from_recovery_code(code: &str) -> Option<Secret> {
        let clean: String = code
            .chars()
            .filter(|c| !matches!(c, '-' | ' '))
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let bytes = unbase32(&clean)?;
        if bytes.len() != SECRET_BYTES + 2 {
            return None;
        }
        let mut secret = [0; SECRET_BYTES];
        secret.copy_from_slice(&bytes[..SECRET_BYTES]);
        (bytes[SECRET_BYTES..] == check(&secret)).then_some(Secret(secret))
    }

    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(text: &str) -> Option<Secret> {
        let text = text.trim();
        if text.len() != SECRET_BYTES * 2 || !text.is_ascii() {
            return None;
        }
        let mut s = [0; SECRET_BYTES];
        for (i, b) in s.iter_mut().enumerate() {
            *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Secret(s))
    }
}

fn check(secret: &[u8; SECRET_BYTES]) -> [u8; 2] {
    let mut h = Sha256::new();
    h.update(b"KHC recovery");
    h.update(secret);
    let d = h.finalize();
    [d[0], d[1]]
}

/// Crockford's alphabet: no I, L, O or U, so a code copied by hand reads back.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn base32(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0);
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn unbase32(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes() {
        // The letters a hand might write for the digits.
        let c = match c {
            b'O' => b'0',
            b'I' | b'L' => b'1',
            c => c,
        };
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
        acc &= (1 << bits) - 1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(seed: u8) -> Secret {
        let mut s = [0; SECRET_BYTES];
        for (i, b) in s.iter_mut().enumerate() {
            *b = seed.wrapping_mul(31).wrapping_add(i as u8 * 7);
        }
        Secret(s)
    }

    #[test]
    fn the_id_comes_from_the_secret() {
        let a = secret(1);
        assert_eq!(a.id(), a.clone().id());
        assert_ne!(a.id(), secret(2).id());
        let digest = Sha256::digest(a.0);
        assert_eq!(a.id().0, digest[..16]);
        assert_eq!(PlayerId::from_hex(&a.id().hex()), Some(a.id()));
        assert_eq!(Secret::from_hex(&a.hex()), Some(a));
    }

    #[test]
    fn debug_output_hides_the_secret() {
        let s = secret(3);
        let shown = format!("{s:?} {:?}", vec![s.clone()]);
        assert!(!shown.contains(&s.hex()));
        assert!(!shown.contains(&format!("{}", s.0[0])));
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn a_recovery_code_restores_the_secret() {
        for seed in 0..20 {
            let s = secret(seed);
            let code = s.recovery_code();
            assert_eq!(Secret::from_recovery_code(&code), Some(s.clone()));
            assert_eq!(
                Secret::from_recovery_code(&code.to_lowercase().replace('-', " ")),
                Some(s.clone())
            );
            // One character changed is refused by the check.
            let mut wrong: Vec<char> = code.chars().collect();
            wrong[3] = if wrong[3] == 'A' { 'B' } else { 'A' };
            let wrong: String = wrong.into_iter().collect();
            assert_ne!(Secret::from_recovery_code(&wrong), Some(s));
        }
        assert_eq!(Secret::from_recovery_code("not a code"), None);
        assert_eq!(Secret::from_recovery_code(""), None);
    }
}
