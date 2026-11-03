# KHC-024: Noise floor and run comparator

| | |
|---|---|
| Gate | Brief 1, WP-13: how much two runs of one scripted duel differ on one PC, and a tool that compares runs within that noise |
| Owner | burba |
| Prerequisites | KHC-010 (the mixed rulesets are two of the four pairings) |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. Nothing from them is in this repository. Runs on one Windows PC, while other builds were running on it |

## What was done

- `source/validation/src/run_compare.rs` and the `run_compare` binary. It matches gameplay events by kind and order, allows tick offsets inside `--window`, and compares every field but ticks, places and ids exactly (HP, damage, scores, hits, knock-outs). Every other event is compared by count and sorted into exit, loading or cosmetic. It prints a readable diff and exits 1 on a gameplay difference. `--hash` compares `--state-hash` files and names the first tick and group that differ.
- `--script-after-load` (`duel_script.rs`): scripts count from when loading is over, not from the program's start. Their tick is the fight's on the host's clock in a session, and the first loaded tick alone. The log gets `duel_script_start`, and the comparator counts ticks from it.
- `--script-locked` [duel choice, KHC-024]: takes the network's timing out of a session for a test. The joiner answers each snapshot once and tags the answer with that snapshot's tick. The host uses it exactly 60 ticks later, and a missing answer is logged as `duel_lock_miss`. It changes the joiner's reaction time, so a locked run is for comparing runs, not for judging play.
- Both sides log `duel_score` on every change to the match, so a joiner's log and a replay's can be compared with the host's. `--duel-seed`, `--duel-rival-pick` and `--script-stop` make a run repeatable. `--state-dump` writes the text behind each hash.

## What differed, and why

| Difference | Kind | Cause | Now |
|---|---|---|---|
| A script's presses shift with the time spent loading | Gameplay | Scripts counted ticks from the start of the program | Fixed: `--script-after-load` |
| Every tick's state differs from tick 0 | Gameplay | Sora's frame and idle counter, and KH3's animation clock, count how long he stood | Fixed: at the script's start the clocks go to zero and KH3's Sora is rebuilt (`Player::restart_clocks`, test only) |
| A fight starts while the host's KH3 data is still loading, so that Sora stands for the first ticks | Gameplay | Unattended lobby readied before the load ended | Fixed for scripted runs: the lobby's script waits |
| The host uses the joiner's buttons a varying number of ticks after they were made, then the runs part completely | Gameplay | Network and frame timing | Recorded: this is what an unlocked networked run is. `--script-locked` removes it for tests |
| The joiner's Keyblade differs between runs (`stats` [55,57,53] or [57,61,53]) | Gameplay | The unattended lobby's menu presses race the joiner's camp opening | Recorded, not fixed. It is the most likely cause of the KH3 vs KH2 networked outlier below; the outlier's cause is otherwise not shown |
| Solo KH2 vs KH3: one run in three starts 8 units further on at tick 0 (-592 instead of -600) | Gameplay | Not found | Recorded. Same two outcomes each batch |
| Exit tick | Exit | When each process stops. A joiner now stops 3 s after the host, so that its leaving is not in the host's last ticks | Counted, not a failure |
| `duel_bank`, `duel_art`, `duel_music_loaded` | Loading | Threads finish when they finish | Counted, not a failure |
| `hitch`, `frame_stats`, `census`, `effect`, `state`, `pose_jump` | Cosmetic | Frame timing and logging on the drawn frame | Counted, not a failure |

## Noise floor (build `9cdbd611…`, solo set on build `7e45af7f…`)

Three runs per pairing, 5400 ticks (90 s) after the script's start. "Alone" is one process, the host's simulation with a scripted rival. "Locked" is a host and joiner over loopback with `--script-locked`. "Open" is the same without it.

| Pairing (host vs joiner) | Alone: hashes | Locked: host's hashes | Locked: joiner's offset | Open: host's log |
|---|---|---|---|---|
| KH2 vs KH2 | 3 of 3 identical, 150 events, offset 0 | INVALID: 2190 to 2398 `duel_lock_miss` each run | Not measured | Different, offsets to 1502 ticks |
| KH3 vs KH3 | 3 of 3 identical, 66 events, offset 0 | 2 of 3 identical, 81 events; run 2 parts at tick 3856 with no miss, cause not found | 8 to 15 ticks | Different, 27 gameplay differences |
| KH2 vs KH3 | 2 of 3 identical (see above), 64 events | 3 of 3 identical, 138 events, offset 0 | 3 ticks (one event at 184: the match's end, as the host stopped) | Different, offsets to 566 ticks |
| KH3 vs KH2 | 3 of 3 identical, 70 events, offset 0 | 2 of 3 identical, 142 events; run 2 is the other Keyblade | 13 ticks | Different, offsets to 671 ticks |

**What to use:** compare alone or locked runs. On the host, gameplay events match at offset 0: use `--window 0`. On a joiner, use `--window 15`. A networked run without the lock cannot be compared event by event: compare its final result only.

**The pass line is not fully met.** It asks for no gameplay differences per pairing. Alone, one pairing has an outlier run. Locked, two pairings have one each, and KH2 vs KH2 is not a valid measure at all.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief1\`: `final\` (alone), `last\` (locked), `open\` (open), `base\` (before the fixes), each with `*.compare.txt` where written. Scripts in `%USERPROFILE%\kh2-kh3-ref\scripts\`: `brief1_runs.ps1`, `brief1_compare.ps1`, `brief1_last.py`, `brief1_dumpdiff.py`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Comparator tests on synthetic logs: identical, offset only, a real difference, a missing event, broken lines; hash files | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: no warnings, 1410 tests passed, 2 ignored |
| `tools\Build-Launcher.ps1`, what `build.bat` runs | Agent | PASS |
| Frame rate within 5% of the run before the change (`--perf-overlay`) | Local | NOT RUN: the flags are off in play, but this was not measured |
| Three runs per pairing, alone | Local | PASS for KH2 vs KH2, KH3 vs KH3, KH3 vs KH2; FAIL for KH2 vs KH3 (one run in three differs) |
| Three runs per pairing, over loopback, locked | Local | PASS for KH2 vs KH3; FAIL for KH3 vs KH3 and KH3 vs KH2 (one run in three); not valid for KH2 vs KH2 |
| Noise floor written here | Local | PASS |
| The same on two PCs | Owner | NOT RUN |

## Uncertainty

Three runs is a small sample: a pairing's 3 of 3 does not say it never parts. A locked joiner in KH2 vs KH2 falls more than 60 ticks behind (72 to 76 snapshots waiting) and I did not find why; only that pairing does it. The runs shared the PC with other builds, which is part of the noise measured. One PC says nothing of two: see KHC-019.

## Verdict

PARTIAL. The comparator is PASS. The noise floor is recorded, with the outliers above open. Next: KHC-031 and KHC-028, which do not depend on them.
