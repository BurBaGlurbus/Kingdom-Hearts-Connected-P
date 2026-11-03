# KHC-057: Config in the hub's pause menu

| | |
|---|---|
| Gate | burba: a Config menu in the pause menu like KH2's, with the graphics, audio and other settings |
| Owner | burba |
| Prerequisites | KHC-056 |
| Source | The player's Steam KH 1.5+2.5, only read, at run time (the Game page's text is the game's own) |
| Wire | none |

## What was done

- The hub's pause menu has a **Config** entry where it had Hub. Its pages, in order:
  - **Game**: KH2's own Config page in the game's text (Field Camera, Camera Up/Down, Camera Left/Right, Summon Effects, Navigational Map, Vibration, Command Menu). Left and Right change the save's config bits, as the room's own Config page does; they are kept when the menu closes.
  - **Graphics**, **Sound**, **Other**: the room's settings pages (display, frame rate, custom limit, resolution, anti-aliasing, brightness, motion smoothing; master, music and effects volume, background sound; text language, HUD, camera speed). Changes apply at once and are saved when the menu closes, as the room's own camp does.
  - then the hub's: KH3's ability pages (when the rules allow KH3), Say, Players, Card, Friends, Hub settings (renamed from Settings) and Rules.
- The Game page shows a placeholder row until the camp's text has loaded (the first opening), so the pages after it keep their places.
- [Test] `KHC_HUB_MENU_PAGE=NAME` with `--hub-menu TICK` opens the menu on that page.

## Results

Captures (private, `runs\brief6\cfg2-Game`, `cfg-Graphics`, `cfg-Sound`, `cfg2-Other`) show each page with the game's own menu art and its values.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS |
| Captures of Game, Graphics, Sound and Other | Local | PASS |
| A setting changed by hand applies and is kept after closing | Local | NOT RUN: no presses in an unattended run |
| Text Language changed from the hub's menu | Local | NOT RUN: the language swap reloads the menus, and with the hub's own camp that path was not tried |

## Verdict

PARTIAL. The pages are there and read right; nothing was changed by hand.
