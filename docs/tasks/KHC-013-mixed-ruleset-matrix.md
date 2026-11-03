# KHC-013: Mixed-ruleset matrix

| | |
|---|---|
| Gate | Brief 4, WP-04: every attacker and victim pairing under *Each player* known, intended, fixed where wrong and kept in tests |
| Owner | burba |
| Prerequisites | KHC-010; Brief 1's comparator (KHC-024) |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. One Windows PC, loopback |

## How a hit crosses rulesets today

KH3's Sora runs on KH3's own core (`kh3_sora`) whenever its sheets are installed, which is the normal case. Without them a KH3 seat runs `kh3_player`'s layer on KH2's controller (the "layer"). So there are four routes, one per pairing:

| Attacker → victim | Code | Damage | Cap | Who decides guard and dodge | Reaction |
|---|---|---|---|---|---|
| KH2 → KH2 | `duel_hits::land` | formula × host multiplier (×3 attacks, ×1.5 magic) | 40% of the victim's 170 | Victim: KH2 guard windows (clash cone), Reflect, invulnerable frames | KH2's from the ATKP row |
| KH2 → KH3 | `land`, then `core_take_kh2_hit` | the same number, as a share of the reference max HP, times KH3's max | 40% share | Victim: KH3's core (guard, dodge, Risk Dodge) | KH3's core, from the row's class (nothing, flinch or blow) |
| KH3 → KH2 | `kh3_hit_on_kh2` | KH3's own damage as a share of KH3's max, × host multiplier ÷ its default, of 170 | 40% | Victim: KH2 Guard facing the attacker, Reflect, invulnerable frames | KH2 flinch (KH3 Small) or blow (anything heavier) |
| KH3 → KH3 | `kh3_hits::core_hits` | KH3's own damage; since this record × host multiplier ÷ its default | 40% of KH3's max, since this record | Victim: KH3's core | KH3's core |

The intent (KHC-010's duel choice, kept): a hit takes the same share of the victim's gauge whatever ruleset the victim is on, and the host's multipliers and cap reach every hit.

## What was done

- **Fixed: KH3 against KH3 ignored the host's rules.** Core hits on a core Sora took no notice of the damage multipliers or the hit cap, while the same hit on a KH2 Sora did. They now go through `MatchRules::kh3_damage`: × multiplier ÷ default, then the cap. At the default rules only the cap can change a hit; no default-rules run had a hit above it (largest share seen 0.079).
- **Fixed, layer only: per-seat KH3 rules.** `layered = rules.all_kh3()` made Grand magic ×2, the volley cap and KH3's strike reactions apply only when both seats were KH3. Per the brief's default, what the attack is (a grand magic, a bolt's cap, its weight, the KH2 repel) now follows the attacker's seat, and how it is taken (dodge, guard push, reaction) the victim's. With the core installed this path is not taken by mixed duels; with both seats on one ruleset nothing changes.
- **Fixed: KH3 against KH3 counted no hits.** The round and match tallies (`duel_dealt`, `duel_landed`) now count core hits. Best of 3 rounds showed `hits [0,0]` before.
- **Fixed: a crash in one-process duels.** A KH2 rival read KH3's sheets (`kh2_body` reads level 99's defense) before KH3's files finished loading: "the sheets are not installed" in three of four `--rival-script` runs. The foe body is now read only once KH3's world is there.
- **Kept: the reference HP for a KH3 victim** is the attacker's KH2 max HP (`rules_max`). No lobby choice changes max HP: `duel_loadout` sets level 99 with every bonus level, and the camp's choices (Keyblade, abilities, items) do not touch it. All 46 loadouts and 1664 reference values logged in this record's runs read 170. The brief's default applies (a fixed reference only if a lobby choice changes max HP).
- `duel_hit` and `kh3_hit` log `share`, `ref_max`, `capped`, `attacker_rules` and `victim_rules`.
- Pure functions with tests: `share_of`, `capped`, `cap_applied`, `ratio_to_default`, `kh3_damage`; a pairing table test (`every_pairing_takes_the_same_share_and_stops_at_the_cap`); `leave_kh3`, the rematch's return to KH2, tested.

## The matrix, as run

Rows are what the logs show (`brief4_matrix.py`); "share" is of the victim's gauge. Grouped by the brief's cells.

| Attacker | Victim | Seen | Share | Result | Verdict |
|---|---|---|---|---|---|
| KH2 combo hit | KH2 idle | yes | 0.018–0.071 | hit, KH2 flinch (3) | PASS |
| KH2 finisher | KH2 idle | yes | 0.071–0.141 | hit, blow (7, 11) | PASS |
| KH2 combo hit | KH2 guarding | yes | – | guard flash, recoil, then refused while invulnerable | PASS |
| KH2 combo hit | KH3 idle | yes | 0.018–0.071 | hit, KH3 Damage | PASS |
| KH2 finisher | KH3 idle | yes | 0.071–0.194 | hit, KH3 Damage | PASS |
| KH2 spell | KH3 | yes | 0.012–0.265 | hit, KH3 Damage | PASS |
| KH2 hit | KH3 guarding | yes | 0.018–0.071 | KH3's core guards (Guard, or Reflect after a finisher) | PASS for the outcome; **FAIL for the log and tally**: see below |
| KH2 hit | KH3 Dodge Roll, Air Slide, Risk Dodge | no | – | the KH3 script dodged, but no KH2 swing met a dodge | NOT RUN |
| Reflect counter | KH2, KH3 | no | – | `--duel-magic` cast spells; no counter identified in the logs | NOT RUN |
| KH3 combo hit | KH2 idle | yes | 0.006–0.024 | hit, flinch (3) | PASS |
| KH3 launcher, flow attack | KH2 idle | yes | 0.018–0.041 | hit, blow (4) | PASS |
| KH3 spell (Firaga, Thundaga) | KH2 idle | yes | 0.071–0.076 | hit, flinch | PASS |
| KH3 shotlock bolt | KH2 idle | yes | 0.024 | hit, blow | PASS |
| Ragnarok | KH2 idle | yes | 0.029 a bolt | 24 bolts a volley, 0.70 of the gauge | PASS for the rule (the same share as on KH3); balance in KHC-015 |
| KH3 hit | KH2 guarding | yes | – | guard flash and recoil, then refused while invulnerable | PASS |
| KH3 combo, launcher, spells, bolts, Ragnarok | KH3 idle and in Damage | yes | 0.019–0.079 | hit by KH3's core; multipliers and cap now apply | PASS |
| KH3 hit | KH3 guarding, dodging | yes (states) | – | KH3's core guards (Reflect) and dodges (Dodgeroll, AirSlide); no hit is logged | PASS |
| KH3 Grand magic | any | unclear | – | the script's spells logged as MAG_FIGA / MAG_FIGA_FIN; whether one was a grand magic is not logged | NOT RUN |

**The one FAIL.** A KH2 hit on a KH3 Sora is handed to KH3's core, which guards or takes it on its next step. The host logs it as `duel_hit`, plays the hit's sound and spark, and counts it in the match's tally before the core decides. HP is right (the core decides); the tally, the logs and the spark are wrong for guarded hits. Fixing it needs the core to report what each queued hit became. Next task.

**Ragnarok on a KH2 Sora.** 48 bolts in two volleys took 1.39 gauges in `m-h2j3-strafe`; the KH2 bench never dodges and KH2 has no answer to a volley but invulnerable frames. The share per bolt is the same as on KH3. A balance question for KHC-015 and the owner, not a rule mismatch.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `brief4\base\` (before, on `df15c62`), `brief4\wp04\` (four pairings, three solo and three duo runs each, 80 s), `brief3\brief4-m-*` (four-minute loopback Best of 3: `m-h3j3`, `m-h2j3`, `m-h3j2`, `m-h2j3-magic`, `m-h2j3-strafe`). Scripts: `scripts\brief4_runs.ps1`, `brief4_cells.ps1`, `brief4_matrix.py`. The `wp04` runs used a build without the tally line; the `m-*` runs had it.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Unit tests for the share, cap and ratio functions and the rematch | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings` | Agent | PASS |
| `cargo test --locked` | Agent | PASS at the end of the brief (KHC-015): 1478 passed. `visual::frame_limit::tests::the_app_holds_the_cap`, a timing test, failed earlier only while two duels ran on the PC |
| `build.bat` (`tools\Build-Launcher.ps1`) | Agent | PASS at the end of the brief |
| The full matrix, every cell PASS or an owner-approved exception | Local | FAIL: one FAIL (log and tally of guarded hits on KH3), four cells NOT RUN |
| Scripted Best of 3, host KH2 vs joiner KH3, same result on both screens | Local | PASS: `m-h2j3`, 2–1 to the host on both |
| Scripted Best of 3, host KH3 vs joiner KH2, same result on both screens | Local | PASS: `m-h3j2`, 1–2 to the joiner on both |
| A rematch where the picks change restores KH2's gauges | Agent | PASS (`a_sora_back_on_kh2_has_kh2_gauges_and_no_kh3_state`); the loopback rematch was KHC-010's `mixed-c` |
| Frame rate within 5% | Local | NOT RUN |

## Uncertainty

The cells came from scripts: KH2's bench never dodges and seldom guards, the KH3 script dodges on its own schedule, and no run aimed a KH2 swing at a KH3 dodge. The layer path (no KH3 core) was not run at all; its change is by reading only.

## Questions for the owner

| Question | Default until answered |
|---|---|
| Should KH3-only damage kinds apply when only the attacker is KH3? | Yes, by attacker seat (done, layer path) |
| Should a KH3 victim's share use a fixed reference HP? | No: no lobby choice changes max HP |
| Should the host's multipliers and cap reach KH3 against KH3? | Yes (done; no change at the default rules in these runs) |

## Verdict

PARTIAL. The routes are known and three mismatches are fixed; one cell fails (log and tally only) and four are not run. Next: the core's report of what a queued hit became, then scripts that reach the NOT RUN cells.
