# KHC-041: Hub mode and its map

| | |
|---|---|
| Gate | Brief 6, WP-20: walk an empty hub with no combat; decide where its map comes from |
| Owner | burba |
| Prerequisites | none |
| Source | The player's Steam KH 1.5+2.5 and Birth by Sleep (the Mirage Arena), only read, at run time. One Windows PC. Builds pinned privately as `b6hub`, `b6hub2`, `b6hub3` |
| Wire | none |

Brief 6 numbered its records KHC-031 to KHC-040; 031 to 034 were already taken (Briefs 1 and 2) and Brief 5 used 016 and 030, so Brief 6 uses KHC-041 to KHC-050 in its package order (WP-20 = KHC-041 ... WP-29 = KHC-050).

## What was done

- `--hub` (`room/hub.rs`, `Online::hub`): Sora alone in the Mirage Arena, no enemies, party, summons or second deck. The room's pad goes through `own_pad`, where hub mode keeps the stick and Jump and drops Attack, Guard, KH3's two buttons and every command (spell, item, Drive form); a form asked for from the menu is cleared. Reaction Commands have nothing to offer: the Mirage Arena carries none of the room's objects.
- Sixteen spawn points on two rings (450 and 1100 from the centre); Sora goes to the point of his hub seat once the arena is shown. Invisible walls: `duel_kh3::contain` holds him 2000 from the centre, inside the arena's wall ring (about 2160).
- `mirage_arena.rs` loads and shows the arena for the hub too (it waited for a duel session before).
- The follow camera is unchanged.

## What the engine's loaders can do (step 1)

- `area.rs` loads a KH2 field area by name from the selected install, as the exe's area loader does: its map, ARD scripts, story programs and spawn points. So a KH2 area is possible in principle, but it brings its scripts, exits and townspeople, which the hub would have to switch off one by one.
- `npcs.rs` spawns an area's townspeople (objentry type 15) and walks them on the exe's clock.
- `world_map.rs` is the world map program (area wm00): not a place to stand.

[owner question, KHC-041] The map is the Mirage Arena, the brief's default: it already loads reliably from a player's install for the duel, with nothing in Git. The step 2 comparison of three candidates by captures was NOT RUN; only the Mirage Arena was captured.

## Results

| Run | Frames | 1% low | p99 |
|---|---|---|---|
| `empty-32`: the hub alone | 60 | 53 | 18.8 ms |

The capture shows Sora on a spawn point in the Mirage Arena with the follow camera behind him. The command menu still shows (its commands do nothing): hiding it is open.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief6\`: `empty-32` (the hub alone), `bots-32` (`--hub-bots 32`), `bots-4` (a projection check), `net-8` (`khc-hub` on loopback, `khc-hub --bots 8`, one game client). Runs: `scripts\brief6_runs.ps1 -Kind empty|bots|net -Bots N`; build: `scripts\brief6_build.sh NAME`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Hub mode refuses every combat input in a table of 1,280 pad states | Agent | PASS |
| Spawn points inside the walls and the server's bounds, apart from each other | Agent | PASS |
| Captures of the empty hub from two angles | Local | PASS for one run's two captures (ticks 600 and 1200; the camera turned between them) |
| 1% low within 5% of the duel arena | Local | NOT RUN: no duel run was measured beside it |
| Compare three candidate maps by captures | Local | NOT RUN |
| The owner picks the map | Owner | NOT RUN |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Uncertainty

Only the follow camera's default was seen; no camera problems showed in two captures, which is not a search for them.

## Verdict

PARTIAL. Hub mode works on the Mirage Arena; the map comparison and the frame-rate comparison with a duel were not run.
