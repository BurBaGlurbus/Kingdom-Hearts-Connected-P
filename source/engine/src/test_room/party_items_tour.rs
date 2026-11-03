//! The unattended `--party-items-tour <dir>` (in the magic tour's setup):
//! Sora's item slots hold a Mega-Potion, a Mega-Ether and a Megalixir
//! before each use [test-room values]. Before each use Donald and Goofy are worn down (HP
//! to a third, MP to none; Goofy knocked out before the Megalixir) [tour
//! setup], then Sora uses the item from his Items list.

/// Mega-Potion, Mega-Ether, Megalixir (03system `item`).
pub const SLOTS: [u16; 3] = [5, 6, 7];
const FIRST: u64 = 200;
const STEP: u64 = 200;
/// Ticks between wearing the members down and the use.
const LEAD: u64 = 20;

/// The slot Sora uses this tick.
pub fn use_slot(tick: u64) -> Option<usize> {
    let t = tick.checked_sub(FIRST)?;
    let k = usize::try_from(t / STEP).ok()?;
    (t.is_multiple_of(STEP) && k < SLOTS.len()).then_some(k)
}

/// The use the members are worn down for this tick.
pub fn wear(tick: u64) -> Option<usize> {
    use_slot(tick + LEAD)
}

pub fn captures() -> Vec<u64> {
    let mut c = Vec::new();
    for k in 0..SLOTS.len() as u64 {
        let s = FIRST + k * STEP;
        c.extend([s - 5, s + 45, s + 90]);
    }
    c
}

pub fn end() -> u64 {
    FIRST + SLOTS.len() as u64 * STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_item_once_after_the_members_are_worn_down() {
        assert_eq!(use_slot(FIRST), Some(0));
        assert_eq!(use_slot(FIRST + 2 * STEP), Some(2));
        assert_eq!(use_slot(FIRST + 3 * STEP), None);
        assert_eq!(use_slot(FIRST + 1), None);
        assert_eq!(wear(FIRST - LEAD), Some(0));
        assert!(captures().iter().all(|&c| c < end()));
    }
}
