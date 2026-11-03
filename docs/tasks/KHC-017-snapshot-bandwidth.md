# KHC-017: Snapshot bandwidth

| | |
|---|---|
| Gate | Brief 3, WP-08: a match fits a home upload link without spikes |
| Owner | burba |
| Prerequisites | KHC-031 (`--net-log`), KHC-026 (30 Hz needs the playhead) |
| Source | The player's Steam KH 1.5+2.5 and KH3, only read, at run time. One Windows PC, loopback |
| Wire | PROTOCOL 9 |

## What was done

- **Measured first.** The network log's `by_kind` is now filled: bytes a second of inputs, snapshots, events, other messages, and the part of the snapshots that is flying things (`flights`). The session counts messages by their first byte; the game names them.
- **Delta snapshots** (`duel_wire::Encoder`, `Decoder`). A fighter's state is twelve groups (place, yaw, action, frame, blend, vitals, gauges, items, flying things, KH3 state, KH3 animation, prediction's motion) and a mask says which follow. A full snapshot goes every 30 ticks; in between, only what changed since the newest snapshot the joiner says it has (`snapshot_ack` in its input message). A delta from a snapshot the joiner never had is dropped, and the next full one puts it right; deltas need no order, so the unreliable channel's newest-wins rule stands.
- **Flying things as lines.** Wisdom's bullets and KH3's spell and shotlock actors go as a line: where, which way, how fast, from which tick. The joiner moves them along it, and the host sends a new line only when the thing is more than 1 unit off its line, turns, or the line is 200 ticks old. [duel choice, KHC-017] One rule for all of them: a bolt a layer steers (`RoomHooks::steers_bullet`) or one that homes just needs more lines.
- **Prediction's motion only when it is read.** The joiner's input says whether it predicts; otherwise the 60 bytes a tick of `Motion` stay home.
- The input message's ticks are 32 bits now (snapshot ticks too); a duel would have to last two years to wrap them.
- **Optional 30 Hz snapshots**, `--snapshot-rate 30` on the host. The playhead of KHC-026 holds two snapshots, so at 30 Hz the opponent shows about 67 ms late. Off by default; a locked scripted run always sends 60.
- **Positions not quantized** (step 4): T249 kept them f32, and the targets were met without touching them.
- Replays keep every snapshot as it came (deltas included) and the joiner's code shows the newest of a tick, so a fast replay no longer drops what a later delta starts from.

## Results

Scripted matches as T279 ran them (host `--duel-bench`, joiner `--rival-script`), one fight each, joiner's download while fighting:

| Run | Before (KHC-026's build) | This change |
|---|---|---|
| KH2 ruleset, LAN | 12.6 KB/s median, 15.1 most | **3.9 median, 5.4 most** |
| KH3 ruleset, LAN | 11.9 median, 12.1 most | **5.0 median, 5.5 most** |
| KH2 ruleset, `--net-sim 20,5,5` | 12.3 median, 15.1 most | **4.2 median, 5.5 most** |

Snapshots are about 2.7 KB/s of that in KH2 and 3.9 in KH3; the rest is renet's and the datagrams' framing, events and acknowledgements. The host's upload fell from 12.6 to 4.0 KB/s.

Same score, damage, hits and winner on both screens in all six runs. At 20,5,5 both builds had 2 skip warnings, both in the stall at the fight's start that KHC-026 describes.

Neither build's scripted KH3 match fired a shotlock volley, so the 36 to 46 KB/s peak T279 measured did not occur in either. The unit tests show what a volley costs now: a bolt flying straight sends a line when it starts and again when its speed is known, then nothing; one turning 0.03 rad a tick sends a line every few ticks and never strays more than 1 unit.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief3\`: `bytes-kh2-*`, `bytes-kh3-*`, `lossy-*` for `wp08` and `wp15`. Runs: `scripts\brief3_bytes.ps1`; read with `scripts\brief3_last.py`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Full and delta snapshots round trip; a delta after a lost full one; a delta from one never had; any order; something gone | Agent | PASS |
| Lines stay within 1 unit, straight and turning; a straight one costs nothing between full snapshots | Agent | PASS |
| Every message kind round trips, the input's new fields included; bytes by kind | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS, in the commit's run |
| Typical traffic 6 KB/s or less | Local | PASS: 3.9 to 5.0 |
| Peaks under 20 KB/s | Local | PASS for the runs made (5.5 at most); a shotlock volley NOT RUN |
| Drawn positions within 1 unit of the uncompressed path | Local | PASS for the Soras, which are sent exactly; flying things within 1 unit by construction and by test, not compared on screen |
| `--net-sim 20,5,5`: no more skip warnings than the baseline | Local | PASS: 2 and 2 |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN |

## Uncertainty

A volley's real cost is from tests, not a run. 30 Hz was built and its controller tested, not played; KH3's animation clock then steps every other tick on the joiner.

## Verdict

PASS for the targets measured; the volley peak and 30 Hz play are NOT RUN.
