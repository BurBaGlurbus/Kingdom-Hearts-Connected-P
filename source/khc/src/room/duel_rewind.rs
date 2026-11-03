//! Hit fairness for the joiner's attacks (KHC-025): the joiner sees the host's
//! Sora in the past, so its swings are judged against him as he was on the
//! joiner's screen, within a cap. The host keeps a short history of each Sora,
//! and the joiner's buttons say which snapshot was on screen when they were
//! made. Melee only: spells and shotlock bolts already fly on the host.

use std::collections::VecDeque;

/// Ticks of each Sora kept: 200 ms.
pub const HISTORY_TICKS: usize = 12;
/// The furthest back a swing looks: 100 ms [duel choice, KHC-025].
pub const REWIND_CAP: u64 = 6;

/// When a rewound swing meets a defender who has since begun to dodge, who
/// wins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DodgePolicy {
    /// The defender as he was at the rewound tick decides.
    #[default]
    AttackersView,
    /// It also misses if he cannot be hit now.
    DefenderWins,
}

/// The policies in the lobby's order: (policy, lobby name, command-line name).
pub const DODGE_POLICIES: [(DodgePolicy, &str, &str); 2] = [
    (DodgePolicy::AttackersView, "Attacker's view", "attacker"),
    (DodgePolicy::DefenderWins, "Defender wins", "defender"),
];

impl DodgePolicy {
    /// The hit misses: he could not be hit then, or now.
    pub fn misses(self, then: bool, now: bool) -> bool {
        match self {
            DodgePolicy::AttackersView => then,
            DodgePolicy::DefenderWins => then || now,
        }
    }

    pub fn code(self) -> u8 {
        DODGE_POLICIES.iter().position(|p| p.0 == self).unwrap_or(0) as u8
    }

    pub fn from_code(code: u8) -> Option<DodgePolicy> {
        DODGE_POLICIES.get(usize::from(code)).map(|p| p.0)
    }

    pub fn named(name: &str) -> Option<DodgePolicy> {
        DODGE_POLICIES.iter().find(|p| p.2 == name).map(|p| p.0)
    }
}

/// The last few ticks of something, by tick.
#[derive(Clone, Debug)]
pub struct History<T> {
    ticks: VecDeque<(u64, T)>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        History {
            ticks: VecDeque::new(),
        }
    }
}

impl<T> History<T> {
    pub fn push(&mut self, tick: u64, value: T) {
        if self.ticks.len() >= HISTORY_TICKS {
            self.ticks.pop_front();
        }
        self.ticks.push_back((tick, value));
    }

    /// The entry of that tick; failing that the oldest one after it, so a tick
    /// the simulation skipped is read as the next it ran.
    pub fn at(&self, tick: u64) -> Option<(u64, &T)> {
        self.ticks
            .iter()
            .find(|(t, _)| *t >= tick)
            .map(|(t, v)| (*t, v))
    }
}

/// The tick a swing looks back to: what was on the joiner's screen, no further
/// than the cap, and never ahead of now.
pub fn rewound_tick(now: u64, shown: u64) -> u64 {
    shown.clamp(now.saturating_sub(REWIND_CAP), now)
}

/// What the host keeps for the joiner's swings, and the average for the overlay.
#[derive(Clone, Debug, Default)]
pub struct Rewind {
    /// [Test] `--no-rewind`: swings are judged against the present.
    pub off: bool,
    /// The snapshot on the joiner's screen when the buttons in use were made.
    pub shown: Option<u64>,
    /// The host's Sora's feet, for KH3's own hit test.
    pub host_at: History<[f32; 3]>,
    pub total: u64,
    pub hits: u64,
}

impl Rewind {
    pub fn note(&mut self, ticks: u64) {
        self.total += ticks;
        self.hits += 1;
    }

    pub fn average(&self) -> Option<f32> {
        (self.hits > 0).then(|| self.total as f32 / self.hits as f32)
    }

    /// The tick a joiner's swing looks back to now.
    pub fn target(&self, now: u64) -> Option<u64> {
        if self.off {
            return None;
        }
        self.shown.map(|shown| rewound_tick(now, shown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_history_keeps_its_last_ticks() {
        let mut h = History::default();
        for t in 0..30u64 {
            h.push(t, t * 2);
        }
        assert_eq!(h.at(29), Some((29, &58)));
        assert_eq!(h.at(18), Some((18, &36)), "the oldest of 12");
        assert_eq!(h.at(5), Some((18, &36)), "older than that reads the oldest");
        assert_eq!(h.at(30), None, "nothing yet");
    }

    #[test]
    fn a_skipped_tick_reads_the_next_one() {
        let mut h = History::default();
        h.push(10, "a");
        h.push(12, "b");
        assert_eq!(h.at(11), Some((12, &"b")));
    }

    #[test]
    fn the_lookup_is_capped_both_ways() {
        assert_eq!(rewound_tick(100, 97), 97);
        assert_eq!(rewound_tick(100, 94), 94, "just at the cap");
        assert_eq!(rewound_tick(100, 50), 94, "no further than the cap");
        assert_eq!(rewound_tick(100, 120), 100, "never ahead of now");
        assert_eq!(rewound_tick(3, 0), 0, "near the start");
    }

    #[test]
    fn the_rule_says_who_wins_a_dodge() {
        use DodgePolicy::*;
        assert!(
            !AttackersView.misses(false, true),
            "the joiner's view lands"
        );
        assert!(AttackersView.misses(true, false), "he had dodged by then");
        assert!(DefenderWins.misses(false, true), "invulnerable now misses");
        assert!(DefenderWins.misses(true, false));
        assert!(!DefenderWins.misses(false, false));
        for p in [AttackersView, DefenderWins] {
            assert_eq!(DodgePolicy::from_code(p.code()), Some(p));
        }
        assert_eq!(DodgePolicy::from_code(9), None);
        assert_eq!(DodgePolicy::named("defender"), Some(DefenderWins));
    }

    #[test]
    fn the_average_counts_the_hits() {
        let mut r = Rewind::default();
        assert_eq!(r.average(), None);
        r.note(4);
        r.note(6);
        assert_eq!(r.average(), Some(5.));
        assert_eq!(r.target(10), None, "no snapshot reported yet");
        r.shown = Some(2);
        assert_eq!(r.target(10), Some(4));
        r.off = true;
        assert_eq!(r.target(10), None);
    }
}
