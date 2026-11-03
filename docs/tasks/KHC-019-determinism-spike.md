# KHC-019: Determinism spike

| | |
|---|---|
| Gate | Brief 1, WP-10: whether the duel's simulation is bit-identical across runs and PCs, how costly its state is to copy, and so rollback, shadow verification or host authority |
| Owner | burba |
| Prerequisites | KHC-024 |
| Source | The player's installs, only read, at run time. One Windows PC so far |

## Randomness and time on the duel's path

| Source | Where | Effect on the simulation |
|---|---|---|
| Fixed 60 Hz tick | Bevy's `FixedUpdate` (T236) | The base: the simulation does not read the frame time. A long frame is lost past Bevy's limit, which shifts when things happen, not what a tick does |
| Drive roll seed from the clock | `duel_net::begin`, `DriveRt::seed` (Antiform, `drive_room.rs`) | Yes, differs each session. `--duel-seed` fixes it. Alone it starts from a constant |
| KH3's status and grand-magic rolls | `kh3_sora` `next_random`, xorshift32 | Deterministic stream from a fixed start |
| Music pick from the clock | `duel_music.rs` | Cosmetic |
| Vote's coin flip from the clock | `duel_vote.rs` | Not on a fight's path; the result is sent |
| Session nonce from the clock | `net::session` | Never reaches the simulation |
| Timeouts and overlay's `Instant` | `duel_net.rs`, `net::session`, `duel_log.rs` | Whether a session goes on, not what a tick does |
| Threaded loading of Drive kits and KH3's data | `drive_room.rs`, `kh3_model.rs` | Can decide when a Sora may act. Held off by `--script-after-load` for KH3; Drive kits not separately proven |
| `HashMap`s | `kh3_player` (`core.rs` grid and planes, `attack.rs` book), `duel_hits.rs` `previous` | Read by key in what I looked at. Not audited for iteration; the identical runs below are the evidence that it did not matter there |
| `game.tick` read by rules | Action starts, cast requests | Stored as absolute ticks, so left out of the hash (`started`) |

## What was done

- `--state-hash <path>` (`duel_hash.rs`): after each fixed tick, on the host or alone, `tick,hash` and one column per group: `motion`, `vitals`, `items` (form, items, bullets) and `kh3` for each Sora, and `match`. SHA-256 truncated to 64 bits, of each field's exact digits. `--state-dump` writes the text.
- The hash covers what a snapshot carries. KH3's Sora has more inside him than `Shown`, so a hidden difference shows only when it reaches a shown field.
- `run_compare --hash` reports the first tick and groups.
- Each `--state-hash` run logs `duel_copy_cost` every 600 ticks.

## One-PC comparison

Three runs per pairing, 5400 ticks. Details in KHC-024.

| Pairing | Alone | Over loopback, `--script-locked` |
|---|---|---|
| KH2 vs KH2 | 3 of 3 identical | Not valid: the joiner misses its 60-tick window |
| KH3 vs KH3 | 3 of 3 identical | 2 of 3; run 2 parts at tick 3856 in `motion1`, `kh3_1`, cause not found |
| KH2 vs KH3 | 2 of 3; run 1 parts at tick 0 in `motion0` | 3 of 3 |
| KH3 vs KH2 | 3 of 3 identical | 2 of 3; run 2 parts at tick 1189 in `vitals0`, `match`, likely the other Keyblade |

So on one PC the simulation can repeat bit for bit over 90 s, including KH3's and across two processes. What remains is mostly in how a run is set up, with one difference not yet explained in each of two columns.

## Copy cost

Cloning `Player`, `Vitals` and `Kh3` of both Soras, measured during runs alone and over loopback:

| Pairing | Median | Worst seen |
|---|---|---|
| KH2 vs KH2 | 3 µs | 11 µs |
| KH3 on either or both | 12 to 28 µs | 188 µs |

3072 bytes by `size_of`; what they point to is not counted. This is under the 1 ms target, **but it is not the whole state**. A rollback would also have to copy, or make copyable:

- `DriveRt` and `ItemRt` of each Sora, which hold loaded files and have no `Clone`
- the spells' runtime (`game.magic`) and its shots
- the hit records (`Vitals` has them) and `duel_dealt`, `duel_landed`
- `Match`, the arena's collision floor state (`Rival::floor`), and KH3's geometry cache, which need not be copied if it stays read-only
- not needed: effects, sounds, cameras, HUD, pose evaluation

So the number measures the easy part. Copying the listed parts is not measured.

## Decision

Default until the owner's two-PC comparison: **keep host authority**, and no ranked play. The thresholds ask for identical hashes over 10 scripted matches on two PCs and a copy under 1 ms. One-PC identity is not established in every pairing, the copy is not measured whole, and two PCs have not been run.

Owner's step, once the outliers in KHC-024 are understood: on each PC, the same executable, the same flags, alone (`--rival-script --script-after-load --script-stop 5400 --state-hash h.csv`), then `run_compare --hash a.csv b.csv`.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief1\`: `final\`, `last\`, each run's `host.csv` and `host.jsonl`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| The flag compiles; a fixed state hashes to a fixed value; a change shows in its own group | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: no warnings, 1410 tests passed, 2 ignored |
| `tools\Build-Launcher.ps1`, what `build.bat` runs | Agent | PASS |
| Frame rate within 5% of the run before the change (`--perf-overlay`) | Local | NOT RUN: the flags are off in play, but this was not measured |
| One-PC comparison recorded | Local | PASS as a record; identity is not in every pairing |
| Copy cost recorded | Local | PASS for `Player`, `Vitals`, `Kh3`; the rest of the list NOT RUN |
| Two-PC comparison and the decision | Owner | NOT RUN |

## Uncertainty

Three runs per pairing. Same CPU throughout: floating-point or ordering differences between CPUs are not tested at all.

## Verdict

PARTIAL. Flag and hash PASS. One-PC comparison and part of the copy cost recorded. Two-PC comparison and the final decision are NOT RUN.
