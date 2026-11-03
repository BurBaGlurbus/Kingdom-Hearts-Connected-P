# KHC-023: Replays

| | |
|---|---|
| Gate | Brief 1, WP-12: save a match and play it back without simulating it again |
| Owner | burba |
| Prerequisites | KHC-024 (the comparator) |
| Source | The player's installs, only read, at run time. A replay holds only ids and numbers from the session, and goes in the `replays` folder beside the settings, never in Git |

## What was done

- `source/khc/src/room/duel_replay.rs`. A `.khcr` file is a header (file layout, `PROTOCOL`, executable hash, both names, the rules with each seat's ruleset, the KH3 install hash when a Sora uses KH3), then records of tick, channel and a length-prefixed message.
- `net::Session` keeps a copy of what the joiner's path gets: the host's sends, the joiner's receives (`set_tap`). So either player's game can save. Before the first match only the lobby's reliable messages are kept.
- Each session with a match saves a file. `--replay-out` names it. An unattended run saves none unless told to.
- `--replay <file>`: `Session::playback` is a session with no socket. It feeds the saved messages to the joiner's own code (`join_apply`, the lobby, the match, the music) at 60 Hz, and nothing is simulated. The lobby's messages go in at once, and play starts at the first snapshot once KH3's data is in.
- Space pauses, the full stop steps one tick while paused, and 1, 2, 3 are x0.5, x1, x2. A fast speed drops snapshots but no reliable message.
- Another `PROTOCOL` or KH3 install is refused with the reason, at the launcher and at the room. Another build plays with a warning on the overlay. A KH2 install is not hashed in the header: a replay on a different KH2 install may look wrong with no warning.
- *Replays* on the launcher's Multiplayer screen lists the newest 30.
- [duel choice, KHC-023] The camera is always the joiner's, whichever side saved, because it is the joiner's code. A file's end shows a message and stays until Esc.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief1\last\`, each `replay-*` folder beside its run. Script: `%USERPROFILE%\kh2-kh3-ref\scripts\brief1_replay.ps1`.

| Match | File | Against the original joiner's log | Final numbers |
|---|---|---|---|
| KH2 host vs KH3 joiner | Joiner's | SAME, 69 events, offset up to 3 ticks | Same |
| KH3 host vs KH2 joiner | Joiner's | SAME, 69 events, offset up to 27 ticks | Same: wins [1,1], dealt [170,447], hits [38,26] |
| KH3 host vs KH2 joiner | Host's | DIFFERENT by the comparator: 70 `duel_score` events, the joiner's log has 68 | Same as above |
| KH2 vs KH2 (build before the last) | Joiner's and host's | SAME, 84 events | Same |

The host's file differs because a real joiner sometimes reads two match messages in one tick and logs only the second, while a replay of the host's file feeds them a tick apart. The final numbers agree.

| File size | 90 s of fight plus the end of a lobby |
|---|---|
| KH2 vs KH3 | 722 KB |
| KH3 vs KH2 | 975 KB |
| KH3 vs KH3 | 1.3 MB |

That is about 0.5 to 0.9 MB per minute of fight, under the target of about 1 MB. The per-tick share is the snapshot; nothing is compressed yet.

## Checks

| Check | Group | Verdict |
|---|---|---|
| File round trip, a cut file, refusal by protocol and by install, warning by build, release order, pause and speed | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: no warnings, 1410 tests passed, 2 ignored |
| `tools\Build-Launcher.ps1`, what `build.bat` runs | Agent | PASS |
| Frame rate within 5% of the run before the change (`--perf-overlay`) | Local | NOT RUN: the flags are off in play, but this was not measured |
| A scripted loopback match recorded and replayed: same score and events under the comparator | Local | PASS from a joiner's file, in two matches. From a host's file: same final numbers and event kinds, two extra intermediate scores |
| A file from another protocol refused with the reason (a patched copy) | Local | PASS: "this replay was saved with duel protocol 99, and this game speaks 2" |
| About 1 MB per minute or less | Local | PASS: 0.5 to 0.9 MB per minute of fight |
| The launcher's Replays row, Space, the full stop, 1, 2, 3 by a person | Owner | NOT RUN |

## Uncertainty

A replay's look depends on the player's install. Lobby rows and the player's own menus can still open in a replay, because it is the joiner's code; pressing them sends nothing. Whether a replay looks right was judged by its log, not by watching it.

## Verdict

PASS by logs on loopback for the Agent and Local lines. The launcher's row and the keys are NOT RUN by a person.
