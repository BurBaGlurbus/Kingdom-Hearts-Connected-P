# KHC-059: No hub; a menu launcher in the 1.5+2.5 settings style

| | |
|---|---|
| Gate | burba: remove the hub completely; everything through the menu, loading into the room only for a duel; the launcher in the KH 1.5+2.5 menu style with Quick start, preloading, menu sounds, simpler options, room codes first |
| Owner | burba |
| Prerequisites | KHC-041..058 (what is removed), KHC-044 (profile, kept) |
| Source | The player's Steam KH 1.5+2.5 `Image/SettingMenu.hed/.pkg`, read at run time only. One Windows PC |
| Wire | none (PROTOCOL 14 stays: Hello/Welcome still carry the player id) |

## What was done

- The hub is gone: `source/hub` (server, bots, levels, cards, social), the room's `hub*.rs` (Waypoint Plaza, avatars, tags, hub deck, hub pause menu), `--hub`, the hub's Mirage Arena use, Back to hub in the match room and the engine's `hide_hud`. The records KHC-041..043 and 045..050, 052, 055..057 describe removed work and are marked superseded in the index.
- Kept: the player's id, recovery code and name rules (`source/khc/src/identity.rs`, `names.rs`, `blocklist.txt`, moved from the hub crate), `profile.json`, and preloading (KHC-058), which now waits for the arena only in a duel.
- The launcher (`launcher.rs`, `launcher/collection_menu.rs`):
  - Drawn with the 1.5+2.5 settings menu's own background, title band, row bars and pointer, read from the install when the launcher starts (on a thread). Nothing is copied or cached. Without them (no folder yet, or the package differs) the same layout is drawn in plain shapes.
  - Menu sounds from that menu's sound bank: cursor, decide, back, refused.
  - Quick start on the first launch (and from Options): the game found and checked, KINGDOM HEARTS III (optional), Preload on/off, the name, Start. The controls stand beside it.
  - The menu: Join with a room code, Host with a room code, Local network (host, hosts found, join by address), KH3 practice, Replays, Options, Quit.
  - Options: Name, Controls, Preload, Rules code, Relay server, KINGDOM HEARTS III, Game folder, Quick start, recovery code, restore, reset identity, delete profile.
  - Each row's setting stands right of its bar and the cursor's description at the bottom, as in the 1.5+2.5 menu.

## Evidence

- [installed-build observation] SettingMenu has 118 entries; English layout entry 74 (TIM2 0: 2048x2048 background whose picture is 1920x1080 at (64, 484); TIM2 1: the 2048x1388 atlas), sound bank entry 78. The launcher checks the entry count, the layout's magic and both picture sizes before using them.
- [inferred] Which sound is cursor (7), decide (2), back (3) and refused (4) is guessed from their lengths; not traced in the game's code or checked by ear.
- Private dumps and probes: the research folder's `setting_dump`, `setting_png`, `sedbprobe`, `timconv` (outside the checkout).

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS |
| The menu on screen with the install's art (a timed launcher capture, `KHC_LAUNCHER_SHOT_AT`) | Local | PASS for the main menu; Quick start, Options and the plain fallback not captured |
| The sounds by ear | Local | NOT RUN |
| A duel started from the new menu | Local | NOT RUN |

## Verdict

PARTIAL. Built; the look and sounds come from the install; play from the new menu not run.

## Next

burba's look at it; the sound choice by ear; KH3's original pause menu (KHC-056's open item) stays open.
