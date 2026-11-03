# KHC-055: The hub's command deck

| | |
|---|---|
| Gate | burba: in the hub, no HUD but the command deck; Search match first, the elapsed time over the deck on KH2's Triangle prompt, Cancel search while searching; Rules second, as a concise submenu; the other rows proposed |
| Owner | burba |
| Prerequisites | KHC-041 to KHC-052 |
| Source | The player's Steam KH 1.5+2.5, only read, at run time. One Windows PC, loopback |
| Wire | none (the hub's `Pad` message carries the search) |

## What was done

- The engine's room gains `hide_hud`: no gauge, portrait, HP, MP or Drive gauge. The hub sets it; the command deck stays. The deck swap and the L1 shortcuts are off in the hub.
- `room/hub_deck.rs` takes the deck's presses before the room's tick and runs KH2's own deck with the hub's rows:
  1. **Search match**: looks for a match with the deck's rules. While searching the row reads **Cancel search**, and the reaction slot over the deck shows **Searching 0:14** on the Triangle prompt; Triangle or Cancel search stops it. [hub choice, KHC-055] The hub's queue now reaches every instance, so a search finds anyone else searching (or standing on a pad).
  2. **Rules** (Magic's list art): Preset, Ruleset, Mode, Time (greyed unless Time limit), Damage (x0.5 to x3) and Arena. Decide changes the row and the list stays open on it. The rules are kept in `profile.json` as a rules code and go with a search or a challenge.
  3. **Say** (Items' list art): the preset phrases.
  4. **Menu**: the hub's pages in the pause menu (players, cards, friends, settings, emotes, Leave hub).
- The reaction slot also offers, on Triangle, a challenge waiting (it opens the menu on Accept), a door nearby (goes through), or a player within 300 units (their card). Attack no longer opens cards or doors: it is the deck's Decide.
- [Test] `--hub-search` starts one search once the hub has the player.

## Results

Run `deck` (private, `runs\brief6\deck\`): `khc-hub`, three network bots and one client with `--hub-search`. The capture at tick 900 shows no gauge or portrait, the deck reading Cancel search, Rules, Say and Menu, and **Searching 0:14** on the Triangle prompt over it.

## Checks

| Check | Group | Verdict |
|---|---|---|
| The clock's text; every Rules row changes, comes round, stays valid and short | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS |
| Capture: the deck alone, Cancel search, the elapsed time on Triangle | Local | PASS |
| The Rules and Say lists on screen; Triangle at a door or a player | Local | NOT RUN: an unattended run does not press them |
| A search matching two players | Local | PASS for the queue by the pads run of KHC-048; two searches across instances not run |
| The rows feel right | Owner | NOT RUN |

## Verdict

PARTIAL. The deck works and was seen once; its lists and the Triangle offers other than the search were not seen.
