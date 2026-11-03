# KHC-046: Levels

| | |
|---|---|
| Gate | Brief 6, WP-25: a cosmetic Level from play, on tags and cards |
| Owner | burba |
| Prerequisites | KHC-044, KHC-045, Brief 5's result screen |
| Source | No game data |
| Wire | the hub's |

## What was done

- `hub::levels`: the curve (100 + 25 x L; 35,525 XP to Level 50, the cap), the awards (finish 50, win 25, first of the day 50, a new ruleset pairing 50, the tutorial once 100), nothing for a forfeit or drop in the first 15 s or for practice, half after three matches between the same two ids in an hour, 600 a day at most. Cosmetic unlocks: titles at 1, 5, 10, 20, 30 and 50, frames, badge shapes, a phrase. "Level", never "Rank".
- The server awards only when both players' reports agree (winner, pairing, forfeit, length within 5 s); a mismatch awards nothing and is logged. [owner question, KHC-046] LAN hubs award nothing, and a public hub with unverified ids keeps levels off.
- The client shows the level and a level-up as toasts. KHC-052's pass: a duel from the hub draws the award's lines over its result ("Finished +50", "Won +25", then "Level 2  25 / 150 XP"); "No XP for this match" when the lines are empty.

## Checks

| Check | Group | Verdict |
|---|---|---|
| The curve, caps, repeats, forfeits, practice and the tutorial, several levels at once | Agent | PASS |
| Matching reports award XP; mismatched ones do not; LAN and unverified public hubs award nothing | Agent | PASS |
| The award's lines | Agent | PASS |
| A rated match on loopback | Local | PASS: `rated 1` in `flow` (KHC-048) |
| A capture of the result screen with the breakdown | Local | NOT RUN: the unattended result moved on before the capture |
| The numbers and the cap | Owner | Defaults in force |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Verdict

PARTIAL. Rules, handshake and the result's lines done, and one match rated on loopback; the lines not captured.
