# KHC-018: Internet play: rendezvous, relay and room codes

| | |
|---|---|
| Gate | Brief 3, WP-09: two players on different networks connect with a room code, directly when they can and through a relay when not |
| Owner | burba |
| Prerequisites | KHC-029, KHC-026, KHC-017, KHC-027; Brief 2's KHC-032 and KHC-033 before friends are invited (see below) |
| Source | The player's Steam KH 1.5+2.5, only read, at run time. One Windows PC, loopback: relay, host and joiner as three processes |
| Wire | PROTOCOL 11 |

## What was done

- **New datagrams** (`net::wire`): Register and Registered (a host's room and its code), Lookup, PeerAt and NoRoom (a joiner finds the room; each side learns the other's public address), Punch (opens the way through a router), and Relayed, which carries any of the session's datagrams whole through the relay under the room's key. What asks the relay is padded to the size of its answer.
- **The relay** (`source/relay`, a new workspace member). `Relay::handle(now, from, bytes)` is a pure state machine; `khc_relay [ADDRESS:PORT]` puts it on one UDP socket (default `127.0.0.1:47621`). Six-character codes from `23456789ABCDEFGHJKMNPQRSTUVWXYZ`, with no 0, O, 1, I or L. [duel choice, KHC-018] One room per code and one joiner per room; a room nobody touches for 60 seconds goes, so a waiting host refreshes it every 20; at most 8 rooms per source address; control messages limited to a burst of 20 and 10 a second per source, forwarding to 1000 datagrams a second; no reply larger than its request; nothing it forwards is read, and nothing about who connects is kept.
- **The session** (`Session::host_room`, `join_room`). The host registers and is told its code (`Event::RoomCode`); the joiner looks the code up, and both punch toward the other's public address while the joiner says Hello there. If no Welcome has come in 2.5 seconds [duel choice, KHC-018: under the brief's 3 s, so a relayed player is in within the 3 s target too], the joiner sends everything by the relay, and the host answers by the way its joiner came. A rejoin (KHC-027) tries the direct way again and then the relay; the relay follows a joiner to a new address by the key.
- **The game**: `--relay HOST:PORT` with `--host --room` hosts a room, and `--relay HOST:PORT --room CODE` joins one. The balloon shows "Room code ABC234" to the host, or that the relay does not answer, in which case the LAN can still find the host. The overlay's first line is "Direct" or "Relay" and the round trip. [Test] `--relay-only` skips the direct try.
- **The launcher's Multiplayer screen**: *Host with a room code*, *Join by room code* and *Relay server* (the relay's address, kept in the settings); the first two are off until a relay is set. After a duel it shows how the last one's link went (*Last duel's link: Relay 42 ms*), read from the room's output.
- The LAN path is unchanged: hosting without `--relay` binds, announces and welcomes as before. The exact executable-hash check stays.

## Encryption: proposed, not added

Data is still neither encrypted nor authenticated: anyone who can see the packets can read them or send their own under the nonce. Step 5 asks for authenticated encryption once a session exists, and for the owner's approval before a crate is added. The proposal:

- **Key exchange:** X25519, ephemeral keys in Hello and Welcome (32 bytes each), with `x25519-dalek`.
- **Cipher:** ChaCha20-Poly1305 on every Data payload, a nonce from a per-direction counter, with `chacha20poly1305` (RustCrypto). Both are pure Rust with no `unsafe` in our code, and build on MSVC.
- **Against a relay in the middle:** the room code mixed into the key (an HKDF of the shared secret and the code, with `hkdf` and the `sha2` already here), so whoever relays without the code cannot read or forge, and the code is never sent to the relay after the lookup. A 6-character code is guessable offline only by someone who also holds the traffic.
- **Cost:** 16 bytes of tag per datagram, about 1 KB/s at 60 a second, and a PROTOCOL bump.

Not done until the owner approves the three crates. Until then: friends rooms only, over a relay the owner runs or trusts.

## Results

Three processes on loopback (relay, host, joiner), `--net-sim 40,5,1` on both players, a timed match:

| Run | Way | Connected after | Result on both screens |
|---|---|---|---|
| `relay-direct` | Direct | 11 joiner ticks, under 0.2 s | Same score, damage, hits and winner |
| `relay-only` | Relay | 11 joiner ticks, under 0.2 s | Same |
| `lan-wp09`, no relay | Direct (LAN) | 1 tick | Same |

Through the relay the round trip was the simulated 100 ms, as direct, and traffic the same: the joiner received 4.4 KB/s. The session tests on real loopback sockets connect directly, by relay, refuse a wrong code with its reason, and resume a cut session over the relay.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief3\`: `relay-direct`, `relay-only`, `lan-wp09`, and the relay's own output in each. Runs: `scripts\brief3_relay.ps1`, `brief3_duo.ps1 -Relay PORT [-ForceRelay]`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Relay state machine: register, lookup, one joiner, expiry and refresh, rate limits, rooms per source, reply size, forwarding by key, a joiner's new address, oversized data | Agent | PASS |
| Sessions through a real relay on loopback: direct within 3 s, relay within 3 s, wrong code refused, a cut resumed over the relay | Agent | PASS |
| Every datagram kind round trips; what asks the relay is as large as its answer; room codes as typed | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` | Agent | PASS, in the commit's run |
| Three processes on loopback complete a match with `--net-sim` on, connected within 3 s | Local | PASS, direct and relayed |
| A LAN-only run behaves as before | Local | PASS: connected, same result on both screens |
| Authenticated encryption | Owner | BLOCKED: waits for approval of the crates above |
| Two players on different networks complete a match | Owner | NOT RUN: needs a relay somewhere both can reach, which is the owner's to decide |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN |

## Questions for the owner, and what is in force

| Question | In force until answered |
|---|---|
| Where will the relay run, and who operates it? | Loopback only; nothing is published |
| May the repo add `x25519-dalek`, `chacha20poly1305` and `hkdf`? | No: proposed above |
| Keep the exact executable-hash match? | Kept |

## Uncertainty

Loopback has no NAT, so whether punching works between real routers is untested: the relay is the safety net. Brief 2 left `--predict` off (its corrections missed their target), and Brief 3's depends on it only for inviting friends; this was built anyway, as asked, and a friend at 100 ms sees their own Sora one round trip late unless they turn it on.

## Verdict

PARTIAL. Built and PASS on loopback; encryption BLOCKED on the owner, and play across two networks NOT RUN.
