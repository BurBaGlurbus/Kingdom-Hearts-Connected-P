# Kingdom Hearts: Connected

Multiplayer for Kingdom Hearts II Final Mix on PC: 1v1 duels between two Soras, with the KH2 or KH3 ruleset,
over a LAN or the internet by room code. A native Windows program in Rust, built on [Bevy](https://bevy.org).

> **Unofficial fan project.** Not made, endorsed or supported by Square Enix or Disney. Kingdom Hearts is a
> trademark of Square Enix and Disney. This repository holds no game files, textures, models, sounds, movies or
> saves: the program reads them at run time from your own copies of the games.

## What you need

- Windows 10 or 11, 64-bit.
- Your own Steam copy of **KINGDOM HEARTS HD 1.5+2.5 ReMIX**.
- Optional: your own Steam copy of **KINGDOM HEARTS III**, for the KH3 ruleset and its effects.
- The same KHC build as the person you play: another build is refused with a message naming both versions.

## Status

Early. The duel has been played with two windows on one PC and on two PCs on the same Wi-Fi. Internet play by
room code is built but has not been tried with a friend on another network yet. See
[docs/tasks/](docs/tasks/README.md) for what has been checked, and what has not.

## Quick start

1. Build it (see [Building](#building)) or take a release build, and run `KHC.exe`.
2. The first launch opens **Quick start**: point it at your KH 1.5+2.5 install (and KH3 if you have it), choose
   whether to preload, and type the name the other player sees.
3. From the menu: **Host a Room** gives you a code to send a friend; **Join a Room** takes theirs; **LAN Host**
   hosts for players on your network; **Practice** tries the KH2 or KH3 ruleset against a training dummy;
   **Replays** plays saved duels.

A twelve-character room code holds your public address: give it only to the person you are playing. If a friend
cannot get in, their game shows a **reply code**; type it on the host and the host knocks back. Read
[SECURITY.md](SECURITY.md) before playing online.

## Security

Please report security problems privately, as [SECURITY.md](SECURITY.md) explains, not in a public issue.

## LAN duel

Two players on the same network can each run KHC.exe and fight each other's Sora. Each player needs their own Steam copy of KH 1.5+2.5 and KH3, because the game reads its data
from the install and checks it; the launcher on its own is not enough. Both players need the same launcher
build: another one is refused with a message naming both versions and each thing that differs. The host list shows a
host's version, and `KHC --version` prints your own.

**Host or join from the launcher:** *LAN Host* hosts and waits for a player. Hosts on the same network appear in
the join list by name: pick one to join. Or choose *Join by address* and type the host's IP
address. *Name* is the name the other player sees (your Windows user name until you change it; the launcher
keeps it).

**Firewall:** the first time the game hosts or joins, Windows asks whether to allow it on networks. Allow it on
private networks (your home Wi-Fi), or the other player cannot reach you.

**Known limitations:**

- Limits, summons, Limit Form, Donald and Goofy, and the room's Shadow are switched off in a session.
- Without `--predict` the joiner sees its own Sora about one round trip late. With it the joiner's KH2 Sora
  answers within a tick, but a swing's hop toward a Sora in motion is often corrected, which shows as a short
  slide: see KHC-033. KH3's seats are not predicted, and a KH3 host's buttons do not wait.
- The host's own swing sounds come with its simulation, after the wait on its buttons, not with what it draws.
- KH2 has no victory pose. `--duel-victory stance|summon|none` (host) picks what the winner's KH2 Sora plays at the
  result: the game's own end-of-battle stance switch, the summon's appear motion, or nothing (the default until
  the owner picks, KHC-030). A knock-out slows the field to a third for a second on both screens.
- Internet play by direct room code needs the host's router to let a friend in (UPnP on, or a router that does not
  filter), and no router that gives each connection its own port on either side (KHC-051); a relay (below) gets through
  more routers. Packets are not encrypted yet (KHC-018). LAN discovery uses
  a broadcast that guest networks and some routers block; join by address then.

**Internet play (room codes, no server):** with no relay set, *Host with a room code* on the Multiplayer screen shows
a twelve-character code such as `ABCD-EFGH-JKMN`: it is the host's internet address, found through free public STUN
servers, and the game asks the router to open the port (UPnP or NAT-PMP, removed after the duel). The friend types it
under *Join by room code* and connects straight to the host. If the router gives every connection its own port, the
host is told there can be no code, and the friend can host instead. If the friend is not let in after a few seconds,
their game shows a *reply code*: the host types it into the waiting duel's window (Enter), and both games knock at once,
which gets through routers that block strangers (KHC-053). From a command line: `--host --room` and
`--room CODE` (KHC-051).

**Internet play (room codes through a relay):** `source/relay` builds `khc_relay`, a small rendezvous and relay server: `khc_relay 0.0.0.0:47621`.
Set its address on the launcher's Multiplayer screen (*Relay server*); *Host with a room code* then shows a six-letter code in the
duel, and the friend types it under *Join by room code*. The two try each other directly first and go through the relay if that
fails within a few seconds; the overlay says *Direct* or *Relay*. From a command line: `--host --relay HOST:PORT --room` and
`--relay HOST:PORT --room CODE`. Where a relay runs for real is not decided; nothing is published.

**Replays:** each session's matches are saved as a `.khcr` file in the `replays` folder beside the settings, and *Replays* on the launcher's Multiplayer screen plays them: Space pauses, the full stop steps one tick while paused, and 1, 2 and 3 are half, normal and double speed. A replay is what the joiner received, drawn by the joiner's own code from your install with nothing simulated again, so the camera is the joiner's. One from another version of the duel's protocol or another KH3 install is refused, with the reason; one from another build plays with a warning.

**Crashes:** when the game stops on an error it leaves a report in the `crash` folder beside the settings (the error, where, the build, the rules and the last ten seconds of duel events and network lines, without user names, personal paths or addresses), and the other player sees "Opponent's game stopped on an error" instead of a timeout. A panic on a helper thread is reported too, but the game goes on and the other player is not told.

**Two windows on one PC:** `--input kbm` makes a window read only the keyboard and mouse, `--input pad` (or `pad:2` for the second controller) only a controller, even while another window has the focus; `--input both` reads everything. The launcher's Multiplayer screen has the same choice (Controls). Its default, Auto, splits them by itself when a duel's two players are on the same PC: the host gets the keyboard and mouse and the joiner the controller.

## Packages

| Package | What it does |
|---|---|
| `khc` | `KHC.exe`: the launcher, and the room with the duel and the KH3 ruleset on it (`source/khc/src/room/`). |
| `net` | Sessions, wire format and link simulation over UDP. Moves bytes only. |
| `kh3_sora` | KH3's Sora, engine-free, driven by values read from your own KH3 files. |
| `kh3_ui` | KH3's command deck, shotlock display and gauges: reads their Scaleform movies from your own install and plays them. |
| `kh3_player` | KH3's moves as a layer over the `player` controller. |
| `kh3_data` | Readers for your own KH3 install (paks, packages, meshes, animations, sound). |
| `kh3_cascade` | KH3's Cascade particle evaluator. |
| `bbs_data` | Reads Birth by Sleep's map files from your own install, for the Mirage Arena. |

`KHC.exe` keeps its settings, manifests and saves in `%LOCALAPPDATA%\KHC`.

## Building

Everything builds from this repository: `source/engine` and the crates around it are the room, and the rest adds multiplayer.

`build.bat` builds `target\release\KHC.exe` and leaves no other executable in that folder. The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.0), with Bevy `=0.19.1`, glam `=0.32.1` and renet `=2.0.0`, the same as the engine.

**Checks:**

```
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

**Comparing runs** (`docs/tasks/KHC-024-noise-floor-and-run-comparator.md`): `cargo run -p validation --bin run_compare -- [--window TICKS] FIRST.jsonl OTHER.jsonl...` compares scripted runs' logs: hits, scores and knock-outs exactly, ticks within the window. With `--hash` it compares `--state-hash` files and names the first tick and group of fields that differ. The room's test flags for it, on an unattended run (`--autoplay`):

| Option | What it does |
|---|---|
| `--script-after-load` | The scripts start when loading is over, at the fight's start in a session |
| `--script-locked` | On both players: also takes the network's timing out, so a session can repeat |
| `--script-stop N` | The run ends N ticks after the script started |
| `--state-hash PATH`, `--state-dump PATH` | After each tick, a hash of the duel's state per group of fields; the text behind them |
| `--duel-seed N` | The Drive rolls' start, instead of the clock |
| `--duel-rival-pick kh2\|kh3` | With `--rival-script` and no session: the scripted rival's ruleset under `--duel-ruleset each` |
| `--duel-crash-at TICK` | This game panics at that tick, to try the crash report |

## Licence

Apache-2.0, see [LICENSE](LICENSE), the same licence as the code it grew from. Third-party notices are in [NOTICE](NOTICE)
and [CREDITS.md](CREDITS.md), and the third-party licence texts they name are in `docs/release/`. There is no GPL code
in this repository. The licence covers this project's code and documents only, not any game, its data or its
trademarks.
