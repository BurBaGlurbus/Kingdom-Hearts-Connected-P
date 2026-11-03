# KHC-060: Menu names and Practice for both rulesets

| | |
|---|---|
| Gate | burba: "Join a Room", "Host a Room", "LAN Host"; Practice as a submenu to practise KH2 or KH3 combat |
| Owner | burba |
| Prerequisites | KHC-059 |
| Source | The player's Steam KH 1.5+2.5 (and KH3 for KH3 practice), read at run time. One Windows PC |
| Wire | none |

## What was done

- The menu reads Join a Room, Host a Room, LAN Host, Practice, Replays, Options, Quit.
- Practice opens a submenu: KINGDOM HEARTS II or KINGDOM HEARTS III (greyed out, with the reason, when KH3 is not set).
- `--kh2-test` (new): the KH3 practice's setup under the KH2 ruleset: Sora alone with the duel's build, the room's Shadow standing as the training dummy in front of him. KH3 practice is unchanged (`--kh3-test`).

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy -p khc --all-targets --locked -- -D warnings` | Agent | PASS |
| A 420-tick `--kh2-test` run (private research folder `runs/khc060/kh2`): duel build applied, dummy placed, KH2 deck and gauge on screen | Local | PASS |
| The dummy seen in the capture | Local | FAIL to show: it is logged in place but not visible at tick 300 (the camera faces the other way after the scripted run) |
| The submenu on screen, by hand | Local | NOT RUN |

## Verdict

PARTIAL. Works as started from the command line; the submenu not driven by hand.
