# KHC-026: Adaptive playout and clock sync

| | |
|---|---|
| Gate | Brief 3, WP-15: the joiner plays snapshots smoothly over internet jitter, without stalls or skip jumps |
| Owner | burba |
| Prerequisites | KHC-031 (`--net-log`), KHC-024 |
| Source | The player's Steam KH 1.5+2.5, only read, at run time. One Windows PC, loopback |
| Wire | No change: the playhead is the joiner's alone, and the input message already says which host tick was on screen. PROTOCOL stays 8 |

## What was done

- `source/khc/src/room/duel_playout.rs`, a controller that knows nothing of the room: arrivals in, a playhead in host ticks out.
  - It measures each snapshot's lateness against the earliest of the last two seconds and holds back the 95th percentile of it, plus a tick for rounding: 2 to 8 snapshots, changed by one snapshot a second at most. [duel choice, KHC-026] Lateness rather than the spread of arrival intervals, so a lost snapshot widens nothing and drift over two seconds stays under a tick.
  - The playhead follows the host's clock as that estimate gives it, at most 5% faster or slower. No skipping, except past half a second behind (this PC stalled), where it jumps once and says so as the old warning did.
  - Past the newest snapshot it goes on for 6 ticks, then waits, and the existing warning comes after half a second of that.
  - While the host's clock runs at double rate (its tick does in the lobby and the result menu), nothing moves, so it simply shows the newest.
- `duel_sync::join_apply`: live sessions use the playhead; positions, yaw and a same action's frame glide between the snapshot shown and the next, and a gap carries a Sora on at his speed. Actions, HP, forms and the rest stay the snapshot's. A Sora moved more than 200 units a tick was put there and does not glide. Replays and `--script-locked` keep the queue of one snapshot a tick.
- The hold in force is `Playout::hold_ticks`; the overlay shows it and the speed; the network log has `hold_ticks`, `speed`, `jumps` and `froze` (the longest wait with nothing to show since the line before). The rewind of KHC-025 already reads what was on screen, which now includes the hold.

## Results

Poor profile (`--net-sim 75,15,3` on both), a 5-minute timed match, the host strafing and guarding, the joiner walking and jumping: `poor5-wp15` against `poor5-base`, the build before this change.

| | This change | Before |
|---|---|---|
| Seconds of the fight with a skip | 2 in 296 | 5 in 298 |
| Snapshots skipped | 82 | 97 |
| Longest wait with nothing to show | 633 ms | not measured |
| Hold in force | median 4 snapshots, at most 8 | 2, fixed |

Both skips and both waits came in the first ten seconds of the fight, around one stretch of 867 ms in which nothing at all arrived (the PC was still settling after loading; both runs show such a stall near the start). For the 4 minutes 50 seconds after it there was no skip and no wait over 6 ticks.

On a LAN the joiner's view now trails the host by about 2 ticks, where the fixed queue never drained and sat 6 to 8 behind (Brief 2's finding).

Short matches with attacks at the Poor profile end with the same score on both screens (`poor-wp15`).

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief3\`: `poor5-wp15`, `poor5-base`, `poor-wp15`, `poor-base`, `wp15-smoke`. Read with `scripts\brief3_last.py`; runs from `scripts\brief3_poor5.ps1` and `brief3_duo.ps1`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Controller on synthetic traces: steady, jittery, burst loss, 0.1% drift both ways, a stall, the host's doubled clock, 30 Hz | Agent | PASS |
| Gliding: between two snapshots, a different action, a Sora put somewhere, a gap | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS, in the commit's run |
| Poor profile, 5 minutes: skip warnings 1 a minute or fewer | Local | PASS: 2 in 5 minutes, against 5 before |
| Poor profile, 5 minutes: no freeze over 100 ms | Local | FAIL: 533 and 633 ms, both in one 867 ms silence at the start; none after |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN |

## Uncertainty

One PC runs both ends, so its own stalls are in both players' numbers. Whether a gap of 867 ms happens between two PCs at all is not known; no buffer under 8 snapshots hides one. How the glide looks has not been judged by a person.

## Verdict

PARTIAL. Skips meet the target; the freeze target fails only inside one stall that nothing reached the joiner through.
