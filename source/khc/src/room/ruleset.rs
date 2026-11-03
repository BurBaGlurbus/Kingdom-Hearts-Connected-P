//! [KHC-014] What differs between the rulesets a Sora can fight with, behind one seam: a seat's rule
//! (`SeatRule`), the weight a hit carries out (`Weight`), what the attacker's ruleset does to its damage, how the
//! victim's ruleset takes it (defensive state, guard push, reaction, KH3's own victim side), and a KH3 hit as the
//! KH2 side reads it (`CrossHit`). The hit code (duel_hits.rs) asks this module and branches on nothing else.
//!
//! An enum with a `match`, not trait objects: the room's hooks are plain function pointers because state kept
//! inside `Game` could not take `&mut Game` (extension.rs), and the same holds here. A new ruleset is a new variant.

use super::duel_rules::MatchRules;
use super::*;
use kh3_player as kh3;

/// The rules one Sora fights with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatRule {
    /// KH2's own controller.
    Kh2,
    /// KH3's moves layered on KH2's controller (`kh3_player` without its core: KH3's sheets were not installed).
    Kh3Layer,
    /// KH3's own Sora (`kh3_sora`): he decides his own guard, dodge and reactions.
    Kh3Core,
}

impl SeatRule {
    /// The rule of the Sora on `seat`: KH3's core when it runs him, else the seat's pick.
    pub fn of(rules: &MatchRules, seat: u8, f: &Fighter) -> SeatRule {
        if f.ext.fighter().kh3.core_on() {
            SeatRule::Kh3Core
        } else if rules.seat_kh3(seat) {
            SeatRule::Kh3Layer
        } else {
            SeatRule::Kh2
        }
    }

    /// The seat fights with KH3's moves.
    pub fn kh3(self) -> bool {
        self != SeatRule::Kh2
    }

    /// The ruleset's name in the logs.
    pub fn name(self) -> &'static str {
        if self.kh3() { "KH3" } else { "KH2" }
    }

    // Defensive state.

    /// Whether the room's hit test may miss him: KH3's core decides on his own side.
    pub fn room_judges(self) -> bool {
        self != SeatRule::Kh3Core
    }

    /// Whether his moves' own frames make him untouchable now (the invulnerable timer aside).
    pub fn untouchable(self, f: &Fighter, clips: &Clips) -> bool {
        match self {
            SeatRule::Kh2 => untouchable(&f.player, clips),
            SeatRule::Kh3Layer => f.ext.fighter().kh3.untouchable(&f.player, clips),
            SeatRule::Kh3Core => false,
        }
    }

    /// Whether he cannot be hit now, as the room judges it (the joiner's rewind keeps this per tick).
    pub fn cannot_be_hit(self, f: &Fighter, clips: &Clips) -> bool {
        self.room_judges() && (f.vitals.invulnerable > 0. || self.untouchable(f, clips))
    }

    // Incoming hit, the victim's ruleset.

    /// [KH3 ruleset] A guard took the hit: the push-back by its weight; a heavy one offers the Risk Dodge.
    pub fn guarded(self, f: &mut Fighter, weight: Option<Weight>, away: Vec3, attacker_at: Vec3) {
        if let (SeatRule::Kh3Layer, Some(w)) = (self, weight) {
            f.ext.fighter_mut().kh3.guarded(
                &mut f.player,
                w.strike,
                [away.x, away.z],
                attacker_at.to_array(),
                w.air,
            );
        }
    }

    /// [KH3 ruleset] A dodge met a hit: the counter window opens.
    pub fn dodged(self, f: &mut Fighter) {
        if self == SeatRule::Kh3Layer {
            f.ext.fighter_mut().kh3.dodged();
        }
    }

    /// A hit is about to land on him.
    pub fn hit_taken(self, f: &mut Fighter) {
        if self == SeatRule::Kh3Layer {
            f.ext.fighter_mut().kh3.hit_taken();
        }
    }

    /// [Each player's ruleset] KH3's Sora takes a KH2 hit on his own victim side, as its share of his gauge.
    #[allow(clippy::too_many_arguments)]
    pub fn take_share(
        self,
        f: &mut Fighter,
        share: f32,
        class: u8,
        magic: bool,
        away: Vec3,
        attacker_at: Vec3,
        attacker_yaw: f32,
        attacker_air: bool,
    ) {
        if self == SeatRule::Kh3Core {
            f.ext.fighter_mut().kh3.core_take_kh2_hit(
                share,
                class,
                magic,
                [away.x, away.z],
                attacker_at.to_array(),
                attacker_yaw,
                attacker_air,
            );
        }
    }

    /// [KH3 ruleset] The reference's reaction for the hit's weight on KH2's damage motions: whether a motion
    /// started, and whether he shrugged it off (flow states and a command's first tenth of a second shrug off light
    /// ones; the damage stays). None: his ruleset reacts as KH2 does.
    pub fn react(
        self,
        f: &mut Fighter,
        weight: Option<Weight>,
        away: Vec3,
        high: bool,
        clips: &Clips,
    ) -> Option<(bool, bool)> {
        let w = weight.filter(|_| self == SeatRule::Kh3Layer)?;
        let shrug = f.ext.fighter().kh3.shrugs_strike(&f.player, w.strike);
        // A body already in the air keeps its motion (no new voice).
        let started = !shrug
            && f.ext
                .fighter_mut()
                .kh3
                .hurt(&mut f.player, w.strike, [away.x, away.z], high, clips);
        Some((started, shrug))
    }

    // Outgoing hit, the attacker's ruleset.

    /// [KH3 ruleset] What the attacker's ruleset does to a hit's damage: a grand magic's spells hit twice as hard
    /// (`formula` again under the rules), a shotlock volley stops at its cap. `Err`: the volley's cap is spent.
    pub fn attack_damage(
        self,
        rules: &MatchRules,
        hit: AttackFacts,
        damage: (i32, i32),
    ) -> Result<(i32, i32), &'static str> {
        if !self.kh3() {
            return Ok(damage);
        }
        let (mut damage, mut uncapped) = damage;
        if hit.grand && hit.spell && !hit.bolt {
            let doubled = (hit.formula as f32 * kh3::tuning::GRAND_DAMAGE).round() as i32;
            damage = rules.damage(doubled, hit.magic, hit.reference);
            uncapped = MatchRules {
                hit_cap: 100,
                ..*rules
            }
            .damage(doubled, hit.magic, hit.reference);
        }
        if hit.bolt {
            let cap = hit.reference * kh3::tuning::VOLLEY_CAP_PERCENT / 100;
            damage = damage.min((cap - hit.volley_dealt).max(0));
            if damage == 0 {
                return Err("volley cap");
            }
        }
        Ok((damage, uncapped))
    }

    /// [KH2 ruleset] Whether a guarded swing's own row repels it (the KH3 reference repels only reflect level 3,
    /// which no Keyblade swing has).
    pub fn repelled_by_guard(self) -> bool {
        !self.kh3()
    }

    /// The attacker's own count after a hit landed (`taken` off the victim): a volley's share, a hit's focus and
    /// style, the weight it struck with.
    pub fn landed(
        self,
        a: &mut Fighter,
        bolt: bool,
        spell: bool,
        taken: u32,
        struck: Option<Weight>,
    ) {
        if !self.kh3() {
            return;
        }
        let k = &mut a.ext.fighter_mut().kh3;
        if bolt {
            k.volley_dealt += taken as i32;
        } else {
            k.landed(!spell);
        }
        if let Some(w) = struck {
            k.struck(w.source, w.strike);
        }
    }
}

/// What the attacker's ruleset reads of a hit for its damage.
#[derive(Clone, Copy, Debug)]
pub struct AttackFacts {
    /// The game's formula before the duel's multipliers.
    pub formula: i32,
    pub magic: bool,
    pub spell: bool,
    pub bolt: bool,
    /// The attacker is in a grand magic's window.
    pub grand: bool,
    /// What his volley has dealt so far.
    pub volley_dealt: i32,
    /// The max HP the rules reckon against.
    pub reference: i32,
}

/// [KH3 ruleset] The weight a hit carries (kh3_player::hurt): what it came from, its knock and scale, and whether
/// its attacker was in the air.
#[derive(Clone, Copy, Debug)]
pub struct Weight {
    pub source: kh3::Source,
    pub strike: kh3::hurt::Strike,
    pub air: bool,
}

impl Weight {
    /// A hit's weight: a bolt's, a spell's (heavy by its reaction class, double in a grand magic), or the swing's
    /// by the attacker's state.
    pub fn of(a: &Fighter, clips: &Clips, spell: Option<SpellFacts>) -> Weight {
        let source = match spell {
            Some(s) if s.bolt => kh3::Source::Bolt {
                ragnarok: s.ragnarok,
            },
            Some(s) => kh3::Source::Spell {
                heavy: s.heavy,
                grand: s.grand,
            },
            None => a.ext.fighter().kh3.source(&a.player, clips),
        };
        Weight {
            source,
            strike: kh3::hurt::strike(source),
            air: a.player.in_air(),
        }
    }

    /// A heavy knock, which has its own burst where it lands.
    pub fn heavy(&self) -> bool {
        self.strike.knock.heavy()
    }

    /// The `kh3_react` log's fields.
    pub fn names(&self) -> (&'static str, &'static str, f32) {
        (
            self.source.name(),
            self.strike.knock.name(),
            self.strike.scale,
        )
    }
}

/// What a spell hit's weight reads.
#[derive(Clone, Copy, Debug)]
pub struct SpellFacts {
    pub bolt: bool,
    pub ragnarok: bool,
    pub heavy: bool,
    pub grand: bool,
}

/// The attacker's KH3 state a hit reads: a grand magic's window, and what his volley has dealt.
pub fn kh3_attack_state(a: &Fighter) -> (bool, i32) {
    (super::duel_kh3::grand(a), a.ext.fighter().kh3.volley_dealt)
}

/// The Sora's turn for his hit spheres (KH3's lean included).
pub fn turn(f: &Fighter, yaw: f32) -> Quat {
    super::duel_kh3::turn(yaw, &f.ext.fighter().kh3)
}

/// His KH3 layer's mode, for the logs.
pub fn mode_name(f: &Fighter) -> &'static str {
    f.ext.fighter().kh3.mode.name()
}

/// A bolt's own impact spark is due on this Sora (once in a few ticks).
pub fn spark_ready(f: &mut Fighter) -> bool {
    f.ext.fighter_mut().kh3.spark_ready()
}

/// [KH3 ruleset] A heavy swing's own burst where it landed, sized by its weight.
pub fn heavy_hit_look(game: &mut Game, slot: u8, contact: Vec3, yaw: f32, weight: &Weight) {
    super::duel_kh3::show(
        game,
        kh3::look::Look::HeavyHit,
        slot,
        Some((contact.to_array(), yaw)),
        super::duel_kh3::hit_size(&weight.strike),
    );
}

/// Whether a Sora cannot be hurt by the other Sora now. [Duel rule] A Sora
/// takes the other's combo as an enemy takes Sora's: his flinches (hurt
/// motions 0-5, ground and air) have no invulnerable frames, so each step
/// that connects lands, the hit record and its combo group deciding as they
/// do against a target. A blow (a finisher's launch or a knock-down), its
/// landing, the recovery and the knock-out keep the frames the game's
/// motions give them, as do dodges and attacks with frames of their own,
/// so a launched Sora gets up before he can be hit again.
pub fn untouchable(p: &Player, clips: &Clips) -> bool {
    if p.invulnerable > 0. {
        return true;
    }
    if matches!(p.action, Action::Hurt(i) if i < player::hurt::BLOW[0]) {
        return false;
    }
    p.range_invulnerable(clips)
}

/// [Each player's ruleset] A hit of KH3's Sora as the KH2 side reads it, in the room's space: KH3 found the contact
/// and its weight, and its damage as a share of KH3's gauge.
#[derive(Clone, Debug)]
pub struct CrossHit {
    pub attacker_at: Vec3,
    pub contact: Vec3,
    /// Which way it pushes, in the room's XZ.
    pub direction: [f32; 3],
    /// The damage as a share of the attacker's own max HP.
    pub share: f32,
    pub magic: bool,
    /// A flinch (3) or a blow (4); KH3's nothing is no reaction.
    pub reaction: Option<u8>,
    /// For the logs: KH3's attack name, its damage kind and raw damage.
    pub name: &'static str,
    pub kind: &'static str,
    pub raw: f32,
}

/// A KH3 core hit (`at`: where the attacker stood in KH3's space; `kh3_max`: his max HP) as the KH2 side reads it;
/// None when it takes no HP (a guard, a reflect).
pub fn cross_hit(
    hit: &kh3_sora::host::HitRecord,
    at: kh3_sora::Vec3,
    kh3_max: i32,
) -> Option<CrossHit> {
    use kh3_sora::enums::{ETresDamageAttribute, ETresDamageKind};
    if !hit.takes_hp() {
        return None;
    }
    Some(CrossHit {
        attacker_at: Vec3::from_array(kh3::core::from_core(at)),
        contact: Vec3::from_array(kh3::core::from_core(hit.position)),
        direction: kh3::core::from_core(hit.direction),
        share: hit.damage / kh3_max.max(1) as f32,
        magic: hit.attribute != ETresDamageAttribute::Physical,
        reaction: match hit.damage_kind {
            ETresDamageKind::None => None,
            ETresDamageKind::Small => Some(3),
            _ => Some(4),
        },
        name: hit.attack_name,
        kind: hit.damage_kind.name(),
        raw: hit.damage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sora() -> Fighter {
        Fighter {
            player: Player::new([0.; 3], 0.),
            vitals: Default::default(),
            ext: Box::new(online::FighterOnline::default()),
        }
    }

    #[test]
    fn a_seat_s_rule_follows_the_pick_and_the_core() {
        let rules = MatchRules {
            ruleset: duel_rules::Ruleset::Each,
            picks: [false, true],
            ..MatchRules::default()
        };
        let f = sora();
        assert_eq!(SeatRule::of(&rules, 0, &f), SeatRule::Kh2);
        assert_eq!(SeatRule::of(&rules, 1, &f), SeatRule::Kh3Layer);
        assert!(SeatRule::Kh3Layer.kh3() && SeatRule::Kh3Core.kh3() && !SeatRule::Kh2.kh3());
        assert_eq!(SeatRule::Kh3Core.name(), "KH3");
        assert!(!SeatRule::Kh3Core.room_judges());
    }

    #[test]
    fn only_kh3_changes_an_attack_s_damage() {
        let rules = MatchRules::default();
        let facts = AttackFacts {
            formula: 10,
            magic: true,
            spell: true,
            bolt: false,
            grand: true,
            volley_dealt: 0,
            reference: 170,
        };
        let plain = (rules.damage(10, true, 170), 15);
        assert_eq!(SeatRule::Kh2.attack_damage(&rules, facts, plain), Ok(plain));
        // A grand magic's spell: 10 x 2 = 20, x1.5 = 30.
        assert_eq!(
            SeatRule::Kh3Layer.attack_damage(&rules, facts, plain),
            Ok((30, 30))
        );
        let bolt = AttackFacts {
            bolt: true,
            grand: false,
            volley_dealt: 1000,
            ..facts
        };
        assert_eq!(
            SeatRule::Kh3Layer.attack_damage(&rules, bolt, plain),
            Err("volley cap")
        );
        assert!(SeatRule::Kh2.repelled_by_guard() && !SeatRule::Kh3Core.repelled_by_guard());
    }
}
