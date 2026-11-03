# KHC-056: The hub's pause menu edits the duel's builds

| | |
|---|---|
| Gate | burba: START in the hub opens KH2's menu, where abilities, items and the rules can be changed for each ruleset; on KH3's moves, KH3's own original menu |
| Owner | burba |
| Prerequisites | KHC-055 |
| Source | The player's Steam KH 1.5+2.5, only read, at run time |
| Wire | none |

## What was done

- The hub puts Sora on the duel's baseline (`duel_loadout.rs`: level 99, the duel's abilities, items and AP), as a duel session does.
- `room/hub_build.rs`: once the camp's tables are in, the build kept in `duel-prefs.json` goes on Sora, fitted to the duel's limits, and the camp shows the duel's stock, every Keyblade and the duel's AP (as the lobby's `prepare_builds`, with no bans). While the hub's menu is open, what is equipped (Keyblade, item slots, abilities, KH3 abilities) goes on Sora and into `duel-prefs.json`, so the next duel's builds start from it.
- The hub's pause menu (START) is the game's own: Items and Abilities (KH2's, left out when the rules are KH3 only), Hub (the pages) and Leave hub; Accept, Decline and Busy come first when a challenge waits.
- The pages gain Rules (the deck's six rules, Left and Right change them) and, while the rules let this player fight on KH3's moves (KH3 or Free pick), KH3's ability pages first (`kh3_menu.rs`'s nine pages, with their AP), so Hub opens on them.
- `duel_lobby_room.rs`'s `limits_of`, `build_of` and `put_build` are public for this.
- [Test] `--hub-menu TICK` opens the menu at a tick.

## KH3's original menu: not done

KH3's own pause menu is a Scaleform movie with its own ActionScript, as the HUD's three movies were (`source/kh3_ui`, ported method by method for KHC-006 and KHC-007). The menu is a larger set of movies and screens. Until it is ported, a Sora on KH3's moves gets KH3's abilities on the game's KH2-style pages. Porting it is the next step if wanted: find the menu's movies under `/Game/UI/Scaleform/` in the player's paks, port their classes as the HUD's were, and drive them from the same build.

## Results

Run `menu` (private, `runs\brief6\menu\`), `--hub-menu 300`: the capture shows KH2's menu over the hub with Items, Abilities, Hub and Leave hub, and Sora at level 99 with the duel's HP and MP.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS |
| Capture: the menu's top level in the hub | Local | PASS |
| Items and Abilities changed in the hub reach the next duel's lobby | Local | NOT RUN: needs a person at the menu |
| The KH3 ability pages and the Rules page on screen | Local | NOT RUN |
| KH3's original menu | Local | NOT RUN: not built |

## Verdict

PARTIAL. KH2's menu with the build, the rules and KH3's abilities is in; KH3's original menu is not built, and nothing was changed by hand to see it reach a duel.
