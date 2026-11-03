//! The unattended `--summon-tour <dir>` (in the magic tour's setup: Sora
//! stands still, the Shadow is frozen nearby). Each summon of `sumn` in
//! turn is asked for as the Summon list's pick would, with a full Drive
//! gauge [tour setup]. Sora attacks (Attack, the top row) so that the
//! summons that answer Sora's attacks (Peter Pan, Genie) have something to
//! answer; Genie's first form is picked from the second deck's row 2; then
//! the second deck's Limit row, which the summon's script offers its limit
//! on, starts the summon's limit and the reaction button takes its
//! follow-ups. The first three summons are dismissed, the last runs out of
//! time. The tour owns the four charms.
use visual::field_command_menu::Buttons;

/// The first summon's tick and the ticks between summons.
const FIRST: u64 = 200;
const STEP: u64 = 1600;
/// Ticks after a call: the second deck, the walk down to its Limit row,
/// the limit's start, its follow-ups, the top deck back, and Dismiss.
const SHOW_DECK: u64 = 640;
const LIMIT_ROW: [u64; 3] = [652, 664, 676];
const LIMIT_DECIDE: u64 = 690;
/// Back up to the top row, whose command the limit's offers on record 0
/// take, before its follow-ups.
const TOP_ROW: [u64; 3] = [694, 698, 702];
const FOLLOW_UPS: std::ops::Range<u64> = 710..1000;
const FOLLOW_UP_EVERY: u64 = 15;
const HIDE_DECK: u64 = 1050;
const DISMISS: u64 = 1150;
/// The last summon's time (3 bars of 600) plus its leaving.
const LAST: u64 = 2600;
pub const SUMMONS: usize = 4;

/// The summon (row index) the tour asks for at `tick`.
pub fn request(tick: u64) -> Option<usize> {
    let k = tick.checked_sub(FIRST)?;
    (k.is_multiple_of(STEP) && k / STEP < SUMMONS as u64).then_some((k / STEP) as usize)
}

/// The summon of `tick` and the ticks since it was called.
fn phase(tick: u64) -> Option<(u64, u64)> {
    let k = tick.checked_sub(FIRST)?;
    let n = (k / STEP).min(SUMMONS as u64 - 1);
    Some((n, k - n * STEP))
}

/// The tick a summon is dismissed (the last one is not).
pub fn dismiss(tick: u64) -> bool {
    phase(tick).is_some_and(|(n, t)| t == DISMISS && n < SUMMONS as u64 - 1)
}

/// The follow-up taken as its deck row would be, on the reaction ticks:
/// a special deck's row `special` (Chicken Little's limit; the deck shows
/// it once Sora's opening motion ends, so a cursor walk would take the
/// deck's own row before that), else the record-0 offer while there is
/// one.
pub fn follow_up(tick: u64, offered: bool, special: Option<usize>) -> Option<usize> {
    // The record-0 offer on a cycle's first turn; a special deck's row on
    // its row turns.
    let k = turn(tick)?;
    if k >= 2 || k == 1 && special.is_none() {
        return None;
    }
    match special {
        Some(row) => Some(super::party_limit::SPECIAL + row),
        None => offered.then_some(0),
    }
}

/// Chicken Little's limit (the first summon): the lock-on button once,
/// after his deck is up.
const LOCK: u64 = 800;

pub fn lock(tick: u64) -> bool {
    phase(tick) == Some((0, LOCK))
}

/// The follow-up span runs in cycles of four turns of `FOLLOW_UP_EVERY`
/// ticks: the deck's rows on the first one or two (a row's command makes
/// the limit capture Sora anew), the reaction button on the last two. The
/// reaction commands chain to a state of the motion the limit captures Sora
/// in (range 26), which opens some way into it: Stitch's 600 at 30, Peter
/// Pan's 468 at 40 of capture 255, Genie's 535 at 96 of capture 253.
const TURNS: u64 = 4;

/// The turn (0..4) whose first tick `tick` is.
fn turn(tick: u64) -> Option<u64> {
    let (_, t) = phase(tick)?;
    let k = t.checked_sub(FOLLOW_UPS.start)?;
    (FOLLOW_UPS.contains(&t) && k.is_multiple_of(FOLLOW_UP_EVERY))
        .then_some(k / FOLLOW_UP_EVERY % TURNS)
}

/// Stitch's limit: the deck's two rows on the row turns (`None`: not a row
/// turn).
pub fn rotation(tick: u64) -> Option<usize> {
    turn(tick).filter(|&k| k < 2).map(|k| k as usize)
}

/// The reaction button every 3 ticks on the reaction turns, so a press
/// lands in the window the follow-up's state needs (FUN_1403ba070), as a
/// player's would.
pub fn mash(tick: u64) -> bool {
    phase(tick).is_some_and(|(_, t)| {
        FOLLOW_UPS.contains(&t)
            && (t - FOLLOW_UPS.start) / FOLLOW_UP_EVERY % TURNS >= 2
            && t.is_multiple_of(3)
    })
}

/// Ticks after a call when Sora attacks, every `ATTACK_EVERY` ticks: after
/// the summon has come and before the deck swap (the last summon, never
/// dismissed, again after its limit to near its end).
const ATTACKS: std::ops::Range<u64> = 400..600;
const LAST_ATTACKS: std::ops::Range<u64> = 1100..2400;
const ATTACK_EVERY: u64 = 20;

fn attack(tick: u64) -> bool {
    let Some((n, t)) = phase(tick) else {
        return false;
    };
    let last = n == SUMMONS as u64 - 1;
    t.is_multiple_of(ATTACK_EVERY) && (ATTACKS.contains(&t) || (last && LAST_ATTACKS.contains(&t)))
}

/// The second summon (Genie): after he has come, the second deck's row 2
/// (his forms) opens and its first form is picked; then the top deck
/// comes back.
const GENIE_STEPS: [(u64, u8); 6] = [
    (200, b'l'),
    (212, b'v'),
    (224, b'v'),
    (236, b'r'),
    (250, b'd'),
    (320, b'l'),
];

fn genie_step(tick: u64) -> Option<u8> {
    let (n, t) = phase(tick)?;
    if n != 1 {
        return None;
    }
    GENIE_STEPS.iter().find(|s| s.0 == t).map(|s| s.1)
}

/// The deck walk to the Limit row and back, Genie's form pick, Attack.
/// The follow-ups are taken by [`follow_up`], not by the cursor.
pub fn press(tick: u64) -> Buttons {
    let g = genie_step(tick);
    let t = phase(tick).map(|p| p.1);
    let at = |x: u64| t == Some(x);
    Buttons {
        left: at(SHOW_DECK) || at(HIDE_DECK) || g == Some(b'l'),
        up: TOP_ROW.iter().any(|&x| at(x)),
        down: LIMIT_ROW.iter().any(|&x| at(x)) || g == Some(b'v'),
        right: g == Some(b'r'),
        decide: attack(tick) || at(LIMIT_DECIDE) || g == Some(b'd'),
        ..Buttons::default()
    }
}

pub fn captures() -> Vec<u64> {
    let mut c = Vec::new();
    for k in 0..SUMMONS as u64 {
        let s = FIRST + k * STEP;
        c.extend([
            s + 20,
            s + 140,
            s + 400,
            s + LIMIT_DECIDE - 5,
            s + 760,
            s + 860,
            s + 1000,
            s + 1200,
        ]);
    }
    // Genie's form list open.
    c.push(FIRST + STEP + 245);
    let last = FIRST + (SUMMONS as u64 - 1) * STEP;
    c.extend([last + 1900, last + 2100]);
    c.sort_unstable();
    c.dedup();
    c
}

pub fn end() -> u64 {
    FIRST + (SUMMONS as u64 - 1) * STEP + LAST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_summon_is_asked_once_and_all_but_the_last_dismissed() {
        let asked: Vec<_> = (0..end()).filter_map(request).collect();
        assert_eq!(asked, vec![0, 1, 2, 3]);
        assert_eq!((0..end()).filter(|&t| dismiss(t)).count(), SUMMONS - 1);
        assert!(captures().iter().all(|&c| c < end()));
        assert!(press(FIRST + SHOW_DECK).left && !press(FIRST).left);
        // No Attack near a deck swap, the limit's start or a Dismiss.
        for k in 0..SUMMONS as u64 {
            let s = FIRST + k * STEP;
            let dismissed = k < SUMMONS as u64 - 1;
            for t in [SHOW_DECK, LIMIT_DECIDE, HIDE_DECK, DISMISS] {
                if t == DISMISS && !dismissed {
                    continue;
                }
                assert!((s + t - 30..s + t + 30).all(|x| !attack(x)), "{k} {t}");
            }
            assert!(press(s + LIMIT_DECIDE).decide && turn(s + 710) == Some(0));
            assert!(turn(s + 1000).is_none() && !mash(s + 710) && mash(s + 741));
            assert_eq!(follow_up(s + 725, true, None), None);
            assert_eq!(follow_up(s + 725, false, Some(1)), Some(101));
            assert!(press(s + 694).up && !press(s + 710).decide);
            assert_eq!(follow_up(s + 710, true, None), Some(0));
            assert_eq!(follow_up(s + 710, false, None), None);
            assert_eq!(follow_up(s + 710, false, Some(2)), Some(102));
            assert_eq!(follow_up(s + 711, true, Some(2)), None);
        }
        assert!(lock(FIRST + LOCK) && !lock(FIRST + STEP + LOCK));
        assert!(press(FIRST + 400).decide && press(FIRST + 3 * STEP + 2000).decide);
        // Genie's form pick: Left, Down, Down, Right, decide, Left.
        let g: Vec<_> = (FIRST + STEP..FIRST + STEP + 400)
            .filter(|&t| press(t) != Buttons::default())
            .collect();
        assert_eq!(g.len(), 6, "{g:?}");
    }
}
