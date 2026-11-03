# KHC-033: Prediction and correction

| | |
|---|---|
| Gate | Brief 2, WP-03 part A: the player's own Sora moves at once, and the host's snapshots correct him |
| Owner | burba |
| Prerequisites | KHC-032 steps 1 to 5, KHC-024 |
| Source | The player's Steam KH 1.5+2.5 install, only read, at run time. One Windows PC, loopback |
| Number | The brief called this record KHC-012a, and KHC-012 was already taken |

## What was done

- `source/khc/src/room/duel_predict.rs`, behind `--predict`, off by default. A ring of 64 ticks holds each tick's buttons and predicted state. `Ring::reconcile` and `Blend` know nothing of the room, and their tests run on a toy.
- `Snapshot` gains `input_ack`. The joiner's seat also carries `Motion`, what a restore cannot work out: vertical speed, air velocity, arc, combo, the jump cut and its take-off, the command lockout, the physics sampler, and whether the swing connected. About 60 bytes a snapshot, 3.6 KB/s. PROTOCOL 4 and 7, with PROTOCOL 5 for the swing event of KHC-034.
- `rival_play::predict_step` is the host's step for that seat, on the room's own Sora and floor state. `RoomHooks::replica_input` now returns the step.
- The joiner compares the host's newest snapshot with its prediction at `input_ack`. Under 4 units in the same action it keeps the prediction. Otherwise it takes the host's state, replays the later buttons and fades the drawn offset over 6 ticks; past 120 units it draws him there at once.
- [duel choice, KHC-033] While the host has him in an action only it runs (hurt, a cast, an item, a guard's recoil, a form change) the joiner shows him as the host does and does not step him. HP, MP, Drive and items stay on the snapshot being shown.
- [duel choice, KHC-033] A swing's hop aims at the other Sora where the host will have him when it uses those buttons: his newest place, carried on at his last speed for the ticks between `input_ack` and those buttons, 30 at most.
- [duel choice, KHC-033] **KH3's seats are not predicted**, the brief's default. Copying the state is cheap (KHC-019), but a KH3 step starts effects and cues, so running it again in a replay is not free of what it did.
- Off in `--script-locked` runs and in replays. Replays from before this work are refused by protocol.

## Results

Press to motion on the joiner's own screen at 80 ms: **1 tick, largest 1**, over 10 to 12 presses in each of 6 runs. Measured before the change: 14.5 median, largest 19. The brief expected about 7.

Corrections over 10 units, per minute, target 2 or fewer:

| Run | With the foe lead | And `connected` | And the sampler, as committed |
|---|---|---|---|
| Attacks and combos, ten matches, 40,5,1 | 30 median, 37 largest | 14 median, 21 largest | **14 median, 20 largest** |
| Walking and jumping only, 40,0,0 | | 16 and 34 | **6 and 11** |

The `connected` flag halved it in fights and the sampler helped walking and jumping, not fights (the commit's message says it halves them, which is true only of the latter). The target is not met. Most of what remains is a swing's hop toward a Sora who has just been hit or has moved, and hits only the host knows of. Walking and jumping correct mostly in the first seconds after loading, when frames stall, and in bursts after a stall.

Ten scripted matches at 40,5,1: 10 of 10 over, 10 of 10 the same score, damage, hits and winner on both screens. One match each at 20,2,0, 75,15,3 and 115,30,10 (on the build before the sampler): over, same score. The brief's "no more skipped snapshots than the baseline" at 75,15,3 was not compared: there is no baseline run at that profile.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief2\`: `press-dev-sim` (before), `i-press`, `ten`, `f-ten`, `h-ten`, `f-move`, `h-move`, `f-good`, `f-poor`, `f-bad`. Read with `brief2_corr.py`, `brief2_matches.py`, `brief2_press.py`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| No correction when states agree, a correct replay when an input differs, a blend that never exceeds its bound | Agent | PASS |
| Every message kind round trips with sample values, `Motion` included | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: 1429 passed, 2 ignored |
| `tools\Build-Launcher.ps1` | Agent | PASS |
| Press to visible motion 1 tick or fewer at 80 ms | Local | PASS |
| Corrections over 10 units 2 a minute or fewer | Local | **FAIL**: 14 a minute in fights, 6 to 11 walking and jumping |
| Ten matches, same HP and score on both screens | Local | PASS |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN |
| Feel with a controller on two PCs | Owner | NOT RUN |

## Uncertainty

Loopback on one busy PC, with builds and other programs running at times. How 14 corrections a minute look is not known: each is faded over 6 ticks, and only a person can say whether it reads as a slide or as a jump. The KH3 replay cost was not measured beyond copying.

## Verdict

PARTIAL. It responds at once and both screens agree on results, but corrections miss their target, so `--predict` stays off by default.
