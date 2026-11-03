# KHC-014: One seam for what differs between rulesets

| | |
|---|---|
| Gate | Brief 4, WP-05: what differs between rulesets behind one interface, so a new ruleset adds an implementation and not more branches; no gameplay change |
| Owner | burba |
| Prerequisites | KHC-013 (its runs are the frozen behaviour), KHC-024 (the comparator) |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. One Windows PC |

## What was done

- New `source/khc/src/room/ruleset.rs`. `SeatRule::{Kh2, Kh3Layer, Kh3Core}` is an enum with a `match` in one module; no trait objects in `Game` (extension.rs: a hook kept inside `Game` could not take `&mut Game`). `SeatRule::of` reads a seat's pick and whether KH3's core runs him.
- The brief's five parts, as they sit now:

| Part | In the seam |
|---|---|
| Outgoing hit | `Weight` (source, strike, attacker in the air), `SpellFacts`, `AttackFacts`, `SeatRule::attack_damage` (grand magic, volley cap), `repelled_by_guard`, `landed`; `CrossHit` / `cross_hit` (a KH3 core hit as the KH2 side reads it) |
| Incoming hit | `guarded`, `dodged`, `hit_taken`, `take_share` (KH3's core victim side), `react` |
| Defensive state | `room_judges`, `untouchable`, `cannot_be_hit` (also what the joiner's rewind keeps per tick) |
| Resources | `kh3_attack_state`, `mode_name`, `spark_ready`, `turn`; HP and MP still live on `Fighter` and KH3's mirror in `kh3_hits` |
| Seat setup | Not moved: `duel_kh3::settle_seats` / `leave_kh3` (KHC-013) |

- `duel_hits.rs` has no `kh3_player::`, `kh3_sora::` or `.kh3.` reference left; `layered`, `core_victim`'s branches and `kh3_hit` go through `SeatRule`. KH2's `untouchable` moved to the seam with its comment. `kh3_hits` turns a core hit into a `CrossHit` before handing it over.
- No wire change.

## Results

The four pairings, one process with the scripted rival (`solo`, 80 s) and three runs each, on the seam's build against KHC-013's (`wp04`), compared with `run_compare --window 0`:

| Pairing | Result |
|---|---|
| KH2 vs KH2 | SAME, 134 gameplay events, all three runs |
| KH3 vs KH3 | SAME, 61 gameplay events, all three runs |
| KH2 vs KH3 | Each run SAME as one of the frozen runs. These runs have two outcomes (a start 60 ticks apart, KHC-024's known outlier); both builds show both |
| KH3 vs KH2 | Each run SAME as one of the frozen runs; the same two-outcome start |

Loopback runs (`seamduo`) change course from run to run (KHC-024), so only their end is compared: see Checks.

## Evidence

Private: `%USERPROFILE%\kh2-kh3-ref\runs\brief4\wp04\` (frozen), `brief4\seam\`, `brief4\seamduo\`; builds `runs\brief3\bin\b4wp04`, `b4seam`. Script `scripts\brief4_runs.ps1`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `duel_hits.rs` has no `kh3_player::` or `.kh3.` outside the seam | Agent | PASS (`grep`) |
| Seam unit tests (a seat's rule, only KH3 changes an attack's damage) | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings` | Agent | PASS |
| `cargo test --locked` | Agent | See KHC-015 (run once for the brief's code) |
| The four event logs show no gameplay difference, WP-13 comparator | Local | PASS for one-process runs, within the recorded two-outcome start of the mixed pairings |
| Loopback runs end as before | Local | PASS: each pairing's round, winner and score at 80 s as in the frozen runs (KH3 vs KH3 now shows its tally, KHC-013's line) |
| Frame rate within 5% | Local | NOT RUN |

## Uncertainty

The layer path (KH3 without its core) was not run; it is the same code moved. One commit for the whole move, not one a function: each function moved was compared together. Seat setup did not move.

## Verdict

PASS for the seam and the one-process comparison. Next: KHC-015 measures through it; a new ruleset is a `SeatRule` variant (KHC-020).
