# KHC-048: Challenges, hub to duel and back

| | |
|---|---|
| Gate | Brief 6, WP-27: challenge someone in the hub, play, come back with XP |
| Owner | burba |
| Prerequisites | KHC-045, KHC-047; Brief 3's relay; Brief 5's lobby |
| Source | No game data |
| Wire | the hub's |

## What was done

- The server's challenge state: Available, Looking, Challenging, Challenged, In match, Away, Do not disturb. A challenge carries rules (up to 96 bytes) and times out in 20 s; three a minute; 30 s after a decline before the same target again; a blocked challenger's challenge shows nowhere and times out; a busy or do-not-disturb target answers Busy; a target who leaves ends it as Gone. Duel pads: two players Looking for 3 s are matched.
- On accept both get `MatchStart` with a match id and the rules. [hub choice, KHC-048] The challenger hosts: the hub does not know who is nearer the relay. The host sends its room code (`HostCode`) and the other player receives it (`MatchCode`). Results come back as `Result` (KHC-046).
- In the game (KHC-052's pass): a challenge opens the hub menu on Accept, Decline and Busy (`hub_menu.rs`). [hub choice, KHC-048] Hub to duel and back restarts the program, the restart the brief allows: the duel was written to start as its own process. On `MatchStart` the host restarts as a duel host with a relay room and sends its code to the hub; the other player restarts when the code arrives. The duel process keeps its own link to the hub (`--hub-match`): it reports the result, its status reads In match until it does, and the result menu has Back to hub, which restarts into the same instance. Pads on Waypoint Plaza set Looking for match. The launcher has a Hub row and a Hub server row.
- The spike was not measured as such: the restart path was chosen and works; the switch within one process was not tried.
- A challenge needs a relay set on the Multiplayer screen (or `--hub-relay`); without one the player is told so. KHC-051's room codes with no server could replace that requirement; not done.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Every transition and timeout, a blocked challenger, rate limits, a challenge to a player who just left, pads | Agent | PASS |
| Loopback with a relay, a hub and two clients: matched, match, return, XP | Local | PASS by pads (`flow`): matched, duel, rated, the joiner back in the hub; a challenge answered from the menu NOT RUN (no presses in an unattended run); the XP lines not captured |
| Challenge accepted to duel lobby open in 5 s or less | Local | NOT RUN: not timed |
| The same on two PCs | Owner | NOT RUN |
| The hub as an option beside Host and Join | Owner | Default in force: a Hub row on the Multiplayer screen |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |


## Update: hub to duel and back, on loopback (KHC-052's pass)

`scripts\brief6_flow.ps1`: a relay, `khc-hub`, and two game clients with their own profiles (`KHC_PROFILE_DIR`), each started on a duel pad of Waypoint Plaza (`--hub-pad`). Run `flow` (private, `runs\brief6\flow\`):

1. Both stood on pads; the hub matched them after 3 s (`matches 1`).
2. The host restarted as a duel host with a relay room (`hub_relaunch`), registered, and sent its room code to the hub; the other player waited, got the code (`MatchCode`), restarted and joined.
3. The duel ran (time limit, benches) and ended after 34 s; both reported (host winner 0, joiner winner 1) and the hub rated it (`rated 1`, `mismatched 0`).
4. The joiner's duel restarted into the hub again and stood back in instance 1 on Waypoint Plaza.

The lobby's capture shows both small cards (`Player6945 Lv 1 Newcomer` and the other's). The result screen's XP lines were not captured: the unattended result asked for a rematch before the capture tick. One run; two PCs NOT RUN.

## Verdict

PARTIAL. Pads to duel to hub PASS on loopback; a challenge answered by a person, the timing and two PCs NOT RUN.
