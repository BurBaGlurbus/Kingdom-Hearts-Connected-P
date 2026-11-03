# KHC-034: Prediction's camera and cosmetics

| | |
|---|---|
| Gate | Brief 2, WP-03 part B: camera, lock-on, swing trails and sounds from the predicted Sora; an overlay line |
| Owner | burba |
| Prerequisites | KHC-033 |
| Source | The player's Steam KH 1.5+2.5 install, only read, at run time. One Windows PC, loopback |
| Number | The brief called this record KHC-012b, and KHC-012 was already taken |

## What was done

- Camera, lock-on and swing trails need no code on the joiner: the room's Sora on that instance is the predicted one, so what reads him follows. The drawn offset goes on after the fixed tick and comes off before the next, so the camera reads the true place.
- Swing sounds: `duel_predict::own_sounds` runs the room's sound scan on the joiner and keeps only swings. Voices, cloth and steps stay the host's: a voice's pick is a roll of the host's.
- The host marks its rival's swings `DuelEvent::OwnSwing`. The joiner drops one it has played in the last 90 ticks and plays any other. PROTOCOL 5.
- Overlay: "Corrections N a minute, largest M".
- Each correction goes in the log as `duel_correction`, with `moved`, the actions, and `input_ack`.

## Results

Ten matches at 40,5,1, build `h-ten`: 161 swings played from the prediction, 154 of the host's events matched and dropped, 11 of the host's played because the joiner had not. On the runs checked for a host event followed within 40 ticks by a local swing of that sound (`ten`), none of 33. Seven local swings had no host event: the prediction swung and the host did not.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief2\`: `ten`, `h-ten`, read with `brief2_matches.py`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| A swing plays once on either side; the rolling minute's count | Agent | PASS |
| The event round trips | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS |
| No swing sound played twice in ten matches | Local | PASS by the logs. Heard by no one |
| Camera, lock-on and trails follow the prediction | Local | NOT RUN as a measurement: true by construction on the joiner, not looked at |
| The overlay line drawn | Local | NOT RUN: runs ran minimized |
| Feel with a controller on two PCs | Owner | NOT RUN |

## Uncertainty

A swing the prediction started and the host refused has played its sound. A second swing of the same sound within 90 ticks can pair with the wrong event, which plays or drops one late, not twice. On the host, with its buttons waiting, trails follow what it draws but swing sounds do not (KHC-032).

## Verdict

PARTIAL. Code and logs PASS; nobody has seen or heard it.
