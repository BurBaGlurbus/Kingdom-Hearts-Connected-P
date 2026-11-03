# KHC-051: Room codes with no server

| | |
|---|---|
| Gate | A friend types a room code and joins, with no server the owner buys or runs; LAN and the KHC-018 relay keep working |
| Owner | burba |
| Prerequisites | KHC-018 (room codes through a relay) |
| Source | The player's Steam KH 1.5+2.5, only read, at run time. One Windows PC on a home network |
| Wire | PROTOCOL 14, unchanged: no datagram kind was added (STUN is its own format, told apart by its magic cookie) |

## What was done

- **The code is the host's address** (`net::direct`). Twelve characters of the relay codes' alphabet (no 0, O, 1, I or L), shown as three groups of four (`ABCD-EFGH-JKMN`): the IPv4 address and port (48 bits), mixed so neighbouring addresses do not look alike, and an 11-bit check, so about 1 typo in 2048 gets through instead of sending a friend to a stranger's address. Six characters is still a relay room, twelve a direct one; dashes and case are ignored. [Duel choice, KHC-051] The mixing is not secrecy: whoever has the code has the host's public address, as with any game played without a server between.
- **The host's public address** comes from free public STUN servers (RFC 5389), asked from the game's own socket so the router's mapping is the one a friend will use: `stun.l.google.com:19302`, `stun.cloudflare.com:3478`, `stun1.l.google.com:19302`. Names are looked up on a thread; each server is asked every 300 ms for up to 3 s; a waiting host asks again every 15 s, which keeps the router's way in open.
- **The router is asked to open the port** (`net::portmap`, on its own thread): UPnP's Internet Gateway Device (SSDP, then `GetExternalIPAddress` and `AddPortMapping` over SOAP), else NAT-PMP (RFC 6886) to the router. The mapping is leased for 30 minutes, renewed at 15, and removed when the session ends; a router that only keeps permanent mappings (UPnP error 725) gets one with no lease, still removed at the end; a port another PC holds (718) moves to the next of four. Plain `std` sockets: no crate and no `unsafe`.
- **Which address goes in the code** (`direct::choose`, pure): the router's mapping when its address is the one STUN sees; else STUN's. Two STUN servers that see different ports mean a router (or the provider's CGNAT) that gives each destination its own port: no code can work, so the host is told why and gets none. A router whose own outside address is private or in 100.64/10 is behind another router or CGNAT: the code is made from STUN's address with a line that a friend may not get in. No mapping on a NATed PC: the code, with a line to turn on UPnP if a friend can't get in.
- **The session** (`Session::host_direct`, `join_direct`): events `RoomCode` (as for the relay), `RoomNote` and `NoRoomCode`. A joiner by direct code knocks with its Hellos (which also open its own router's way back) and gives up after 15 s with a reason a person can act on. The host stays a LAN host throughout.
- **The game**: `--host --room` without `--relay` hosts with a direct code; `--room CODE` with a twelve-character code joins it, without `--relay`. The balloon shows the code and the line, or why there is no code. With `--relay`, both behave as in KHC-018.
- **The launcher's Multiplayer screen**: *Host with a room code* and *Join by room code* no longer need a relay: with none set they use direct codes; with one set, the host uses the relay. *Join by room code* takes either kind.

## Results

Loopback, with our own STUN servers in the tests: a direct code made from what STUN saw carries the joiner to the host, who exchange messages directly; STUN servers that see different ports give no code and say why, and the host still takes a LAN player; no STUN answer gives no code. NAT-PMP against a router of our own on loopback.

On this PC's home network, asked only (nothing was mapped; `cargo test -p net -- --ignored live --nocapture`): both public STUN servers answered with the same public address and port, so the router keeps one port for every destination, which is what direct codes need. The router answered neither SSDP nor NAT-PMP: UPnP looks off (or is not offered), so on this network a host gets the "turn on UPnP" line, and whether a friend gets in depends on whether the router lets an unknown address through.

## Evidence

Private, outside the repo: `%USERPROFILE%\kh2-kh3-ref\runs\khc051\` (the check runs, `checks.txt`; the clean tree they ran in; `direct-1`, the two-window run, from `scripts\khc051_direct.ps1`). The live probe's output is not kept: it holds the owner's addresses.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Codes round-trip, look unalike for neighbouring addresses, refuse 99% of single typos and relay codes | Agent | PASS |
| STUN responses read (RFC 5769's address), foreign datagrams not taken for STUN | Agent | PASS |
| The address chosen from STUN and the router's answer, in each case above | Agent | PASS |
| UPnP description, SOAP answers, chunked HTTP; NAT-PMP with a loopback router | Agent | PASS |
| A direct code carries a joiner to its host on loopback; a port per destination gives no code; no STUN answer gives no code | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, on HEAD plus this change only (the shared checkout holds another thread's unfinished work) |
| Public STUN servers answer this PC, one port for both | Local | PASS |
| The router opens the port (UPnP or NAT-PMP) | Local | NOT RUN to a mapping: the router answered neither when asked |
| Two players on different networks complete a match by direct code | Owner | NOT RUN: needs a friend on another network |
| Two windows on this PC, host by direct code, joiner by that code (`khc051_direct.ps1`) | Local | PASS for what it can show: the code and its UPnP line came 1 s into the host's run; the joiner, knocking on the public address from inside the same network, was not let in (no hairpin or port mapping on this router) and ended at 15 s with its reason. On one network the LAN list is the way |

## What does not work yet, and options for the owner

- **A host whose router filters unknown addresses and does not open ports** (UPnP off, as on this network): the friend's knocks are dropped, so the friend gets the 15 s message. Ways out, by cost: the friend hosts instead; turn on UPnP on the router; or a **reply code** (not built): the joiner's game shows its own code after a failed knock, the host types it in, and both knock at once, which gets through any router that keeps one port per destination, this one included. It needs typing in the game window, which the duel does not have yet.
- **Symmetric NAT or CGNAT on both sides**: no direct way exists; only a relay helps. Free relays nobody here hosts:
  - *Steam Datagram Relay* (Steamworks networking): free and good, but needs a Steam app id (KHC has none; app id 480 is for testing only), every player signed in to Steam, and the Steamworks SDK through FFI (`steamworks` crate), which breaks the workspace's `unsafe_code = "forbid"` rule unless isolated in a crate of its own; Valve's terms also apply.
  - *Epic Online Services P2P*: free with relays, needs an Epic developer account and product, a client id and secret shipped in the game, and the EOS SDK through FFI (same `unsafe` question).
  - *A public signalling channel* (for example a free MQTT broker) could carry the reply code automatically instead of the host typing it; it relays no game traffic, but it is someone else's server with no promise to stay up.
  None was added; each needs the owner's choice.
- IPv6 is not used (it would often need no mapping, but the code would be much longer).

## Uncertainty

No router on this network opened a port, and no second network was available, so the path a real friend takes was not run end to end. How often UPnP is on among the owner's friends is unknown.

## Verdict

PARTIAL. Built and PASS on loopback and against public STUN; play across two networks NOT RUN; on the owner's own network a host needs UPnP turned on, the friend to host, or the reply code above.
