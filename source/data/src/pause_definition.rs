//! Pause definitions: the `slct` rows of `14mission.bar` that `FUN_1402e4c20`
//! opens as the game's mode-8 menu (the cutscene pause, the game-over list,
//! the Retry list). A table is a version word, a row count and 0x40-byte
//! rows (reader `FUN_1403ad480`, list builder `FUN_14031f1f0`, backdrop
//! `FUN_14031d3c0`).
use std::io;

/// Most items a row lists (`+4`, four result/message pairs).
pub const MAX_ITEMS: usize = 4;
const ROW_SIZE: usize = 0x40;
const HEADER_SIZE: usize = 8;

/// One list item: the result id the menu reports and its `sys.bar` message
/// (`0x8000 | id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub result: u16,
    pub message: u16,
}

/// One 0x40-byte row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    /// +0: the definition id the game asks for.
    pub id: u16,
    /// +4..: the items in list order (+2 is their count).
    pub items: Vec<Item>,
    /// +0x14: the `pause.2ld` backdrop group (-1: none), drawn at sprite
    /// priority 0x7531.
    pub backdrop: i16,
    /// +0x16: the panel group (-1: none).
    pub panel: i16,
    /// +0x24 / +0x25 / +0x26: flags. Bit 0 of +0x25 is the full-screen mode
    /// (the menu clears the screen fade and the pad's Cancel and Start do
    /// nothing).
    pub flags: [u8; 3],
}

impl Definition {
    /// Whether the menu owns the screen (`+0x25` bit 0): Cancel and Start
    /// are ignored.
    pub fn exclusive(&self) -> bool {
        self.flags[1] & 1 != 0
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

/// Reads every row of a `slct` entry.
pub fn read(bytes: &[u8]) -> io::Result<Vec<Definition>> {
    let word = |at: usize| -> io::Result<u16> {
        bytes
            .get(at..at + 2)
            .map(|w| u16::from_le_bytes([w[0], w[1]]))
            .ok_or_else(|| invalid("pause definition truncated"))
    };
    let count = u32::from_le_bytes(
        bytes
            .get(4..8)
            .ok_or_else(|| invalid("pause definition header truncated"))?
            .try_into()
            .unwrap(),
    ) as usize;
    if count > 256 || bytes.len() < HEADER_SIZE + count * ROW_SIZE {
        return Err(invalid("pause definition table exceeds its entry"));
    }
    (0..count)
        .map(|row| {
            let at = HEADER_SIZE + row * ROW_SIZE;
            let n = (word(at + 2)? as usize).min(MAX_ITEMS);
            let items = (0..n)
                .map(|i| {
                    Ok(Item {
                        result: word(at + 4 + 4 * i)?,
                        message: word(at + 6 + 4 * i)?,
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            Ok(Definition {
                id: word(at)?,
                items,
                backdrop: word(at + 0x14)? as i16,
                panel: word(at + 0x16)? as i16,
                flags: [bytes[at + 0x24], bytes[at + 0x25], bytes[at + 0x26]],
            })
        })
        .collect()
}

/// The row for `id` (`FUN_1403ad480`'s binary search over the sorted ids).
pub fn find(definitions: &[Definition], id: u16) -> Option<&Definition> {
    definitions.iter().find(|d| d.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u16, items: &[(u16, u16)], backdrop: i16, panel: i16, flags: [u8; 3]) -> Vec<u8> {
        let mut r = vec![0u8; ROW_SIZE];
        r[0..2].copy_from_slice(&id.to_le_bytes());
        r[2..4].copy_from_slice(&(items.len() as u16).to_le_bytes());
        for (i, (result, message)) in items.iter().enumerate() {
            r[4 + 4 * i..6 + 4 * i].copy_from_slice(&result.to_le_bytes());
            r[6 + 4 * i..8 + 4 * i].copy_from_slice(&message.to_le_bytes());
        }
        r[0x14..0x16].copy_from_slice(&backdrop.to_le_bytes());
        r[0x16..0x18].copy_from_slice(&panel.to_le_bytes());
        r[0x24..0x27].copy_from_slice(&flags);
        r
    }

    fn table(rows: &[Vec<u8>]) -> Vec<u8> {
        let mut t = vec![2, 0, 0, 0];
        t.extend((rows.len() as u32).to_le_bytes());
        for r in rows {
            t.extend(r);
        }
        t
    }

    #[test]
    fn rows_decode_in_order() {
        let t = table(&[
            row(1, &[(2, 0x8001), (1, 0x8002)], 3, -1, [0, 1, 1]),
            row(4, &[(6, 0x8003)], 0, 15, [0, 0, 1]),
        ]);
        let d = read(&t).unwrap();
        assert_eq!(d.len(), 2);
        let one = find(&d, 1).unwrap();
        assert_eq!(
            one.items,
            vec![
                Item {
                    result: 2,
                    message: 0x8001
                },
                Item {
                    result: 1,
                    message: 0x8002
                }
            ]
        );
        assert_eq!((one.backdrop, one.panel), (3, -1));
        assert!(one.exclusive());
        assert!(!find(&d, 4).unwrap().exclusive());
        assert!(find(&d, 9).is_none());
    }

    #[test]
    fn counts_clamp_and_truncation_is_rejected() {
        let mut r = row(1, &[], 0, 0, [0; 3]);
        r[2] = 9;
        let d = read(&table(&[r])).unwrap();
        assert_eq!(d[0].items.len(), MAX_ITEMS);
        let t = table(&[row(1, &[], 0, 0, [0; 3])]);
        assert!(read(&t[..t.len() - 1]).is_err());
    }
}
