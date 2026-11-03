# KHC-015: Cross-ruleset parity: measure, then tune

| | |
|---|---|
| Gate | Brief 4, WP-06: a repeatable benchmark that compares the rulesets, and per-ruleset scale values that bring them inside a target spread |
| Owner | burba |
| Prerequisites | KHC-013, KHC-014, KHC-024 |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. One Windows PC |

## What was done

- `source/khc/src/room/duel_bench.rs`, `--bench-matrix N`: a one-process duel (the room's Sora, seat 0, against the scripted rival, seat 1, each on his pick) played as N rounds to a knock-out. Each round's row goes to stdout (`bench_round,...`), the log, and CSV at `--bench-out` or `%LOCALAPPDATA%\KHC\bench\matrix.csv`: winner, seconds, each seat's damage and hits a second, best damage in any 5 s. Both Soras stand up full at the arena's starts 2 s after a knock-out; the run exits after the last round.
- `--bench-idle`: the rival stands still, and a KH3 room Sora repeats his script's acting half (combos, flow, shotlock, Ragnarok, spells), so a round is the time to kill an idle Sora.
- Parity: `MatchRules::parity` and `duel_rules::PARITY` (`KH2` 1.0, `KH3` 1.0). With Parity on, the damage a Sora deals is scaled by his ruleset's value under the host's multipliers (`MatchRules::scale`, in `duel_hits::land`, `kh3_hit_on_kh2` and `kh3_hits::core_hits`). Off (Authentic, the default) every scale is 1. `--duel-parity` turns it on; no lobby row yet (Brief 5, WP-07).
- Wire: the rules message carries `parity`. **PROTOCOL 12.** The rules round-trip test covers it.

## Results

Damage is reported as a share of the victim's gauge (KH2 170, KH3 265). Runs were stopped early at the owner's request (2026-11-02: no hour-long tests), so the matrix is partial.

**Time to kill an idle Sora** (`--bench-idle`, 10 rounds a run):

| Attacker vs idle victim | Runs | Rounds | Seconds to kill | Gauge a second | Burst 5 s |
|---|---|---|---|---|---|
| KH2 bench combo vs KH2 | 3 | 30 | 22.0, identical in all three runs | 4.5% | 32% |
| KH3 script vs KH3 | 1 | 4 | 69.2 | 1.7% | 47% |
| KH2 vs KH3, KH3 vs KH2 | 0 | 0 | – | – | – |

**Both fighting** (`--bench-matrix 20`):

| Pairing (seat 0 vs 1) | Runs | Rounds | Round s | Seat 0 wins | Seat 1 wins |
|---|---|---|---|---|---|
| KH2 vs KH2 | 3 | 60 | 40.3, identical in all three runs | 45% | 55% |
| KH3 vs KH3 | 2 | 25 | 93 ± 28 | 44% | 56% |
| KH2 vs KH3 | 1 | 4 | 61.6 | 100% | 0% |
| KH3 vs KH2 | 0 | 0 | – | – | – |

By ruleset in these rounds: KH2 deals 2.3% of a gauge a second and KH3 0.9% (KH3/KH2 0.38); hits 0.68 and 0.28 a second; KH3's 5-s burst is 1.46 times KH2's (shotlock and Ragnarok).

**What these numbers measure.** The scripts, more than the rulesets. KH2's bench closes in and swings without a pause; KH3's script spends time moving, aiming the shotlock and on flow moves, and its rival half stands and guards for 53 s of each 107. So KH3 looks two to three times slower to kill and twice as bursty. Tuning a scale to these would tune to the scripts.

**Parity values: not set.** Both stay 1.0. Read straight from the idle times, KH3 would need about ×3 on its damage to match KH2's time to kill, and its burst would then be over four times KH2's, far outside the 20% target. A fair reference script for each ruleset (the same goal: close in, best combo, finisher) comes first.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief4\bench\`: `m1\` (matrix), `idle1\` (time to kill), `try\`. Builds `runs\brief3\bin\b4bench`, `b4idle`. Scripts `scripts\brief4_bench.ps1`, `brief4_bench_report.py`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `scale = 1.0` leaves damage unchanged (`a_scale_of_one_leaves_damage_unchanged`) | Agent | PASS |
| Burst window and row columns (`duel_bench` tests); rules round trip with `parity` | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: 1478 passed, 2 ignored |
| `tools\Build-Launcher.ps1` (what `build.bat` runs) | Agent | PASS |
| `--bench-matrix` runs unattended and three runs agree | Local | PASS for KH2 vs KH2 (identical rows, three runs); the rest stopped short |
| The matrix for every ordered pair, three runs each | Local | NOT RUN: stopped at the owner's request; KH3 vs KH2 none, KH2 vs KH3 one short run |
| Each benchmark within 15% (burst 20%) across rulesets | Local | FAIL: damage a second 62% apart, burst 32%, with the scripts as they are |
| Every ordered pair wins 40–60% of bot rounds | Local | FAIL for KH2 vs KH3 (4 rounds, all to KH2); PASS for the two same-ruleset pairs |
| Distance in 3 s, vertical reach, invulnerable ticks a 10 s, recovery after a launch | Local | NOT RUN (not built) |
| Parity table set from the matrix | Local | NOT RUN: see above |
| Human playtests of each pairing | Owner | NOT RUN |
| Frame rate within 5% | Local | NOT RUN |

## Uncertainty

Bots are not players, and these bots are not alike. The KH3 numbers come from one or two runs.

## Questions for the owner

| Question | Default until answered |
|---|---|
| Write matched reference scripts for KH2 and KH3 before setting Parity, or set KH3's scale from the idle times now? | Matched scripts first; Parity stays 1.0 |
| Ragnarok takes about 0.7 of a KH2 gauge a volley and a KH2 Sora cannot dodge it (KHC-013). Cap volleys against KH2 Soras? | No change |

## Verdict

PARTIAL. The bench, the idle mode and Parity's switch are built and tested; the matrix is partial and the Parity values are not set. Next: a matched reference script per ruleset, then the full matrix.
