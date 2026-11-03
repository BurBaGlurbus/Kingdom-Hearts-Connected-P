# KHC-053: Reply codes, for a host whose router filters

| | |
|---|---|
| Gate | A friend not let in by a direct room code (KHC-051) gets in when the host types the friend's reply code, with no server and no account |
| Owner | burba |
| Prerequisites | KHC-051 |
| Source | The player's Steam KH 1.5+2.5, only read, at run time. One Windows PC on a home network |
| Wire | PROTOCOL 14, unchanged: the knock is KHC-018's Punch datagram |

## Why

The owner's router keeps one port for every destination (good for direct codes) but opens no port when asked (no UPnP or NAT-PMP answer, KHC-051), so it most likely drops a friend's first knock: the router lets in only an address the host has already sent to. When the host sends to the friend first, both routers see an exchange they started, and both let the other through.

## What was done

- **The joiner's reply code** (`net::Session::join_direct`): a joiner by direct code also asks the STUN servers for its own public address, from the same socket it knocks with. Not let in 4 s after it started, it gets `Event::ReplyCode`: a code in the same twelve-character form as a room code, holding the joiner's address. A router that gives each destination its own port gives no reply code (it could not work). The joiner keeps knocking for 60 s in all (was 15), so there is time to type it, and then ends with a reason that names the reply code.
- **The host's knock** (`Session::knock_toward`): toward the address in a reply code, every 100 ms for 30 s while nobody is in. The joiner's knocks, still coming every 250 ms, then get through, and the session starts as any other.
- **The game**: the joiner's balloon shows "Not in yet? Give your friend this reply code:" and the code. The host waiting with a direct code types it straight into the game: letters, digits and dashes appear in the balloon ("Their reply code: ...", Backspace to fix), Enter knocks toward it ("Letting your friend in...") or says the code's check failed. Until something is typed the host's balloon says "Friend not in? Type their reply code". [Duel choice, KHC-053] The keys still reach the field while the host types: Sora may walk a little while the host waits.

## Results

Loopback test: a joiner whose host never answers gets a reply code holding its own socket's address at about 4 s, and a host given that address sends Punch datagrams to it. Two windows on this PC (`scripts\khc051_direct.ps1 -Set reply-2`): the host's direct code came at tick 48, the joiner's reply code at tick 240 (4 s after its first update). On one PC the two cannot reach each other by public address (no hairpin on this router), so the knock's effect between two routers is not shown here.

## Evidence

Private: `%USERPROFILE%\kh2-kh3-ref\runs\khc051\reply-1`, `reply-2` (the first ended before the joiner had loaded: its first frame came 15 s after start).

## Checks

| Check | Group | Verdict |
|---|---|---|
| A joiner not let in gets a reply code with its own address; the host knocks toward a reply code's address | Agent | PASS |
| Every other session test, the direct-code tests among them | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| The joiner shows its reply code in a real run | Local | PASS (event at tick 240) |
| Typing a reply code into the host's window | Owner | NOT RUN: typing into the game would take the owner's keyboard focus |
| A friend on another network behind a filtering router gets in by reply code | Owner | NOT RUN: needs a friend on another network |

## Uncertainty

That the knock gets through rests on how routers that keep one port per destination usually filter (by address the host has sent to); this was not measured across two real routers. A router that filters by port as well works the same way, since the host knocks at the exact port in the code.

## Verdict

PARTIAL. Built and PASS on loopback and in a two-window run; the typing and a real two-network join NOT RUN.
