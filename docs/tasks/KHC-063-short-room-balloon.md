# KHC-063: A shorter balloon while hosting a room

| | |
|---|---|
| Gate | burba: the dialogue box when hosting a room is too big; condense it |
| Owner | burba |
| Prerequisites | KHC-051, KHC-053 |
| Source | The player's Steam KH 1.5+2.5, read at run time. One Windows PC |
| Wire | none |

## What was done

- A host with a room code shows two short lines instead of four long ones: "Room code XXXX-XXXX-XXXX" and "Stuck? Type their reply code" (or the reply code being typed, or what came of it). "Waiting for a player to join." and the router note ("The router did not open the port: turn on UPnP ...") are no longer shown; the note is still in the log (`duel_room_note`).
- A joiner's balloon: "Joining room ...", then "Not in? Send your friend <reply code>" on one line.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy -p khc --all-targets --locked -- -D warnings`, `cargo test -p khc --locked` | Agent | PASS |
| A 560-tick `--host --room` capture: the balloon is about half the screen's width, two lines | Local | PASS |
| The joiner's balloon on screen | Local | NOT RUN |

## Verdict

PARTIAL. The host's balloon seen condensed; the joiner's not captured.
