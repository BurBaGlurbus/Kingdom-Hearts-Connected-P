//! Model object-collision tables (BAR type 23 in an mdlx), following the
//! public OpenKH ObjectCollision.cs (7a3b945c538d32c6a285128c98aefba093f52ceb):
//! an entry count, padding to 0x40, then 20-byte entries.
use std::io;

pub const HIT: u8 = 2;
/// Target shape (lock-on / talk camera fallback, FUN_1403be230).
pub const TARGET: u8 = 3;
/// Reaction-command shape (FUN_140407de0).
pub const REACTION: u8 = 5;
pub const ATTACK: u8 = 6;
pub const SPHERE: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub group: u8,
    /// HP pool hit (executable: damage index).
    pub parts: u8,
    /// Flinch height selector for HIT entries.
    pub argument: i16,
    pub kind: u8,
    pub shape: u8,
    pub bone: i16,
    pub position: [i16; 3],
    /// Height offset subtracted from the centre's (Y-down) Y.
    pub position_height: i16,
    pub radius: i16,
    pub height: i16,
}

/// The table's default group mask (u32 at +4): the groups whose shapes are
/// on unless a motion or script switches them (FUN_1403ba550's last test).
pub fn default_groups(bytes: &[u8]) -> io::Result<u32> {
    bytes
        .get(4..8)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "object collision header"))
}

pub fn read(bytes: &[u8]) -> io::Result<Vec<Entry>> {
    let invalid = |m| io::Error::new(io::ErrorKind::InvalidData, m);
    let count = bytes
        .get(..4)
        .map(|w| i32::from_le_bytes(w.try_into().unwrap()))
        .ok_or_else(|| invalid("object collision header"))?;
    let count = usize::try_from(count).map_err(|_| invalid("negative collision count"))?;
    if count > 4096 || bytes.len() < 0x40 + count * 20 {
        return Err(invalid("object collision table truncated"));
    }
    let h = |b: &[u8], at: usize| i16::from_le_bytes([b[at], b[at + 1]]);
    Ok((0..count)
        .map(|i| {
            let e = &bytes[0x40 + i * 20..0x40 + (i + 1) * 20];
            Entry {
                group: e[0],
                parts: e[1],
                argument: h(e, 2),
                kind: e[4],
                shape: e[5],
                bone: h(e, 6),
                position: [h(e, 8), h(e, 10), h(e, 12)],
                position_height: h(e, 14),
                radius: h(e, 16),
                height: h(e, 18),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_synthetic_entries() {
        let mut b = vec![0u8; 0x40];
        b[0] = 1;
        let mut e = [0u8; 20];
        e[0] = 1;
        e[4] = ATTACK;
        e[5] = SPHERE;
        e[6] = 2;
        e[8..10].copy_from_slice(&(-40i16).to_le_bytes());
        e[16] = 40;
        b.extend(e);
        let t = read(&b).unwrap();
        assert_eq!(t[0].position, [-40, 0, 0]);
        assert_eq!((t[0].bone, t[0].radius, t[0].kind), (2, 40, ATTACK));
        assert!(read(&b[..0x40 + 10]).is_err());
    }
}
