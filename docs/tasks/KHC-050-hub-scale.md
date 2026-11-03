# KHC-050: Scale and performance

| | |
|---|---|
| Gate | Brief 6, WP-29 |
| Owner | burba |
| Prerequisites | KHC-042, KHC-043, KHC-045 |
| Source | No game data for the Agent test; the game client runs read the install |
| Wire | the hub's |

## What was done

- `hub::bot`: a headless client that walks scripted paths with every allowed motion and emote; `khc-hub --bots N --addr`.
- An Agent test: 32 bots for 60 s on the link simulator's Typical profile (40 ms, 5 ms jitter, 1% loss each way), all in one process.

## Results

| Measure | Target | Measured |
|---|---|---|
| Down a client | 8 KB/s or less | 3.2 KB/s |
| Up a client | 1 KB/s or less | 0.3 KB/s |
| Server tick, 99th percentile | 5 ms or less | 1.4 ms (test build) |
| Violations by honest bots | 0 | 0 |
| Game client, `--hub-bots 32` | 1% low within 10% of 53 | 30: FAIL (KHC-042) |

## Checks

| Check | Group | Verdict |
|---|---|---|
| 32 bots, 60 s, Typical link: bandwidth and tick | Agent | PASS |
| Memory growth under 5% over a one-hour soak | Agent | NOT RUN: no long runs, as asked |
| Server restart: clients rejoin and keep their levels | Agent | PASS for the rejoin in under 10 s (a fresh hub on the same store); levels kept follow from the store, not measured |
| 10% loss, instance overflow | Agent | PARTIAL: overflow PASS; 10% loss not run |
| Abuse: a flood, malformed datagrams, a speed hack, a thousand fake joins from one address; normal clients still served | Agent | PASS |
| A name-collision bot | Agent | PASS: a second client with a taken id is refused; the same name with another id is allowed by design |
| Client within 10% of the empty hub with 32 avatars and 16 tags | Local | FAIL |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Verdict

PARTIAL. The server meets its targets; the client's 1% low with 32 avatars FAILS, and the soak was not run.
