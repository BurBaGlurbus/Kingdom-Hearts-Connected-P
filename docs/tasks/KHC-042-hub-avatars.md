# KHC-042: Avatars, light puppets

| | |
|---|---|
| Gate | Brief 6, WP-21: many other players drawn as light puppets with smooth motion |
| Owner | burba |
| Prerequisites | KHC-041 |
| Source | The player's Steam KH 1.5+2.5 and Birth by Sleep (the Mirage Arena), only read, at run time. One Windows PC. Builds pinned privately as `b6hub`, `b6hub2`, `b6hub3` |
| Wire | the hub's own (KHC-045) |

## What was done

- `room/hub_avatar.rs`: an `Avatar` is a name, id, level, status, title, a short history of moves and a presenter. No `Fighter`, no `Own`, no `Rival`, so no effect keys and no two-seat arrays.
- Each avatar is a copy of the room Sora's meshes, posed from the room's own clips (`rival_kit`) unarmed, with an eight-frame crossfade on a motion change. [hub choice, KHC-042] Skinned poses are kept by (clip, frame) and shared by every avatar, and a mesh is only rewritten when its frame changes.
- Only Idle, Walk, Run, Jump, Fall, Land and HighJump arrive, plus six emote codes from 128; anything else is dropped. Emotes stand still until the owner picks their motions.
- Interpolation: drawn 100 ms behind, carried on up to 200 ms when updates stop, snapped past 600 units. Three identical updates in a row keep two, so a stop is not carried past.
- At most 32 drawn, nearest first.
- `--hub-bots N`: wanderers with no network, from the hub crate's `Walk`, reported 10 times a second as the network would. They use every allowed motion and an emote now and then.

## Results

| Run | Frames | 1% low | p99 |
|---|---|---|---|
| `empty-32` (the hub alone) | 60 | 53 | 18.8 ms |
| `bots-32`, first build | 41 | 38 | 25 ms |
| `bots-32`, poses shared | 59 to 60 | 30 to 31 | 32 ms |
| `net-8` (eight network avatars) | 60 | 51 | 19.2 ms |

Sharing the poses brought the average back to 60, but the 1% low is 30 against 53: the target (within 10%) FAILS. Where the slow frames come from was not measured.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief6\`: `empty-32` (the hub alone), `bots-32` (`--hub-bots 32`), `bots-4` (a projection check), `net-8` (`khc-hub` on loopback, `khc-hub --bots 8`, one game client). Runs: `scripts\brief6_runs.ps1 -Kind empty|bots|net -Bots N`; build: `scripts\brief6_build.sh NAME`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Interpolation: steady, jittery, lossy and teleporting input; a stop is not carried on | Agent | PASS |
| A disallowed action id is ignored; the hub carries a player's own action as an allowed one | Agent | PASS |
| A wanderer stays inside and uses every allowed motion | Agent | PASS |
| 32 bots keep the 1% low within 10% of the empty hub | Local | FAIL: 30 against 53 |
| Captures show crossfades with no pops | Local | NOT RUN: still captures cannot show a pop |
| Every avatar KH2's Sora | Owner | Default in force |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Uncertainty

A looping motion's frame is wrapped by its length, not by the exe's loop range, so a loop may restart at the wrong frame. The remote frame is the sender's, carried on at 60 a second between updates.

## Verdict

PARTIAL. Avatars draw and move; the 1% low with 32 FAILS its target.
