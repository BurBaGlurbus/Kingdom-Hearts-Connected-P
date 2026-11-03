# KHC-032: Equal input delay

| | |
|---|---|
| Gate | Brief 2, WP-02: the host's buttons wait as long as the joiner's, so neither player acts first |
| Owner | burba |
| Prerequisites | KHC-024, KHC-031. KHC-033 for step 6 |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. Runs on one Windows PC over loopback |
| Number | The brief called this record KHC-011, which was already taken |

## What was done

- `source/khc/src/room/duel_delay.rs`: half the round trip in ticks, smoothed over about 2 s, clamped to 0 to 6. A FIFO hands each tick's buttons, spell, item or form back exactly N ticks later. The wait changes only while the Soras are held.
- [duel choice, KHC-032] The reading also counts the ticks the joiner's buttons wait in the host's queue. The brief's half round trip alone left the joiner 0.1 to 2.7 ticks behind at 80 ms, because the queue stays as long as it once grew. So a LAN can wait 1 tick, not 0.
- [duel choice, KHC-032] A KH3 host's buttons do not wait: KH3's seats are not predicted (KHC-033), so he would feel every tick.
- `RoomHooks::own_pad` instead of `SoraDrive.script`, the brief's likely route: the drive hook runs before the room has read the menu's commands, and has none of the pad's values.
- Rule `latency_match`, `--latency-match`, `--no-latency-match`. The lobby's Match page has *Equal input delay* and *Host input delay*; the overlay has "Host input delay: N ticks". PROTOCOL 3.
- Step 6: `duel_predict::host_predict` draws the host's own Sora from the simulated one plus the buttons still waiting. Hits, snapshots and the state hash read the simulated one. `RoomHooks::camera_sora` points the follow camera at the drawn one.
- **Default on**, the brief's default until the owner answers: see Owner's question.
- `--duel-press-test`: both Soras stand and jump in turn, each press logged with the wall clock.

## Results

Press to the action's start on the host's simulation, `--net-sim 40,0,0` on both sides, about 100 ms by renet's reading.

| Delay rule | Runs | Gap, joiner minus host, in ticks |
|---|---|---|
| Half the round trip, the brief's | 7 | 1.0, 0.1, 1.6, 2.0, 0.9, 2.3, 2.7 |
| With the queue, as committed | 6 | -0.3, -1.0, -0.5, 0.2, 1.7, 0.6 |
| With the queue, LAN | 1 | 0.6, waiting 1 tick |

The host's own Sora as drawn starts on the tick of the press: median 0, largest 0, over 12 presses in each of 6 runs.

Comparing with the build before this work (`--script-locked`, KH2 host, KH3 joiner, host's log, `--window 0`): the old build agrees with itself in 2 of 3 and also gives the outcome at offset -81. The new build gives the same two outcomes, 2 of 3 matching the old build exactly. Alone, the state hashes of KH2 vs KH2 and KH3 vs KH3 are identical over 3600 ticks and every gameplay event matches. These were run on the build before the queue reading, which changes nothing with the lock or alone.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief2\`: `press-both`, `f-press`, `g-press`, `i-press`, `j-lan`, `cmp-base`, `cmp-base2`, `cmp-final2`, `solo-base`, `solo-final`. Scripts in `%USERPROFILE%\kh2-kh3-ref\scripts\`: `brief2_duo.ps1`, `brief2_runs.ps1`, `brief2_press.py`. Binaries in `bin\`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| The wait from a round trip, the FIFO exactly N ticks, the rules round trip | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: 1429 passed, 2 ignored |
| `tools\Build-Launcher.ps1`, what `build.bat` runs | Agent | PASS |
| Presses within 1 tick of each other at 80 ms | Local | PARTIAL: 5 of 6 runs, one at 1.7. Median of the six 0.0 |
| No gameplay difference on loopback from before the change | Local | PASS within the noise above, locked and alone. An unlocked LAN run with the rule on can differ, as the host now waits a tick |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN. The logs' `frame_stats` vary more between runs than any cost would show |
| A person, a controller, two PCs | Owner | NOT RUN |

## Uncertainty

Delay readings move by a tick or two in the seconds after loading, when frames stall, and the last reading before a fight is what holds. The host's swing sounds come with his simulation, so they are the wait late against what he sees. Lock-on is chosen from the simulated Sora, N ticks behind what is drawn; only the camera reads the drawn one.

## Owner's question

Should host input delay be on by default? Default per the brief: on, clamped to 100 ms. The lobby's row and `--no-latency-match` turn it off.

## Verdict

PARTIAL. Code and Agent checks PASS. The 1-tick gap holds in 5 of 6 runs. Two PCs NOT RUN.
