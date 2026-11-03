# KHC-025: Hit fairness for the joiner's attacks

| | |
|---|---|
| Gate | Brief 2, WP-14: a joiner's hit that connected on the joiner's screen also connects on the host, within a cap |
| Owner | burba |
| Prerequisites | KHC-033, KHC-024. Brief 3's WP-15 for the final rewind amount |
| Source | The player's Steam KH 1.5+2.5 and KH3 installs, only read, at run time. One Windows PC, loopback. Method: [Valve, Lag Compensation](https://developer.valvesoftware.com/wiki/Lag_Compensation) |

## What was done

- Each joiner input message carries the host tick of the snapshot on screen. PROTOCOL 6.
- `source/khc/src/room/duel_rewind.rs`: a history ring of 12 ticks, the lookup capped at 6 ticks, and the policy. `duel_hits.rs` keeps each Sora's HIT spheres, guard windows, Reflect barriers and whether he could be hit.
- A joiner's melee window is tested against the host's Sora at that tick. Spells and bolts are not rewound.
- [duel choice, KHC-025] A KH3 joiner's own hit test gets the host's Sora at the rewound place, while homing keeps aiming at where he is. Only his place: KH3's guard and dodge read him as he is now, and these hits are not in the rewind's average. No run measured this.
- Rule `dodge`, the lobby's *Late dodges*, `--duel-dodge attacker|defender`. **Attacker's view by default**, the brief's default.
- `duel_hit` logs `rewind` and `behind`, how far back the joiner's screen really was. The overlay has "Rewind average N ticks". `run_compare` treats both as free fields.
- [Test] `--duel-strafe` moves the host's Sora side to side and guards; `--no-rewind` judges against the present. Locked runs do not rewind.
- Step 7 waits for Brief 3's WP-15: the code reads whatever `shown_snapshot` holds, so a new playout delay arrives by itself.

## Results

Joiner's bench combos on a strafing host: 60 s of fight, three runs a set on each of two builds (before and after the `connected` change of KHC-033), attacker's view unless stated. "Stopped" is the share of joiner swings guarded or missed on invulnerable frames.

| Set | Hits a minute | Stopped |
|---|---|---|
| LAN, rewind | 67, 69, 70; 80, 76, 66 | 11.6%; 5.3% |
| LAN, rewind off | 74, 73, 79 | 4.6% |
| 40,5,1, rewind | 66, 78, 67; 73, 63, 59 | 11.1%; 17.8% |
| 40,5,1, rewind off | 61, 56, 66; 63, 69, 76 | 20.4%; 12.6% |
| 40,5,1, Defender wins | 56, 63, 74; 66, 63, 76 | 18.6%; 11.4% |

**These runs do not show that the rewind helps.** Means at 40,5,1 are 68.7 with it and 65.2 without, inside a spread of 20 between runs of one set. Runs differ widely because an open networked run takes a different course each time (KHC-024).

The cap is why the effect could be small. `behind` on a LAN is already 6 to 8 ticks (75 of 76 hits), so every swing is capped at 6: on a LAN, where 0 was expected, and short of the full distance at 80 ms. Most of it is the input queue and the joiner's snapshot buffer, not the network.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief2\`: `fair-*` and `g-*`, `f-lan`, `f-lan-off`. Read with `brief2_fair.py`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| History ring, capped lookup, both dodge policies, rules and input round trips | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS: 1429 passed, 2 ignored |
| `tools\Build-Launcher.ps1` | Agent | PASS |
| Combos at the Typical profile land as on a LAN, within the noise | Local | PASS within a wide noise: 59 to 78 against 66 to 80. Six runs a side cannot rule out a real gap |
| Host's dodges fall by 5 points or less against a LAN | Local | PASS: they rise, 11.1 and 17.8% against 11.6 and 5.3% |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN |
| Host's feel after dodging | Owner | NOT RUN |

## Uncertainty

Whether the rewind makes hits land more often is open, as above. "Stopped" counts guards too: the bench never dodges with invulnerable frames much. A fair test needs the lock (`--script-locked` turns this off now) or many more runs.

## Owner's question

When a joiner's rewound hit meets a dodge the host started since, who wins? Default per the brief: the joiner's view, within 100 ms.

## Verdict

PARTIAL. Built and checked to the targets as written, but its benefit is not shown. WP-15 and the queue depth come next, since they set how far back the joiner's screen is.
