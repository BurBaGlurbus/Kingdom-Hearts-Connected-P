# KHC-027: Rejoin

| | |
|---|---|
| Gate | Brief 3, WP-16: a joiner who drops for up to 30 seconds comes back into the same match |
| Owner | burba |
| Prerequisites | KHC-031, KHC-017 (a resumed joiner starts from a full snapshot) |
| Source | The player's Steam KH 1.5+2.5, only read, at run time. One Windows PC, loopback |
| Wire | PROTOCOL 10 |

## What was done

- **The session** (`source/net/src/session.rs`). Welcome gives the joiner a random token (64 bits from the system's randomness; nonces now come from it too). While the game says a match is live (`set_seat_hold`), a joiner who times out is not ended: the host holds the seat (`Event::SeatHeld`) and the joiner knocks again with its Hello every 250 ms, carrying the token (`Event::Reconnecting`). A Hello with the token is taken from any address, before or after the host noticed the drop, and both get `Event::Resumed` under a new nonce, with renet started over. A wrong token, or none, is refused while the seat is held. Datagrams of the old nonce are ignored, as before. The seat is held 30 seconds from the last word; then both end as a timeout did.
- **The match** (`duel_net.rs`, `duel_match_room.rs`). [duel choice, KHC-027] A live match pauses after one second of silence, well before the five-second timeout, so a drop costs neither player free hits: both Soras are held, and the match's clocks (its phase and the time limit) stand still and are moved on by the pause on return. The balloon says "Waiting for NAME. N s" on the host and "Reconnecting to NAME. N s" on the joiner. On return the host sends the rules, the match, the lobby and a full snapshot again before anything else. The match's sounds are keyed by match and round, so a moved clock replays no GO.
- After 30 seconds without a return, today's result stands: the host wins with `Why::Left`.
- A host that drops is not resumed: the host owns the match. The joiner knocks for 30 seconds and then ends as before, which is 25 seconds later than before. README says so.
- [Test] `--net-cut TICKS,SECONDS` cuts this instance's link both ways, that far into the script, as if a cable were pulled.

## Results

Loopback, a timed match, the joiner's link cut for 10 seconds 50 seconds into the script: the match paused one second into the cut, the host held the seat at five seconds, and both were back **0.19 s after the link returned** (`net_cut` at 939.109, `net_resumed` at 949.294 and 949.295 on the two sides, unix seconds mod 1000). Snapshots flowed again the next second and the match went on to its end.

A cut inside the first minute of a session (`rejoin-wp16`) never times out at all: that minute allows 45 s of silence for a PC still loading (T249). The match paused and went on when the link came back, without a resume.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief3\`: `rejoin2-wp16`, `rejoin-wp16`. Runs with `scripts\brief3_duo.ps1 -JoinExtra --net-cut,...`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| A 10-second cut, both ways, comes back to the same seat within 5 s of the link returning, new nonce, both channels working | Agent | PASS (10.2 s in all) |
| A wrong token, and no token, are refused while the seat is held | Agent | PASS |
| An unheld seat ends as before; a held one runs out on both sides | Agent | PASS |
| Stale Bye, Stopped and Data of the old nonce are ignored | Agent | PASS |
| Every message kind round trips, the token and resume fields included | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS, in the commit's run |
| Loopback 10-second drop resumes within 5 s of the link returning | Local | PASS: 0.19 s |
| Wi-Fi off and on at the joiner mid-round | Owner | NOT RUN |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN |

## Uncertainty

Spells and bolts already in the air when a match pauses go on flying and can still hit; the match's decision waits for the return. When a joiner's own link fails for good, its screen calls it a win by the host leaving, as before, while the host's says the joiner left: neither side can tell whose link it was. Nothing authenticates a data packet yet (KHC-018): the token is safe from guessing, not from someone who can read the traffic.

## Verdict

PASS on loopback; the Wi-Fi check on two PCs is the owner's.
