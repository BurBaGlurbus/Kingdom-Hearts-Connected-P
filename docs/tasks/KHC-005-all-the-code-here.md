# KHC-005: all the code in this repository

| | |
|---|---|
| Gate | None; the owner's decision of 2026-10-31: this project is to stand alone, and needs no Skate 3 code |
| Owner | burba |
| Prerequisites | KHC-004 |
| Source | The engine crates at the commit this project was last tested against; the player's Steam KH2FM and KH3 installs, only read |

## What was done

- The engine's crates are now part of this workspace: `source/engine` (the room and the launcher's parts; package and library `engine`), `source/visual` with `effect`, `source/audio`, `source/data`, `source/validation` and `source/game/*`. `Cargo.toml` takes them by path. There is no git dependency, no pin and no folder override, and `Cargo.lock` has no git source.
- Left out: the Skate 3 skateboard entirely (its crates, `skate.rs`, `skate_rig.rs`, `skate_hud`, the J key, the launcher's disc screens and setting), the title screen and its banner art, the engine's own program and viewer entry points, and the research probes and examples.
- `keyblade_board.rs` is `keyblades.rs`: the item-to-model lookup the Drive code uses, without the board's placement.
- The names of the old project are gone from code, window titles, environment variables (`KHC_*`) and data folders, which are now `%LOCALAPPDATA%\KHC`. Task records were reworded to match, and the records of the split (KHC-001 to KHC-003) were removed; Git history keeps them.
- `NOTICE` and `CREDITS.md` keep every third-party notice the code needs (OpenKH and the others), with their licence texts in `docs/release/`.

Single-player parts of the room stay, because the duel runs inside the room and they share its code.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\`: `stand1` (Test KH3 mode), `standduo1` (two instances over loopback).

## Checks

| Check | Verdict |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --all-targets --locked --offline -- -D warnings` | PASS |
| `cargo test --locked --offline` | PASS |
| `tools\Build-Launcher.ps1`, which `build.bat` runs | PASS: `target\release\KHC.exe` |
| Test KH3 mode, unattended, release: the deck's Waterga, Thundaga, Potion and Curaga | PASS by the log of `stand1` |
| Loopback duel, release: the same from the joiner, on the host's slot 1 Sora | PASS by the logs of `standduo1` |
| A text search of the tree for the old project's names and the skateboard | PASS: only the game's own `n_ex*_skate_r` script names remain |
| Fetching on a clean machine; GitHub Actions | NOT RUN: `--offline` used the local crate cache, and nothing was pushed |
| The launcher's screens, a real pad, two PCs | NOT RUN |

## Uncertainty

Git history still holds the old names and the earlier `Cargo.toml`, and was not rewritten. The engine is the copy from the last tested commit, so later work in the old project (lower memory use, the shortcut menu's drawing) is not here. Removing more single-player code would need its own pass, since the room's tick calls much of it directly.

## Verdict

PASS for building and running from this repository alone on this PC, within the checks above.

## Next

Burba tries it. The owner decides whether Git history should be rewritten before the repository is shared.
