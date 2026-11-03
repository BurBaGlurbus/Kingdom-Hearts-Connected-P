//! The duel's match rules. The host chooses them in the lobby (and with
//! `--duel-damage`, `--duel-magic-damage`, `--duel-hit-cap` / F3); the
//! joiner receives them and shows them read-only.
//! The modes are a table, so a new one is a row and its match flow.

/// How a match is won.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The first knock-out wins.
    Stock,
    /// Rounds, each to a knock-out; the first to win two takes the match.
    BestOf3,
    /// A knock-out wins; when the time runs out, the larger share of HP
    /// left wins (equal shares draw).
    Timed,
}

/// One row of the mode table.
pub struct ModeInfo {
    pub mode: Mode,
    /// The lobby's name for it.
    pub name: &'static str,
    /// Whether the match runs against the time limit.
    pub timed: bool,
}

/// The modes, in the order the host's lobby cycles them.
pub const MODES: [ModeInfo; 3] = [
    ModeInfo {
        mode: Mode::Stock,
        name: "One KO",
        timed: false,
    },
    ModeInfo {
        mode: Mode::BestOf3,
        name: "Best of 3",
        timed: false,
    },
    ModeInfo {
        mode: Mode::Timed,
        name: "Time limit",
        timed: true,
    },
];

/// A mode by its command-line name (`--duel-mode`).
pub fn mode_named(name: &str) -> Option<Mode> {
    match name {
        "stock" => Some(Mode::Stock),
        "best3" => Some(Mode::BestOf3),
        "time" => Some(Mode::Timed),
        _ => None,
    }
}

/// Which game's moves the Soras fight with [duel choice, T279]: KH2's own
/// controller, KH3's Sora, or each player's own pick of the two. The host
/// picks this policy; under `Each` (Free pick) each player picks on their
/// own Ready page [duel choice, KHC-016].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ruleset {
    #[default]
    Kh2,
    Kh3,
    Each,
}

/// The rulesets in the lobby's order: (ruleset, lobby name, command-line
/// name).
pub const RULESETS: [(Ruleset, &str, &str); 3] = [
    (Ruleset::Kh2, "KH2", "kh2"),
    (Ruleset::Kh3, "KH3", "kh3"),
    (Ruleset::Each, "Free pick", "each"),
];

/// One ruleset's card (the lobby's Rulesets page): a new ruleset adds a row
/// [KHC-016]. Short words: the lobby draws them in the game's own menu.
pub struct RulesetCard {
    /// `RULESETS`' lobby name.
    pub name: &'static str,
    /// The game its moves come from.
    pub game: &'static str,
    /// What a Sora on it spends and fills.
    pub resources: &'static str,
    /// A few of its signature moves.
    pub moves: &'static str,
    /// The bench matrix's numbers (`--bench-matrix`, KHC-015), once they are
    /// worth showing. None until a bench is not limited by its bots.
    pub bench: Option<&'static str>,
}

/// The cards, one a ruleset a Sora can fight on (`Each` is a policy).
pub const CARDS: [RulesetCard; 2] = [
    RulesetCard {
        name: "KH2",
        game: "KINGDOM HEARTS II FINAL MIX",
        resources: "HP, MP, Drive gauge",
        moves: "Combos, Drive Forms, Reflect",
        bench: None,
    },
    RulesetCard {
        name: "KH3",
        game: "KINGDOM HEARTS III",
        resources: "HP, MP, Focus gauge",
        moves: "Flowmotion, Shotlock, Grand Magic",
        bench: None,
    },
];

/// A ruleset by its command-line name (`--duel-ruleset`).
pub fn ruleset_named(name: &str) -> Option<Ruleset> {
    RULESETS.iter().find(|r| r.2 == name).map(|r| r.0)
}

/// The time limits the host can pick, in seconds.
pub const TIME_LIMITS: [u16; 5] = [60, 90, 120, 180, 300];
const DEFAULT_TIME: u16 = 180;

/// The default damage rule [duel choice]. Measured with the scripted rival
/// swinging the Kingdom Key at an idle Sora at level 99 (KHC_RIVAL_BENCH,
/// T241): 0.46 hits landed a second at 2 damage, so 170 HP lasts about
/// 185 s at x1. x3 makes that about a minute, and 30-45 s for players who
/// land more of their combos.
pub const DEFAULT_DAMAGE: f32 = 3.;
/// The default magic damage rule [duel choice; T245's benchmark]: hits
/// whose power is the attacker's Magic (spells, Reflect's counter, form
/// magic) take this instead of the attacks' multiplier.
pub const DEFAULT_MAGIC: f32 = 1.5;
/// The lobby's and F3 menu's step and the range they allow (both
/// multipliers).
pub const DAMAGE_STEP: f32 = 0.5;
pub const DAMAGE_RANGE: (f32, f32) = (0.5, 10.);
/// The per-hit caps the host can pick, in percent of the target's max HP
/// (100: no cap).
pub const HIT_CAPS: [u8; 9] = [20, 25, 30, 35, 40, 50, 60, 75, 100];
const DEFAULT_HIT_CAP: u8 = 40;

/// The duel's rules (the host's; the joiner shows them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchRules {
    pub mode: Mode,
    /// What a Sora-against-Sora hit's damage is multiplied by, after the
    /// game's own formula: hits on Strength (attacks) ...
    pub damage: f32,
    /// ... and hits on Magic (spells, Reflect's counter, form magic).
    pub magic: f32,
    /// No single hit takes more than this percent of the target's max HP
    /// (100: no cap).
    pub hit_cap: u8,
    /// Seconds, for the timed mode.
    pub time_limit: u16,
    /// The arena (a row of `duel_arena::ARENAS`).
    pub arena: u8,
    /// KH2's moves, KH3's, or each player's own.
    pub ruleset: Ruleset,
    /// Under `Ruleset::Each`: who picked KH3, by seat (0 the host).
    pub picks: [bool; 2],
    /// The host's buttons wait as long as the joiner's take to arrive
    /// (duel_delay.rs).
    pub latency_match: bool,
    /// Who wins when a rewound swing of the joiner's meets a dodge the host
    /// began since (duel_rewind.rs).
    pub dodge: super::duel_rewind::DodgePolicy,
    /// [KHC-015] Parity: each ruleset's hits take its scale from `PARITY` under the multipliers; off (Authentic),
    /// every scale is 1.
    pub parity: bool,
}

/// [duel choice, KHC-015] Parity's scale per ruleset, on the damage a Sora on it deals: (lobby name, scale). Set
/// from the bench matrix (`--bench-matrix`); see the record for the numbers they came from.
pub const PARITY: [(&str, f32); 2] = [("KH2", 1.0), ("KH3", 1.0)];

/// Today's defaults: the lobby's Standard preset.
pub const STANDARD: MatchRules = MatchRules {
    mode: Mode::Stock,
    damage: DEFAULT_DAMAGE,
    magic: DEFAULT_MAGIC,
    hit_cap: DEFAULT_HIT_CAP,
    time_limit: DEFAULT_TIME,
    arena: 0,
    ruleset: Ruleset::Kh2,
    picks: [false; 2],
    // [Owner's call, default per the brief until answered: KHC-032.]
    latency_match: true,
    // [Owner's call, default per the brief until answered: KHC-025.]
    dodge: super::duel_rewind::DodgePolicy::AttackersView,
    parity: false,
};

impl Default for MatchRules {
    fn default() -> Self {
        STANDARD
    }
}

/// One row of the preset table: a name and its rules.
pub struct Preset {
    pub name: &'static str,
    pub rules: MatchRules,
}

/// The host's presets, in the lobby's order: a new one is a row [duel choice,
/// KHC-016; the owner's list per the brief until answered]. Each is plain
/// rules, Authentic (every per-ruleset scale 1). Choosing one sets how the
/// match is won and hit; the arena, the ruleset and the netcode rows stay.
pub const PRESETS: [Preset; 4] = [
    Preset {
        name: "Standard",
        rules: STANDARD,
    },
    // [Duel choice, KHC-016] The brief names it only: one knock-out at a
    // harder hit, about half Standard's time to a KO.
    Preset {
        name: "Quick KO",
        rules: MatchRules {
            damage: 5.,
            magic: 2.5,
            hit_cap: 50,
            ..STANDARD
        },
    },
    Preset {
        name: "Long fight",
        rules: MatchRules {
            mode: Mode::BestOf3,
            damage: 2.,
            hit_cap: 30,
            ..STANDARD
        },
    },
    Preset {
        name: "Timed 90",
        rules: MatchRules {
            mode: Mode::Timed,
            time_limit: 90,
            ..STANDARD
        },
    },
];

impl MatchRules {
    /// A hit's damage under the rules: the formula's number times the
    /// attacks' or the magic multiplier (`magic`: the hit's power is the
    /// attacker's Magic), rounded, then no more than the cap's share of
    /// `max_hp`; a hit that did damage still does at least 1.
    pub fn damage(&self, formula: i32, magic: bool, max_hp: i32) -> i32 {
        if formula <= 0 {
            return formula;
        }
        let k = if magic { self.magic } else { self.damage };
        let d = ((formula as f32 * k).round() as i32).max(1);
        self.capped(d, max_hp)
    }

    /// No more than the cap's share of `max_hp` (at least 1; 100: no cap).
    pub fn capped(&self, damage: i32, max_hp: i32) -> i32 {
        if self.hit_cap >= 100 {
            return damage;
        }
        damage.min((max_hp * i32::from(self.hit_cap) / 100).max(1))
    }

    /// Whether the cap took something off a hit of `uncapped`.
    pub fn cap_applied(&self, uncapped: i32, max_hp: i32) -> bool {
        self.capped(uncapped, max_hp) < uncapped
    }

    /// The host's multiplier as a ratio to its default: what a hit whose number is already a duel's (KH3's own
    /// damage, which stands beside the defaults) is scaled by.
    pub fn ratio_to_default(&self, magic: bool) -> f32 {
        if magic {
            self.magic / DEFAULT_MAGIC
        } else {
            self.damage / DEFAULT_DAMAGE
        }
    }

    /// [duel choice, KHC-013] A KH3 hit on a KH3 Sora (`raw`, KH3's own damage on a gauge of `max_hp`) under the
    /// host's rules, as its hit on a KH2 Sora is: the multipliers count from their defaults, and the hit stops at the
    /// cap. At the default rules only the cap can change it.
    pub fn kh3_damage(&self, raw: f32, magic: bool, max_hp: i32) -> f32 {
        if raw <= 0. {
            return raw;
        }
        let d = raw * self.ratio_to_default(magic);
        if self.hit_cap >= 100 {
            return d;
        }
        d.min((max_hp * i32::from(self.hit_cap) / 100).max(1) as f32)
    }

    /// [KHC-015] The scale on the damage a Sora on KH3's moves (or KH2's) deals: Parity's, else 1.
    pub fn scale(&self, kh3: bool) -> f32 {
        if self.parity {
            PARITY[usize::from(kh3)].1
        } else {
            1.
        }
    }

    /// The preset these rules are, if any (only what a preset sets counts).
    pub fn preset(&self) -> Option<usize> {
        PRESETS
            .iter()
            .position(|p| self.with_preset(&p.rules) == *self)
    }

    /// These rules with a preset's: the mode, damage, cap and time limit;
    /// the arena, the ruleset and the netcode rows stay.
    pub fn with_preset(&self, preset: &MatchRules) -> MatchRules {
        MatchRules {
            mode: preset.mode,
            damage: preset.damage,
            magic: preset.magic,
            hit_cap: preset.hit_cap,
            time_limit: preset.time_limit,
            parity: preset.parity,
            ..*self
        }
    }

    /// The next (or previous) preset; from custom rules, the first or last.
    pub fn cycle_preset(&mut self, step: i32) {
        let n = PRESETS.len() as i32;
        let next = match self.preset() {
            Some(at) => (at as i32 + step).rem_euclid(n),
            None if step > 0 => 0,
            None => n - 1,
        };
        *self = self.with_preset(&PRESETS[next as usize].rules);
    }

    /// The mode's row.
    pub fn info(&self) -> &'static ModeInfo {
        MODES
            .iter()
            .find(|m| m.mode == self.mode)
            .unwrap_or(&MODES[0])
    }

    /// The next (or previous) mode in the table.
    pub fn cycle_mode(&mut self, step: i32) {
        let at = MODES.iter().position(|m| m.mode == self.mode).unwrap_or(0) as i32;
        let n = MODES.len() as i32;
        self.mode = MODES[(at + step).rem_euclid(n) as usize].mode;
    }

    /// The damage multiplier one step up or down, kept in range.
    pub fn step_damage(&mut self, step: i32) {
        self.damage =
            (self.damage + DAMAGE_STEP * step as f32).clamp(DAMAGE_RANGE.0, DAMAGE_RANGE.1);
    }

    /// The magic multiplier one step up or down, kept in range.
    pub fn step_magic(&mut self, step: i32) {
        self.magic = (self.magic + DAMAGE_STEP * step as f32).clamp(DAMAGE_RANGE.0, DAMAGE_RANGE.1);
    }

    /// The next (or previous) hit cap in the table.
    pub fn cycle_cap(&mut self, step: i32) {
        let at = HIT_CAPS
            .iter()
            .position(|&c| c == self.hit_cap)
            .unwrap_or(4) as i32;
        let n = HIT_CAPS.len() as i32;
        self.hit_cap = HIT_CAPS[(at + step).rem_euclid(n) as usize];
    }

    /// The next (or previous) arena.
    pub fn cycle_arena(&mut self, step: i32) {
        let n = super::duel_arena::ARENAS.len() as i32;
        self.arena = (i32::from(self.arena) + step).rem_euclid(n) as u8;
    }

    /// The next (or previous) ruleset.
    pub fn cycle_ruleset(&mut self, step: i32) {
        let at = self.ruleset_code() as i32;
        let n = RULESETS.len() as i32;
        self.ruleset = RULESETS[(at + step).rem_euclid(n) as usize].0;
    }

    /// The ruleset's wire code and back.
    pub fn ruleset_code(&self) -> u8 {
        RULESETS
            .iter()
            .position(|r| r.0 == self.ruleset)
            .unwrap_or(0) as u8
    }

    pub fn ruleset_from(code: u8) -> Option<Ruleset> {
        RULESETS.get(code as usize).map(|r| r.0)
    }

    /// The ruleset's lobby name.
    pub fn ruleset_name(&self) -> &'static str {
        RULESETS[self.ruleset_code() as usize].1
    }

    /// The Sora on this seat fights with KH3's moves.
    pub fn seat_kh3(&self, seat: u8) -> bool {
        match self.ruleset {
            Ruleset::Kh2 => false,
            Ruleset::Kh3 => true,
            Ruleset::Each => self.picks.get(usize::from(seat)) == Some(&true),
        }
    }

    /// Some Sora fights with KH3's moves: KH3's files are needed now.
    pub fn kh3(&self) -> bool {
        self.seat_kh3(0) || self.seat_kh3(1)
    }

    /// Every Sora does.
    pub fn all_kh3(&self) -> bool {
        self.seat_kh3(0) && self.seat_kh3(1)
    }

    /// The rules may put a Sora on KH3's moves: every player needs its files.
    pub fn needs_kh3(&self) -> bool {
        self.ruleset != Ruleset::Kh2
    }

    /// A KH3 hit's share of max HP on a KH2 Sora under the rules [duel choice]: the host's multipliers
    /// count from their defaults, the numbers KH3's own damage stands beside, and the hit stops at the cap.
    pub fn cross_damage(&self, share: f32, magic: bool, max_hp: i32) -> i32 {
        let d = ((share * self.ratio_to_default(magic) * max_hp as f32).round() as i32).max(1);
        self.capped(d, max_hp)
    }

    /// The next (or previous) time limit in the table.
    pub fn cycle_time(&mut self, step: i32) {
        let at = TIME_LIMITS
            .iter()
            .position(|&t| t == self.time_limit)
            .unwrap_or(3) as i32;
        let n = TIME_LIMITS.len() as i32;
        self.time_limit = TIME_LIMITS[(at + step).rem_euclid(n) as usize];
    }

    /// Whether every field is one the lobby can choose (the joiner's check
    /// of what arrives).
    pub fn valid(&self) -> bool {
        let in_range = |k: f32| k.is_finite() && (DAMAGE_RANGE.0..=DAMAGE_RANGE.1).contains(&k);
        in_range(self.damage)
            && in_range(self.magic)
            && HIT_CAPS.contains(&self.hit_cap)
            && usize::from(self.arena) < super::duel_arena::ARENAS.len()
            && TIME_LIMITS.contains(&self.time_limit)
    }

    /// The mode's wire code and back.
    pub fn mode_code(&self) -> u8 {
        MODES.iter().position(|m| m.mode == self.mode).unwrap_or(0) as u8
    }

    pub fn mode_from(code: u8) -> Option<Mode> {
        MODES.get(code as usize).map(|m| m.mode)
    }
}

/// A hit's damage as a share of the gauge it was reckoned against: how a hit crosses from one ruleset's Sora to the
/// other's [duel choice, KHC-010].
pub fn share_of(damage: i32, reference_max: i32) -> f32 {
    damage.max(0) as f32 / reference_max.max(1) as f32
}

/// `m:ss` for a count of ticks (60 a second), rounded up to the second.
pub fn clock(ticks: u32) -> String {
    let s = ticks.div_ceil(60);
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_damage_rule_scales_and_rounds() {
        let rules = |damage| MatchRules {
            damage,
            hit_cap: 100,
            ..MatchRules::default()
        };
        assert_eq!(rules(2.5).damage(2, false, 170), 5);
        assert_eq!(rules(2.5).damage(3, false, 170), 8);
        assert_eq!(rules(2.5).damage(0, false, 170), 0);
        assert_eq!(
            rules(0.5).damage(1, false, 170),
            1,
            "a hit that did damage still does some"
        );
        assert_eq!(rules(1.).damage(7, false, 170), 7);
    }

    #[test]
    fn magic_has_its_own_multiplier_and_every_hit_its_cap() {
        let r = MatchRules {
            damage: 3.,
            magic: 1.5,
            hit_cap: 40,
            ..MatchRules::default()
        };
        assert_eq!(r.damage(8, false, 170), 24);
        assert_eq!(r.damage(8, true, 170), 12);
        // 40 % of 170 is 68: a Thundaga bolt of 30 on a high-Magic Keyblade
        // at x3 (90) stops there.
        assert_eq!(r.damage(30, true, 170), 45);
        assert_eq!(MatchRules { magic: 3., ..r }.damage(30, true, 170), 68);
        assert_eq!(
            MatchRules {
                hit_cap: 100,
                magic: 3.,
                ..r
            }
            .damage(30, true, 170),
            90
        );
    }

    #[test]
    fn the_host_cycles_modes_damage_and_time_within_their_tables() {
        let mut r = MatchRules::default();
        assert_eq!(r.info().name, "One KO");
        r.cycle_mode(1);
        assert_eq!(r.mode, Mode::BestOf3);
        r.cycle_mode(-2);
        assert_eq!(r.mode, Mode::Timed);
        r.damage = DAMAGE_RANGE.1;
        r.step_damage(1);
        assert_eq!(r.damage, DAMAGE_RANGE.1);
        r.step_damage(-1);
        assert_eq!(r.damage, DAMAGE_RANGE.1 - DAMAGE_STEP);
        r.step_magic(-100);
        assert_eq!(r.magic, DAMAGE_RANGE.0);
        r.cycle_cap(-5);
        assert_eq!(r.hit_cap, 100);
        r.cycle_cap(1);
        assert_eq!(r.hit_cap, 20);
        assert!(!MatchRules { hit_cap: 41, ..r }.valid());
        r.cycle_time(1);
        assert_eq!(r.time_limit, 300);
        r.cycle_time(1);
        assert_eq!(r.time_limit, 60);
        assert!(r.valid());
        assert_eq!(MatchRules::mode_from(r.mode_code()), Some(r.mode));
        assert!(!MatchRules { time_limit: 7, ..r }.valid());
        assert_eq!(mode_named("best3"), Some(Mode::BestOf3));
        assert!(!r.kh3());
        r.cycle_ruleset(1);
        assert!(r.kh3() && r.ruleset_name() == "KH3");
        assert_eq!(
            MatchRules::ruleset_from(r.ruleset_code()),
            Some(Ruleset::Kh3)
        );
        r.cycle_ruleset(1);
        assert_eq!(r.ruleset, Ruleset::Each);
        assert_eq!(r.ruleset_name(), "Free pick");
        r.cycle_ruleset(1);
        assert_eq!(r.ruleset, Ruleset::Kh2);
        assert_eq!(ruleset_named("kh3"), Some(Ruleset::Kh3));
        assert_eq!(ruleset_named("each"), Some(Ruleset::Each));
        assert_eq!(ruleset_named("kh1"), None);
        assert_eq!(mode_named("rounds"), None);
    }

    #[test]
    fn each_seat_has_its_own_ruleset_only_when_the_host_allows_it() {
        let mut r = MatchRules {
            picks: [false, true],
            ..MatchRules::default()
        };
        assert!(
            !r.kh3() && !r.needs_kh3(),
            "KH2 for both, whatever was picked"
        );
        r.ruleset = Ruleset::Kh3;
        assert!(r.seat_kh3(0) && r.all_kh3(), "KH3 for both");
        r.ruleset = Ruleset::Each;
        assert!(!r.seat_kh3(0) && r.seat_kh3(1));
        assert!(r.kh3() && r.needs_kh3() && !r.all_kh3());
        assert!(!r.seat_kh3(7), "no such seat");
        r.picks = [false; 2];
        assert!(
            !r.kh3() && r.needs_kh3(),
            "nobody picked it, but anyone may"
        );
    }

    #[test]
    fn a_kh3_hit_on_a_kh2_sora_takes_its_share_of_the_gauge() {
        let r = MatchRules {
            hit_cap: 40,
            ..MatchRules::default()
        };
        assert_eq!(r.cross_damage(0.1, false, 170), 17);
        assert_eq!(r.cross_damage(0.1, true, 170), 17);
        assert_eq!(r.cross_damage(0.9, false, 170), 68, "the cap");
        assert_eq!(r.cross_damage(0.0001, false, 170), 1, "at least 1");
        let doubled = MatchRules {
            damage: DEFAULT_DAMAGE * 2.,
            ..r
        };
        assert_eq!(doubled.cross_damage(0.1, false, 170), 34);
        assert_eq!(doubled.cross_damage(0.1, true, 170), 17, "magic's own");
    }

    /// KHC-013's matrix in numbers: what one hit takes off each victim's gauge, by attacker and victim ruleset.
    /// The routes are duel_hits' (KH2 attacker) and kh3_hits' (KH3 attacker); 170 is a duel build's KH2 max HP, 265
    /// a KH3 Sora's.
    #[test]
    fn every_pairing_takes_the_same_share_and_stops_at_the_cap() {
        const KH2_MAX: i32 = 170;
        const KH3_MAX: i32 = 265;
        let r = MatchRules::default();
        // (attacker's own number, magic) -> share of the victim's gauge, for each pairing.
        let kh2_on_kh2 = |formula, magic| share_of(r.damage(formula, magic, KH2_MAX), KH2_MAX);
        // A KH3 Sora's share is reckoned against the attacker's KH2 gauge (duel_hits' rules_max).
        let kh2_on_kh3 =
            |formula, magic| share_of(r.damage(formula, magic, KH2_MAX), KH2_MAX) * KH3_MAX as f32;
        let kh3_on_kh2 =
            |raw: f32, magic| r.cross_damage(raw / KH3_MAX as f32, magic, KH2_MAX) as f32;
        let kh3_on_kh3 = |raw: f32, magic| r.kh3_damage(raw, magic, KH3_MAX);
        let cases: [(&str, i32, bool); 5] = [
            ("combo hit", 2, false),
            ("finisher", 8, false),
            ("spell", 10, true),
            ("Reflect counter", 6, true),
            ("big spell, capped", 60, true),
        ];
        for (name, formula, magic) in cases {
            let share = kh2_on_kh2(formula, magic);
            assert!(share <= 0.4, "{name}: the cap");
            let on_kh3 = kh2_on_kh3(formula, magic) / KH3_MAX as f32;
            assert!((on_kh3 - share).abs() < 1e-6, "{name}: the same share");
        }
        // KH3's own damage: the share it took of KH3's gauge, on either gauge.
        for (name, raw, magic) in [
            ("combo hit", 9., false),
            ("flow attack", 21., false),
            ("grand magic", 48., true),
            ("shotlock bolt", 4., true),
            ("Ragnarok, capped", 150., false),
        ] {
            let on_kh3 = kh3_on_kh3(raw, magic) / KH3_MAX as f32;
            let on_kh2 = kh3_on_kh2(raw, magic) / KH2_MAX as f32;
            assert!(on_kh3 <= 0.4 && on_kh2 <= 0.4 + 1e-6, "{name}: the cap");
            // KH2's gauge is whole HP: within half a point of it.
            assert!(
                (on_kh3 - on_kh2).abs() <= 0.5 / KH2_MAX as f32 + 1e-6,
                "{name}: {on_kh3} against {on_kh2}"
            );
        }
        assert_eq!(kh3_on_kh3(150., false), 106., "40 % of 265");
        assert_eq!(kh3_on_kh3(21., false), 21., "unchanged at the defaults");
    }

    #[test]
    fn the_host_multipliers_reach_kh3_against_kh3_from_their_defaults() {
        let r = MatchRules {
            damage: DEFAULT_DAMAGE * 2.,
            magic: DEFAULT_MAGIC / 2.,
            hit_cap: 100,
            ..MatchRules::default()
        };
        assert_eq!(r.kh3_damage(10., false, 265), 20.);
        assert_eq!(r.kh3_damage(10., true, 265), 5.);
        assert_eq!(r.kh3_damage(0., false, 265), 0., "no damage stays none");
        assert!(MatchRules::default().capped(80, 170) == 68);
        assert_eq!(MatchRules::default().capped(80, 0), 1, "at least 1");
        assert_eq!(share_of(17, 170), 0.1);
        assert_eq!(share_of(-3, 170), 0.);
        assert_eq!(share_of(5, 0), 5.);
    }

    /// KHC-015: Authentic (Parity off) leaves every hit as it was.
    #[test]
    fn a_scale_of_one_leaves_damage_unchanged() {
        let r = MatchRules::default();
        assert_eq!(r.scale(false), 1.);
        assert_eq!(r.scale(true), 1.);
        for formula in [0, 1, 2, 7, 33, 120, 4000] {
            let scaled = (formula as f32 * r.scale(true)).round() as i32;
            assert_eq!(r.damage(scaled, false, 170), r.damage(formula, false, 170));
        }
        let parity = MatchRules { parity: true, ..r };
        assert_eq!(parity.scale(false), PARITY[0].1);
        assert_eq!(parity.scale(true), PARITY[1].1);
    }

    /// KHC-016: the presets are valid rules, Authentic, and only reach the match's own rows.
    #[test]
    fn presets_are_rows_and_keep_the_arena_and_ruleset() {
        for p in &PRESETS {
            assert!(p.rules.valid(), "{}", p.name);
            assert!(!p.rules.parity, "{}: Authentic", p.name);
        }
        let mut r = MatchRules {
            arena: 1,
            ruleset: Ruleset::Each,
            picks: [true, false],
            ..MatchRules::default()
        };
        assert_eq!(r.preset(), Some(0), "Standard is today's defaults");
        r.cycle_preset(2);
        assert_eq!(PRESETS[r.preset().unwrap()].name, "Long fight");
        assert_eq!((r.mode, r.damage, r.hit_cap), (Mode::BestOf3, 2., 30));
        assert_eq!(
            (r.arena, r.ruleset, r.picks),
            (1, Ruleset::Each, [true, false])
        );
        r.step_damage(1);
        assert_eq!(r.preset(), None, "custom");
        r.cycle_preset(-1);
        assert_eq!(PRESETS[r.preset().unwrap()].name, "Timed 90");
        assert_eq!(r.time_limit, 90);
        assert_eq!(
            CARDS.len(),
            RULESETS.len() - 1,
            "a card a ruleset, Each is a policy"
        );
        for (card, row) in CARDS.iter().zip(RULESETS) {
            assert_eq!(card.name, row.1);
        }
    }

    #[test]
    fn clocks_round_up_to_the_second() {
        assert_eq!(clock(180 * 60), "3:00");
        assert_eq!(clock(61), "0:02");
        assert_eq!(clock(0), "0:00");
    }
}
