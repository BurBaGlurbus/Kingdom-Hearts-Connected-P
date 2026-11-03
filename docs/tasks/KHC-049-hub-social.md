# KHC-049: Social features and safety

| | |
|---|---|
| Gate | Brief 6, WP-28 |
| Owner | burba |
| Prerequisites | KHC-043, KHC-045, KHC-047 |
| Source | No game data |
| Wire | the hub's |

## What was done

- `hub::social`: nine preset phrases (one a Level 15 unlock), none over 24 characters, ids unique; six emotes from code 128 ([owner question, KHC-049] their motions are the owner's choice; until then they show a bubble and stand). One bubble per 2 s; spamming mutes a player on the server for a minute.
- Friends: ask, the other side accepts by asking back, unfriend; 100 at most; the server tells online friends when someone comes, goes or changes status. Block ends any friendship or request, hides the avatar, stops their bubbles, cards and challenges, and lasts on the server's record. Mute hides tags and bubbles on the client (`profile.json`).
- Reports: a category and the target's last 60 s of moves (twice a second) and bubbles from a ring buffer, stored as JSON; `khc-hub --reports DIR` lists them. [owner question, KHC-049] No automatic action and no free chat.
- Away by itself after 3 minutes without movement, back on moving.
- Toasts: a challenge, a friend online, a level-up, the hub's state; three at most.
- Remembered: the last instance and the hub settings.
- KHC-052's pass: [hub choice, KHC-049] a list in the game's own pause menu rather than a wheel (START opens it in the hub): Say (the phrases and emotes), Players, Card, Friends (Decide goes to their instance) and Settings (nametags, size, opacity, bubbles, crowd 8/16/32, toasts, high contrast, reduce motion, hide hub text). Mute and block are kept in `profile.json`. The Practice and Tutorial doors on Waypoint Plaza start the training room with a scripted rival and the Test KH3 mode.
- Not built: the last 20 opponents, region and ping on a hub list, the automatic reconnect, a report's match replay; hide hub text hides names, bubbles and toasts but not the game's HUD.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Friend requests, limits, blocks, block persistence across a rejoin | Agent | PASS |
| The report's payload; rate limits; auto-away | Agent | PASS |
| The phrase table's integrity | Agent | PASS |
| Captures of the menu, friends page, settings page and toasts | Local | PARTIAL: toasts captured ("In the hub", "Looking for a match"); the menu pages NOT RUN (an unattended run's presses keep the room's own camp) |
| Who reads reports | Owner | Default in force |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Verdict

PARTIAL. Rules and screens built; the screens not captured or used by a person.
