//! Limit Form's four limits (Sonic Rave, Last Arcanum, Strike Raid and
//! Infinity): their commands, the Limit rows' colour and the tint's ramp
//! out. The limits themselves run on the object-script engine: the form's
//! own BDX program (`limitform`, P_EX100_KH1F.mdlx 'limi') is started as a
//! limit scene (FUN_1403d8b40) by the room's `party_limit`.

/// The four limits and their follow-ups (03system `cmd`).
pub const SONIC_RAVE: u16 = 0x2ba;
pub const RAVE: u16 = 0x2bb;
pub const FINISH: u16 = 0x2bc;
pub const LAST_ARCANUM: u16 = 0x2bd;
pub const BASH: u16 = 0x2be;
pub const THE_END: u16 = 0x2bf;
pub const STRIKE_RAID: u16 = 0x2c0;
pub const RAID: u16 = 0x2c1;
pub const JUDGMENT: u16 = 0x2c2;
pub const INFINITY: u16 = 0x2ab;
pub const SHOOT: u16 = 0x2ac;
pub const IMPACT: u16 = 0x2ad;
pub const GIGA_IMPACT: u16 = 0x2ae;
/// `DAT_14074ebe8` / `DAT_1405c9678`: the Limit submenu's rows and the L1
/// shortcuts' replacements, in order.
pub const LIMITS: [u16; 4] = [SONIC_RAVE, LAST_ARCANUM, STRIKE_RAID, INFINITY];
/// The "Limit" command of the Magic slot (submenu 12).
pub const LIMIT_COMMAND: u16 = 0x2c3;

/// The tint ramps back out over 30 ticks once a limit ends (FUN_1403d83e0,
/// `DAT_140623bcc`).
pub const TINT_OUT: f32 = 30.;

/// The limit-row colour (FUN_1403d88e0): 5 while a limit runs or MP
/// charges, 2 (orange: usable, empties MP into a charge) when MP is short,
/// else 0.
pub fn colour(cost: u8, running: bool, mp: i32, charging: bool) -> usize {
    if running {
        5
    } else if cost < 1 {
        0
    } else if charging {
        5
    } else if mp < cost as i32 {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_rows_grey_while_charging_or_running_and_orange_when_short() {
        assert_eq!(colour(60, false, 100, false), 0);
        assert_eq!(colour(60, false, 40, false), 2);
        assert_eq!(colour(60, false, 100, true), 5);
        assert_eq!(colour(60, true, 100, false), 5);
    }
}
