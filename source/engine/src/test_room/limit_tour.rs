//! The unattended `--party-limit-tour <dir>` (in the magic tour's setup:
//! Sora still, the Shadow frozen nearby, so the battle flag is on): left to
//! the second deck, then each entry of the Limit list in turn, the reaction
//! button pressed while a follow-up is offered. Before each limit Sora's MP
//! is full and not charging, and he holds Trinity Limit's ability
//! [test-room values: the room's Sora has no MP to spare between limits].
use visual::field_command_menu::Buttons;

/// The Limit list's entries the tour runs (the list's order: Whirli-Goof,
/// Knocksmash, Comet, Duck Flare, Trinity).
pub const LIMITS: u64 = 5;
/// Ticks each limit gets, from opening the list to the next one.
const STEP: u64 = 1100;
const FIRST: u64 = 200;
/// Ticks without a reaction command before the tour takes a row's command.
pub const REACTION_QUIET: u64 = 240;
/// How far in front of the Shadow Sora stands before each limit.
pub const STAND_OFF: f32 = 200.;
/// Trinity Limit (ability item 0xc6), given to Sora for the tour.
pub const TRINITY_ABILITY: u16 = 0xc6;

#[derive(Clone, Copy)]
enum Press {
    Down,
    Decide,
    Left,
}

fn schedule(tick: u64) -> Option<Press> {
    if tick == FIRST - 40 {
        return Some(Press::Left);
    }
    let t = tick.checked_sub(FIRST)?;
    let (k, at) = (t / STEP, t % STEP);
    if k >= LIMITS {
        return None;
    }
    // Down to the Limit row (the cursor is back on Attack), open the
    // list, down to entry k, decide.
    match at {
        0 | 10 | 20 => Some(Press::Down),
        40 => Some(Press::Decide),
        _ if (60..60 + 10 * k).contains(&at) && (at - 60).is_multiple_of(10) => Some(Press::Down),
        120 => Some(Press::Decide),
        _ => None,
    }
}

/// `running`: a limit is on; the tour presses its commands. `seek`: the
/// limit offers a row the cursor is not on; the tour moves down instead of
/// deciding.
pub fn press(tick: u64, running: bool, seek: bool) -> Buttons {
    let mash = running && mash(tick);
    let mut b = Buttons {
        decide: mash && !seek,
        down: mash && seek,
        ..Buttons::default()
    };
    match schedule(tick) {
        Some(Press::Down) => b.down = true,
        Some(Press::Decide) => b.decide = true,
        Some(Press::Left) => b.left = true,
        None => {}
    }
    b
}

/// The tick a limit's MP is topped up (before its list opens).
pub fn refill(tick: u64) -> bool {
    tick + 10 >= FIRST && (tick + 10 - FIRST).is_multiple_of(STEP)
}

/// The lock-on button once, before the first limit (the commands aim at
/// Sora's lock-on target).
pub fn lock(tick: u64) -> bool {
    tick == FIRST - 60
}

/// The reaction button while a limit runs: every 12 ticks.
pub fn reaction(tick: u64) -> bool {
    tick.is_multiple_of(12)
}

/// The decide button on the limit's command (the Attack row it took):
/// every 8 ticks once the limit has started.
pub fn mash(tick: u64) -> bool {
    tick.checked_sub(FIRST).is_some_and(|t| t % STEP > 130) && tick.is_multiple_of(8)
}

pub fn captures() -> Vec<u64> {
    let mut c = vec![FIRST - 10];
    for k in 0..LIMITS {
        let s = FIRST + k * STEP;
        c.extend([s + 60, s + 135, s + 200, s + 320, s + 480, s + 700]);
    }
    c
}

pub fn end() -> u64 {
    FIRST + LIMITS * STEP + 20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_limit_gets_its_entry() {
        // The second limit: three downs to the row, open, one down, decide.
        let s = FIRST + STEP;
        assert!(press(s, false, false).down && press(s + 20, false, false).down);
        assert!(press(s + 40, false, false).decide);
        assert!(press(s + 60, false, false).down && !press(s + 70, false, false).down);
        assert!(press(s + 120, false, false).decide);
        // While a limit runs: its command, or down to its offered row.
        assert!(press(s + 140, true, false).decide && !press(s + 140, false, false).decide);
        assert!(press(s + 140, true, true).down && !press(s + 140, true, true).decide);
        assert!(lock(FIRST - 60) && !lock(FIRST));
        assert!(refill(s - 10) && !refill(s));
        assert!(captures().iter().all(|&c| c < end()));
    }
}
