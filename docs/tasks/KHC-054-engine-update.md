# KHC-054: the engine brought up to date

| | |
|---|---|
| Gate | None moves. Single player in this engine's room takes the newer engine work; multiplayer keeps working as it did |
| Owner | burba |
| Prerequisites | KHC-005 (the engine copied into this repository) |
| Source | The engine's later work, copied and merged by hand into this repository's own copy (no dependency, no link). The player's Steam KH2FM and KH3 installs, only read |
| Wire | PROTOCOL 14, unchanged. Three new action codes (below) |

## What was done

The engine here was copied at KHC-005 and had not moved since. Every later engine change was merged into this copy file by file (a three-way merge from the copy's starting point), so this repository's own edits stay: 111 files merged cleanly, 34 files are new, 10 files had overlapping edits and were merged by hand. Environment variables in the new code are `KHC_*`, data folders are `%LOCALAPPDATA%\KHC`, and window titles stay Connected's.

What came in, by area:

- **Running cost**: 60 FPS by default with one tick a frame, a limiter that sleeps instead of spinning, single-threaded schedules with one thread per task pool, no baked poses (party, map objects and Keyblades are evaluated when shown), skinned vertices uploaded on their own, the HUD canvas drawn tile by tile, kept effect meshes and one staging belt, memory trimmed after a load, the save point's glow no longer growing. KHC.exe's own launcher now uses the same frame limit and low-cost settings as the engine's.
- **Engine fixes**: anti-aliasing no longer quits Twilight Town, resizing no longer quits (fullscreen, borderless, window), the 2D slots' rewrites ordered before the asset events.
- **Single player**: party weapons hidden until combat and put away after; the L1 shortcut menu; boss fights from a list, mission scripts, flying root motion, Port Royal medals; saves that hold the game, every world open on the map, doors between worlds, each world's party; townspeople motion and talk text; debug outfits and crowns; chests, prizes, the box window; story events behind a switch; the item shop; Tinker Bell; Gummi records, rules, menus and the stage file reader; IK chain blends and the PS2-matched motions; the Keyblade mounted without the axis flip; the stance model (motion 6, the battle flag, lowered field walk and run).
- **Linux build support** (the `cfg` code and its target dependencies), untested on Linux, as it came.

Left out: the Skate 3 skateboard and its disc screens, the title screen, the engine's own program entry points and research probes, and its tools, CI, build scripts and documents.

## Connected's own code, changed to fit

- Poses are no longer baked: `kh3_model.rs`, `rival_play.rs` and `hub_avatar.rs` read `ClipPoses::posed`, `weapon_frame` and `len` instead of `joints`, `weapon_frames` and `weapon2_frames`.
- **The stance in a duel** [Duel choice, KHC-054]. The engine now holds the weapon drawn while the room's battle flag is up and puts it away at the next idle loop otherwise, as the original does; the old 120 idle ticks are gone. A duel has no enemies to raise the flag, so a rival in the room raises it for Sora, and every rival's tick raises it for that rival: both Soras stay armed for the whole match, as Sora does in any battle. The lobby's and the match room's own `armed = true` stay.
- `duel_wire` carries the new actions as codes 20 (`StanceSwitch`), 21 (`LoweredWalk`) and 22 (`LoweredRun`). A build from before this reads them as unknown, so both players need this build, as for any engine change.
- A duel's hit sparks keep the first Keyblade's container (`second: false`), as before.
- `combat::sora_veto` is `combat::kill_veto`, the same rule.
- `Player::restart_clocks` zeroes the new frame and blend clocks in place of the idle count.

## Evidence

None private: this is a build check only.

## Checks

| Check | Verdict |
|---|---|
| `cargo check --all-targets --offline` | PASS |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --all-targets --locked --offline -- -D warnings` | PASS |
| `cargo build --locked --offline --release -p khc` | PASS: `target\release\KHC.exe` |
| `cargo test --locked` | NOT RUN: kept to a build at the owner's request |
| Single player, a duel (loopback or two PCs), the hub, the launcher's screens | NOT RUN |

## Uncertainty

Nothing was run, so the merged single-player work and the duel's stance were not seen in play. A clean three-way merge is not proof that two changes fit: Connected's room code may rely on engine behaviour that moved (the 60 FPS tick lock against the duel's playout clock, the single-threaded schedules against the net threads) in ways a build cannot show. The new Gummi, shop and chest code was not tried from KHC.exe. Program-to-program saves (`KHC_CHAIN`) came with the code but are not wired to KHC.exe's own entry point.

## Verdict

PASS for building; NOT RUN for play.

## Next

Burba plays a loopback duel and a little single player on this build. Later engine work is merged the same way; this merge took everything up to the engine's T379.
