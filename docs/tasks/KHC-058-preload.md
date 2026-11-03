# KHC-058: Preload everything

| | |
|---|---|
| Gate | burba: the launcher asks whether to preload everything, so nothing stutters when it first loads |
| Owner | burba |
| Prerequisites | none |
| Source | The player's Steam KH 1.5+2.5, Birth by Sleep and KH3, only read, at run time. One Windows PC, loopback |
| Wire | none |

## What was done

- The launcher asks once, on its first screen after this change: "Preload everything when a game starts?" with Yes and No, and keeps the answer (`launcher.json`, `preload`). The Multiplayer screen has a Preload row to change it. With it on, every game the launcher starts (duels, the hub, Test KH3, practice) gets `--preload`, and a restart from the hub keeps it.
- `room/preload.rs`, with `--preload`: before play, with Sora standing and "Preloading ..." on screen naming what it waits for:
  - every spell effect's models and textures (the duel warmed them one file a frame during the first countdown; preloading does all at once);
  - the Mirage Arena, in a duel or on a hub that stands on it;
  - KH3's data, sounds, effects and HUD, in a duel on a PC with KH3 (it already starts with the session; preloading waits for it);
  - the match's Struggle art and cup sounds, which waited for the lobby.
- [hub choice, KHC-058] Preloading gives up after 45 seconds and play starts.

## Results

| Run | What | Preload took |
|---|---|---|
| `preload` (the hub alone) | 18 effect files | under a tick |
| `preload-duel` joiner | effects, arena, KH3, match art | 779 ticks (about 13 s, mostly KH3) |
| `preload-duel` host | the same | still waiting for KH3 at tick 899, when the run ended |

Both players read KH3 at once on one PC, which is slower than one would be.

## What it does not cover

GPU shaders compile the first time something is drawn, and nothing here draws ahead; a first effect may still cost a frame. The first frame of a KH3 effect, the enemy assets of a world (the hub and duels use none), and music tracks (decoded on their own thread already) are not preloaded.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS |
| Preload in the hub and in a loopback duel: what it waits for, and done | Local | PASS for the hub and the joiner; the host not done within the run (KH3 on both at once) |
| The launcher's question and the Preload row on screen | Local | NOT RUN: the launcher was not captured |
| Hitches before and after, compared | Local | NOT RUN |

## Verdict

PARTIAL. Built and seen working; no before-and-after measure of hitches.
