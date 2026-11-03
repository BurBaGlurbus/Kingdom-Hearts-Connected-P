# KHC-047: Player cards

| | |
|---|---|
| Gate | Brief 6, WP-26: a card for every player |
| Owner | burba |
| Prerequisites | KHC-044, KHC-045, KHC-046 |
| Source | No game data |
| Wire | the hub's `CardAsk`, `Card`, `CardChoices` |

## What was done

- `hub::card`: `CardData` (name and number, Level and XP, title, frame, favorite Keyblade by number, favorite ruleset, wins and losses by KH2, KH3 and mixed, joined month and year, status, last seen), version 1, under 256 bytes. Each field Public, Friends or Hidden; [owner question, KHC-047] the default is everything public but status and last seen, which are for friends. The server builds the card for its viewer and leaves hidden fields out. Choices a level has not unlocked are put back.
- The card's lines for the full card and the small one.
- KHC-052's pass: the hub menu's Card page shows a player's card and offers Challenge, Add friend, Mute, Block and Report; Attack within 300 units of a player opens it, and the Players page lists the nearest twelve. Cards are kept 10 minutes. A duel from the hub draws both players' small cards over the lobby and the first countdown (`hub_card.rs`). Favorite Keyblade shows as text (`Keyblade 7`): the install's icon and name are not read.
- Not built: "Preview my card" and the visibility settings page (the choices are in `profile.json` and the server keeps them).

## Checks

| Check | Group | Verdict |
|---|---|---|
| Round trips; another version refused | Agent | PASS |
| Visibility: a stranger, a friend, the owner, levels off | Agent | PASS |
| Layout with the longest and shortest names | Agent | PASS |
| Captures of the full card, the small card, the intro | Local | PARTIAL: the small cards in the duel lobby captured (`flow`); the full card page and the intro NOT RUN |
| Default visibility | Owner | Default in force |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Verdict

PARTIAL. Cards are built, sent and shown in the hub menu and the duel lobby; only the small cards were captured.
