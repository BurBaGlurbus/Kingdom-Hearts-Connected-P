# KHC-064: 5-letter room codes

| | |
|---|---|
| Gate | burba: "all 5 letter codes" |
| Owner | burba |
| Prerequisites | KHC-051 (direct codes), the relay (Brief 3) |
| Source | none read |
| Wire | PROTOCOL 15: `CODE_LENGTH` 6 to 5; the relay's asks padded by 9 bytes, not 8 |

## What was done

- Room codes from the relay are 5 letters or digits (no 0, O, 1, I or L): 31^5, about 28.6 million codes.
- PROTOCOL 15: both players and the relay need the new build.
- The relay's Register and Lookup carry one more padding byte, so its answers stay no larger than what asked (no amplification).

## What it does not cover

A room code with no server (KHC-051) carries the host's internet address and port: 6 bytes, so 12 characters. It cannot be 5 letters. A 5-letter code needs a server that knows which host it means: the relay (set under Options, Relay server). With no relay set, Host a Room still gives the 12-character code. burba decides on a server.

## Checks

| Check | Group | Verdict |
|---|---|---|
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings` | Agent | PASS |
| `cargo test --locked -p net -p relay -p khc` (the relay's loopback sessions included) | Agent | PASS |
| A duel through a relay on two PCs | Local | NOT RUN |

## Verdict

PARTIAL. 5-letter relay codes work in tests; serverless codes stay 12 by necessity.
