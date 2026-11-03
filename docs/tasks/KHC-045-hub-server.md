# KHC-045: The hub server

| | |
|---|---|
| Gate | Brief 6, WP-24: a server that holds the hub: who is in it, where, and what they asked for |
| Owner | burba |
| Prerequisites | KHC-044, KHC-042 |
| Source | No game data for the crate; the game client in `net-8` reads the install |
| Wire | `KHCH`, `HUB_PROTOCOL` 1 |

## What was done

- `source/hub` (new workspace member) and `khc-hub`. [hub choice, KHC-045] The hub's datagrams live in `hub::wire`, not `net::wire`: `net` moves bytes and the meaning is the game's. Their own magic `KHCH` and version.
- Packets carry a cumulative ack, reliable messages due (resent after 250 ms) and at most one unreliable message. Messages: Join, Leave, Move, SetStatus, Pad, Challenge, Reply, CardAsk, CardChoices, Emote, Phrase, Friend, Unfriend, Block, Unblock, Report, Result, HostCode, DeleteProfile; Cookie, Welcome, Reject, Roster, RosterAdd, RosterGone, Nearby, Challenged, ChallengeEnd, MatchStart, MatchCode, Card, Bubble, FriendAsk, Friends, Xp, Notice.
- `Hub::handle(now, from, bytes)` and `Hub::tick(now)`: a state machine with no socket. A stranger first echoes a cookie (hash of its address and the minute) before anything is kept; a Join is padded to 240 bytes and nothing sent to a stranger is larger than what it sent.
- Interest: the nearest 24 within 4000 units plus every friend; everyone appears in the roster. Limits: 15 datagrams a second (burst 30); speed by motion (600 a second on the ground, 1000 in the air) with 25% over and 60 units of slack; the map's bounds; a teleport flag at most every 5 s; three violations in 10 s drop the client. Joins: 2 a second per address (burst 10), 4 clients an address (loopback free unless `--public`).
- 32 an instance; a full one opens another; a player may join a friend's instance (`follow`) or a numbered one. Every update for a client goes in one packet each 100 ms.
- Profiles: one JSON file a player, written atomically, version 1 (`--store DIR`; memory otherwise). Counters every 10 seconds.
- The game's side: `room/hub_net.rs` with `--hub-server ADDRESS:PORT` (and `--hub-instance N`): its own move 10 times a second, the roster and nearby avatars, bubbles and toasts. It goes back to the last instance.
- `khc-hub --bots N --addr ADDRESS:PORT` runs headless bots.

## Results

`net-8`: `khc-hub`, eight network bots and one game client on loopback. All nine joined one instance; the game client saw the bots and their tags; the server counted 0 violations, 0 drops, 69 to 86 datagrams a second in and 2.4 to 2.8 KB/s in, 7 to 10 KB/s out in all (about 1 KB/s a bot down, 0.3 up). 24 rate refusals came in the first ten seconds, from nine clients joining from one address at once.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief6\`: `empty-32` (the hub alone), `bots-32` (`--hub-bots 32`), `bots-4` (a projection check), `net-8` (`khc-hub` on loopback, `khc-hub --bots 8`, one game client). Runs: `scripts\brief6_runs.ps1 -Kind empty|bots|net -Bots N`; build: `scripts\brief6_build.sh NAME`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Join and leave, roster, interest, a full instance opening another, a taken id, a bad name | Agent | PASS |
| Rate limit (a flood is refused and dropped), speed and bounds violations drop a cheat, disallowed actions ignored | Agent | PASS |
| Challenge accept, decline, busy, timeout | Agent | PASS (KHC-048) |
| Malformed datagrams ignored; no answer to a stranger larger than its request | Agent | PASS |
| A thousand fake joins from one address; the honest client stays | Agent | PASS |
| Every message kind round trips, and is refused cut short | Agent | PASS |
| 32 bots: 8 KB/s or less down, 1 KB/s or less up a client | Agent | PASS: 3.2 and 0.3 (KHC-050) |
| LAN hub inside a host's process, announced by LAN discovery | Local | NOT RUN: not built |
| Where the server runs and who operates it | Owner | Default in force: loopback and LAN only |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Uncertainty

The speed limits are targets read from KH2's default run (8 a tick); Quick Run, Glide and the Drive forms' speeds were not measured, so a player using them may be dropped. The per-address join bucket also limits loopback tests.

## Verdict

PARTIAL. Server, client and tests PASS; the embedded LAN hub is not built.
