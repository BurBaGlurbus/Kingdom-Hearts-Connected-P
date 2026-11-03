# KHC-028: Crash reports

| | |
|---|---|
| Gate | Brief 1, WP-17: when either PC crashes, the other player is told why, and the crashed side leaves a report |
| Owner | burba |
| Prerequisites | None |
| Source | No game data. Tested with the player's installs, only read, on one PC over loopback |

## What was done

- `source/khc/src/crash.rs`: a panic hook, installed first thing in `main`. It writes `crash\crash-<time>.txt` beside the settings: message, location, thread, time, version, build hash, rules, both seats' rulesets, the last ten seconds of `duel_*` events and the last ten network lines.
- Scrubbing: the profile folder, the user and computer names and both player names are removed, then any home folder's name and any IPv4 address.
- The room feeds it through a new `RoomHooks::note_event`, which is a no-op for a room with no extension, so single player is unchanged.
- **Wire change:** `Datagram::Stopped` and `PROTOCOL` 2. No other wire change was pending: the other threads' commits in this period changed no message.
- [duel choice, KHC-028] The notice is a datagram sent three times over 60 ms past the simulator, not a message on the reliable channel as the brief says. Renet's state belongs to the panicking thread, and a panic may have left it mid-change; the hook holds its own handle on the socket and needs nothing else.
- The report is written first, then the notice goes. An unnamed helper thread's panic is reported, but the other player is not told: its caller gets the join's error and the game goes on. Only the first fatal panic is reported.
- The other side shows "Opponent's game stopped on an error", in the status balloon and on the result. It has its own Sora back, as when a player leaves. The brief's wording has parentheses, which the game's font does not draw.
- F3 > Sora has *Crash on purpose* in a debug build. `--duel-crash-at TICK` does the same in any build.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief1\`: `crash-join\`, `crash-host\`, `crash-final\` (with the joiner's captures in `join-cap\`). Reports in the `crash` folder.

| Crashed side | Report written (unix ms) | Other side's `net_disconnected` (unix ms) | Time |
|---|---|---|---|
| Joiner | 1793556649992 | 1793556650004 | 12 ms |
| Host | 1793556697804 | 1793556697814 | 10 ms |
| Host, last build | 1793574113518 | 1793574113520 | 2 ms |

## Found by it

During an unattended KH2 vs KH3 run alone, the hook caught a real panic: `kh3_sora: the sheets are not installed` at `source\kh3_sora\src\sheets.rs:44`, in 1 of about 100 runs, before the script's start. It looks like KH3's rules being read before KH3's data is installed, a race with the loading thread. **Open, not fixed.** The report and log are in `t3-1\solo-h2j3-1\`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Report formatting, path and name scrubbing, ten seconds kept | Agent | PASS |
| `Stopped` round trip, told apart from a timeout both ways, the shown text | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: no warnings, 1410 tests passed, 2 ignored |
| `tools\Build-Launcher.ps1`, what `build.bat` runs | Agent | PASS |
| Frame rate within 5% of the run before the change (`--perf-overlay`) | Local | NOT RUN: the flags are off in play, but this was not measured |
| A crash on one instance writes the report there; the other hears it within 1 second, both ways | Local | PASS: 2 to 12 ms, using `--duel-crash-at` |
| The joiner shows the message and has its own Sora back | Local | PASS by capture, read by Claude, on the build before the wording left its parentheses (they were not drawn) |
| The new wording on screen | Local | NOT RUN: the capture has the old wording, unit test only |
| The F3 row, pressed in a debug build | Local | NOT RUN |
| Two PCs | Owner | NOT RUN |

## Uncertainty

A panic while the notes are being written leaves the report without them (it does not block). An abort that is not a panic, such as a stack overflow or a graphics driver fault, runs no hook and still looks like a timeout. Whether two PCs behave the same is not known.

## Verdict

PASS on loopback for the Agent and Local lines, by `--duel-crash-at` in place of pressing the F3 row.
