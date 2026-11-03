//! 03system.bin `cmd` table (FUN_1403b68b0 finds a command by id): u32
//! version, u32 count, then 0x30-byte records.
use std::io;

pub const RECORD_SIZE: usize = 0x30;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Command {
    pub id: u16,
    /// +8: the command's name message.
    pub name: u32,
    /// +2: what the dispatcher does (1 Attack, 2 Magic, 3 Item, 4 Drive...).
    pub execute: u16,
    /// +4: the argument (the magic type for Execute 2).
    pub argument: i16,
    /// +6: submenu opened on decide (-1 none, 4 the party list for Cure).
    pub submenu: i8,
    /// +7: the row icon.
    pub icon: u8,
    /// +0xc: flags.
    pub flags: u32,
    /// +0x1c: the MP cost of a spell (0xff = all MP) or the Drive cost.
    pub cost: u8,
    /// +0x20: a limit's gauge time in ticks (FUN_1403d8b40 hands it to
    /// the limit object as a float).
    pub time: u16,
    /// +0x1f: whose command windows decide (FUN_1403b6950): 0 the user's,
    /// 1 the target's, 2 both.
    pub decider: u8,
    /// +0x22: the motion window (range 0x1a's value, obj+0x638) that also
    /// takes this command (FUN_1403ba070).
    pub chain: u16,
    /// +0x2c: forms in which the command is unusable (bit f).
    pub form_mask: u16,
    /// +0x2e: the command group.
    pub group: u8,
    /// The fields the reaction-command search reads.
    pub reaction: Reaction,
}

/// The reaction-command fields of a record (FUN_140407de0, FUN_1404088f0,
/// FUN_140408080, FUN_140408520).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reaction {
    /// +0x10: the cosine Sora's facing must reach toward the target (-1 none).
    pub facing_cos: f32,
    /// +0x14 and +0x18: a yaw offset and the cosine the target's own facing
    /// must reach toward Sora (-1 none).
    pub target_yaw: f32,
    pub target_cos: f32,
    /// +0x1d: the camera on execute (1 watch the target, 3 the partner).
    pub camera: u8,
    /// +0x1e: the offer's priority.
    pub priority: u8,
    /// +0x24: the target marker (index into the marker effect table).
    pub mark: u8,
    /// +0x25: what execution does to Sora's current action (1, 2).
    pub action: u8,
    /// +0x26: the usage counter index.
    pub counter: u16,
    /// +0x28: past this horizontal distance the offer is dimmed (0 none).
    pub distance: u16,
}

pub fn read(bytes: &[u8]) -> io::Result<Vec<Command>> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "command table truncated");
    let count = bytes
        .get(4..8)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()) as usize)
        .ok_or_else(invalid)?;
    let records = bytes.get(8..8 + count * RECORD_SIZE).ok_or_else(invalid)?;
    Ok(records
        .as_chunks::<RECORD_SIZE>()
        .0
        .iter()
        .map(|r| Command {
            id: u16::from_le_bytes([r[0], r[1]]),
            name: u32::from_le_bytes([r[8], r[9], r[10], r[11]]),
            execute: u16::from_le_bytes([r[2], r[3]]),
            argument: i16::from_le_bytes([r[4], r[5]]),
            submenu: r[6] as i8,
            icon: r[7],
            flags: u32::from_le_bytes([r[12], r[13], r[14], r[15]]),
            cost: r[0x1c],
            decider: r[0x1f],
            time: u16::from_le_bytes([r[0x20], r[0x21]]),
            chain: u16::from_le_bytes([r[0x22], r[0x23]]),
            form_mask: u16::from_le_bytes([r[0x2c], r[0x2d]]),
            group: r[0x2e],
            reaction: Reaction {
                facing_cos: f32::from_le_bytes([r[0x10], r[0x11], r[0x12], r[0x13]]),
                target_yaw: f32::from_le_bytes([r[0x14], r[0x15], r[0x16], r[0x17]]),
                target_cos: f32::from_le_bytes([r[0x18], r[0x19], r[0x1a], r[0x1b]]),
                camera: r[0x1d],
                priority: r[0x1e],
                mark: r[0x24],
                action: r[0x25],
                counter: u16::from_le_bytes([r[0x26], r[0x27]]),
                distance: u16::from_le_bytes([r[0x28], r[0x29]]),
            },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ids_and_names() {
        let mut b = vec![2, 0, 0, 0, 1, 0, 0, 0];
        let mut r = [0u8; RECORD_SIZE];
        r[0] = 0x17;
        r[2] = 2;
        r[4] = 3;
        r[6] = 0xff;
        r[8..12].copy_from_slice(&0x81e0u32.to_le_bytes());
        r[0x1c] = 12;
        b.extend(r);
        let c = read(&b).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].id, c[0].name, c[0].execute), (0x17, 0x81e0, 2));
        assert_eq!((c[0].argument, c[0].submenu, c[0].cost), (3, -1, 12));
        assert!(read(&b[..20]).is_err());
    }
}
