# KHC-031: Two-PC baseline and network log

| | |
|---|---|
| Gate | Brief 1, WP-01: real two-PC numbers and a per-match network log, as a baseline for every later package |
| Owner | burba |
| Prerequisites | KHC-010 (each player's ruleset) |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. Nothing from them is in this repository |
| Number | The brief called this record KHC-010, which the mixed-ruleset work had already taken. KHC-031 is the first number past the ones the brief's five parts use |

## What was done

- `--net-log <path>` (`source/khc/src/room/duel_log.rs`): once a second, one JSON line with `tick`, `role`, `rtt_ms`, `packet_loss`, `sent_bytes_per_sec`, `received_bytes_per_sec`, `silent_ms`, `simulated_drops`, `skipped`, `snapshots` (the joiner's buffer) and `input_backlog` (the host's). `by_kind` is null: it is the place for bytes per message kind (Brief 3, WP-08).
- `tools/Run-TwoPc.ps1` starts one side with the log on and reads a log afterwards: median and 99th-percentile `rtt_ms`, the largest loss, and `skipped`.

## Owner's checklist

Default until the owner says otherwise: one Wi-Fi run and one wired run. On each PC, `build.bat`, then a file of three lines (Steam folder, manifest, config), then:

```
tools\Run-TwoPc.ps1 -Role Host -Run wifi-kh2 -Name PC1 -Room room.txt -Ruleset kh2
tools\Run-TwoPc.ps1 -Role Join -Address <host IP> -Run wifi-kh2 -Name PC2 -Room room.txt
tools\Run-TwoPc.ps1 -Summary -Log .local\two-pc\wifi-kh2-Host.net.jsonl
```

"Skipped" is the log's last `skipped` on the joiner, which counts the snapshots behind the "skipped snapshots" warnings, not the warnings themselves. Both are on the overlay.

### Getting connected

| Check | Group | Wi-Fi | Wired |
|---|---|---|---|
| Windows firewall prompt appears on the first host and join; allowing private networks lets the other PC in | Owner | NOT RUN | NOT RUN |
| The host appears in the launcher's list (discovery by broadcast) | Owner | NOT RUN | NOT RUN |
| Join by address works | Owner | NOT RUN | NOT RUN |

### Five Best of 3 matches each

| Network | Ruleset | Matches finished | Median `rtt_ms` | 99th percentile `rtt_ms` | Skipped snapshots | Verdict |
|---|---|---|---|---|---|---|
| Wi-Fi | KH2 | | | | | NOT RUN |
| Wi-Fi | KH3 | | | | | NOT RUN |
| Wi-Fi | Each | | | | | NOT RUN |
| Wired | KH2 | | | | | NOT RUN |
| Wired | KH3 | | | | | NOT RUN |
| Wired | Each | | | | | NOT RUN |

The README's status line says the duel was tested on two PCs on Wi-Fi; the task index says NOT RUN for T240 to T249 and T279 to T281. Change whichever is wrong once this table is filled in.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief1\last\`, each run's `host.net.jsonl` and `join.net.jsonl`.

A line goes out by the clock, not by the tick. In the lobby and the countdown `game.tick` advanced about 120 per second in these logs, and about 60 in the fight; I did not look into why.

## Checks

| Check | Group | Verdict |
|---|---|---|
| A line from a fixed `Stats` value; valid JSON with room for bytes per kind | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: no warnings, 1410 tests passed, 2 ignored |
| `tools\Build-Launcher.ps1`, what `build.bat` runs | Agent | PASS |
| Frame rate within 5% of the run before the change (`--perf-overlay`) | Local | NOT RUN: the flags are off in play, but this was not measured |
| A loopback run with `--net-log` writes valid lines once a second | Local | PASS: 105 lines per side in `last\duo-h3j2-3`, each valid JSON |
| The checklist above, and the README's status line agreeing with it | Owner | NOT RUN |

## Uncertainty

`rtt_ms` and `packet_loss` are renet's estimates. Loopback says nothing about Wi-Fi: the numbers this package exists for are the owner's table above.

## Verdict

PASS for the Agent and Local lines. The owner's checklist is NOT RUN, every row. Nothing else waits on it.
