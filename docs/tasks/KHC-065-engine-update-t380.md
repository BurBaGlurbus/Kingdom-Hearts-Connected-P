# KHC-065: the engine's T380 brought in

| | |
|---|---|
| Gate | None moves. Single player in this engine's room takes the engine's T380; multiplayer keeps working as it did |
| Owner | burba |
| Prerequisites | KHC-054 (the engine merged through its T379) |
| Source | The engine's T380 ("Sora's guard arm, Explosion and Finishing Leap"), the only engine change after the T379 point KHC-054 took. Copied by hand into this repository's own copy (no dependency, no link) |
| Wire | PROTOCOL 15, unchanged. No new action codes |

## What was done

The engine's T380 diff was applied to this copy with its `source/main/src/` paths read as `source/engine/src/`. Every hunk applied without conflict (some at line offsets, where Connected's copy has its own lines):

- `source/data/src/motion_eval.rs`: constraints follow their timed activation entries, so guard lets go of the Keyblade with the left hand. `evaluate_with_clock` takes a flag for the timed pass; nothing in Connected's own crates calls it.
- `source/game/player/src/lib.rs`: a ground attack whose root climbs is lifted into the air with its motion still playing; no-gravity ranges skip the land test (Finishing Leap hops, strikes and leaps).
- `source/engine/src/test_room.rs`, `test_room/character_kit.rs`, `drive_room.rs`, `enemy_battle_hits.rs`, `rival.rs`: attack shapes on the object's origin are kept as round volumes (Explosion, Finishing Leap deal damage), and the callers follow the new signatures.

Left out: the engine's `motion_eval_compare` research probe (its program entry points and probes were never copied here) and its own T380 record.

The duel uses the same player code, so the guard arm and both combo finishers change in a duel as they do in single player, for both Soras. Both players need this build, as for any engine change.

## Evidence

None private: this is a build check only.

## Checks

| Check | Verdict |
|---|---|
| `cargo check --all-targets --offline` | PASS |
| `rustfmt --check --edition 2024` on the changed files | PASS |
| `cargo clippy`, `cargo test` | NOT RUN: kept to a quick check |
| Single player, a duel | NOT RUN |

## Uncertainty

Nothing was run. The lifted ground attack changes Sora's height during Finishing Leap; the duel's rollback compares state hashes and both sides run the same code, but it was not seen in a match.

## Verdict

PASS for building; NOT RUN for play.

## Next

Burba tries Guard, Explosion and Finishing Leap in a loopback duel. Later engine work is merged the same way; this took everything up to the engine's T380 (KH2RW c9fea04).
