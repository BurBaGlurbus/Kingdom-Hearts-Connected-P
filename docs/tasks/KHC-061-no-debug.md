# KHC-061: No debug mode in Connected

| | |
|---|---|
| Gate | burba: "get rid of debug mode for kh connected" |
| Owner | burba |
| Prerequisites | KHC-060 |
| Source | The player's Steam KH 1.5+2.5, read at run time. One Windows PC |
| Wire | none |

## What was done

- The engine's `Game` has `no_debug`; Connected's room sets it for every session (duels, practice, replays).
- With it set: no F3 debug menu (nor the both-sticks press that opens it) and no F2 help, no "F3 Debug menu  F2 Help  Enemies" line at the top, no F1 collision view, and no F-key aids (F4 blow hit, F5..F9 HP/MP/Drive, F11 Drive refill, F12 stand-in party).
- Unattended runs that drive the debug pages themselves (tours) still work: they do not read the keys.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings` | Agent | PASS |
| `cargo test --locked` after this change | Agent | NOT RUN |
| A 420-tick `--kh2-test` capture: no status line at the top | Local | PASS |
| F3 pressed by hand | Local | NOT RUN |

## Verdict

PARTIAL. The status line is gone in a capture; the keys not pressed by hand.
