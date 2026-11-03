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

**From a command line**, add these to `KHC room <Steam-root> <manifest> <config>`:

| Option | What it does |
|---|---|
| `--host` | Host and wait for a player (UDP port 47620) |
| `--port N` | Host on, or join, another port |
| `--bind IP` | Host on one address only (`127.0.0.1` for two instances on one PC) |
| `--join HOST[:PORT]` | Join the host at that address |
| `--name NAME` | The name the other player sees |
| `--net-sim LATENCY,JITTER,LOSS` | Add latency (ms), jitter (ms) and loss (%) to what this instance sends |
| `--net-overlay` | Show the network overlay |
| `--net-log PATH` | Write the network's numbers to a file, one JSON line a second, with bytes by message kind |
| `--snapshot-rate 60\|30` | Host: snapshots a second (60 by default; 30 halves the upload and shows the opponent about 67 ms late) |
| `--predict` | Joiner: its own KH2 Sora moves at once and the host's snapshots correct him (off by default) |
| `--latency-match`, `--no-latency-match` | Host: its own buttons wait as long as the joiner's take to arrive, or do not (on by default; a tick or so on a LAN) |
| `--duel-dodge attacker\|defender` | Host: whether a dodge begun after what the joiner saw still saves the host's Sora (default attacker) |
| `--replay FILE` | Play a saved duel |
| `--replay-out PATH` | Save this session's replay there (default: a new file in `replays`; an unattended run saves none) |
| `--duel-damage N` | Host: multiply the damage of attacks (hits on Strength) by N (0.5 to 10, default 3) |
| `--duel-magic-damage N` | Host: multiply the damage of magic (spells, Reflect's counter, form magic) by N (0.5 to 10, default 1.5) |
| `--duel-hit-cap PCT` | Host: no single hit takes more than PCT % of max HP (20, 25, 30, 35, 40, 50, 60, 75; 100 for none; default 40) |
| `--duel-arena mirage\|room` | Host: fight in the Mirage Arena (default) or the test room's arena |
| `--rival-script` | No network: a second Sora run by a test script, to try the duel alone. With `--join`: the script presses the joiner's buttons (attacks, Fire, Thunder, a Potion, Valor Form) |

In the room, F3 → Sora has *Network overlay* (ping, loss, data rates, buffered snapshots and warnings, top
right), *Simulated network* (Off, Wi-Fi, Busy Wi-Fi, Bad link) and, outside a session, *Scripted rival Sora*.

**Two instances on one PC:** start one with `--host --bind 127.0.0.1` and the other with `--join 127.0.0.1`,
or host from one launcher window and join *127.0.0.1* from a second. The window in the background draws at 30
FPS, but its game still runs at 60 ticks a second. To try Wi-Fi conditions, choose the same *Simulated network*
on both.

**How it works:** the host runs the game for both Soras on the fixed 60 Hz tick. The joiner sends its buttons
every tick (repeating the last six, so a lost packet costs nothing) and draws the host's snapshots a little
behind, by as much as the link's jitter needs (two to eight snapshots), gliding between them. Hit sounds, sparks, guard clashes and knock-outs reach the joiner as reliable events
and play there; nothing cosmetic is streamed. Spells, items and Drive forms work the same way: the joiner's
command goes to the host with its buttons, the host runs it, and the effects it starts reach the joiner as events
naming the effect file, so each side draws them from its own install. Esc leaves; the other side sees "Opponent
left" (or a timeout after five seconds) and the host waits for the next player. During a match a joiner whose link
drops keeps the seat for 30 seconds: the match pauses after a second of silence, both screens say so, and the joiner's game
knocks again by itself, from a new address if need be. A host whose link drops cannot be waited for, as the host runs the
match: the joiner's game keeps knocking for the 30 seconds and then ends the duel.

**The lobby:** when the other player joins, the game's own pause menu opens on its *Duel* pages (L2/R2 turn
the pages, Up/Down choose a row, Left/Right change it; the help line says what a row does, and where the other
player is shows on each page):

1. *Match*: the host sets the arena, the *Ruleset* (*KH2*, *KH3*, or *Free pick*, where each player picks their
   own on the Ready page), a *Preset* (*Standard*, *Quick KO*, *Long fight*: Best of 3 at x2 with a 30% cap,
   *Timed 90*; changing a rule makes it *Custom*), the mode (*One KO*, *Best of 3*, *Time limit*, where the larger
   share of HP left wins at time up), *Damage* (attacks), *Magic damage*, *Hit cap* and the time limit. The joiner
   sees them and cannot change them. *Rules code* shows the host's rules and bans as a code (on the help line):
   type it under *Rules code* on the launcher's Multiplayer screen, or pass `--duel-code CODE`, to host with them
   again. A code from another version or with a typo is refused.
2. *Bans*: each player bans up to two abilities for this match and confirms (no ban is a choice too). A ban
   takes the ability away from both players. Each player's bans stay hidden from the other until both have
   confirmed.
3. *Rulesets*: a card for each ruleset: its game, its gauges and a few of its moves.
4. *Ready*: each player picks a Keyblade (any of the 24, each with its own stats and ability), their own ruleset
   under *Free pick* (the other player's shows below it), and sees the AP used against the limit. While the builds are made the menu's own *Abilities* (up to 166 AP, level 99 with
   every AP Boost the game allows) and *Items* (Equip and the item slots) are on the top level. The host checks
   every build; a refusal shows on the help line.
5. *Ready* on both starts the match. A rule change takes Ready back. Start toggles Ready from anywhere in the
   lobby's menu while the builds are made.

The lobby's menu also has *Leave* (back to the Multiplayer screen), and the other player's row shows where they
are and their round trip in ms. The host's rules and each player's last readied build (Keyblade, abilities,
items) are kept for the next duel in `duel-prefs.json` beside the source manifest (version 2; an older file is
read and migrated); anything the next duel does not allow is left out.

**The arena:** by default the match is fought in the **Mirage Arena** from Birth by Sleep Final Mix, read
from your own install (the same Steam collection holds both games): its round stage, its sky and its walls,
scaled to KH2's size. The host can pick the test room's arena on the lobby's Match page instead. Both players
need the same arena files from Birth by Sleep in their install for the Mirage Arena; if either install cannot show it, or the two differ, the duel stays in
the test room's arena and the lobby says why. The first time, only the parts of Birth by Sleep's package the
arena reads are recorded (a few megabytes, no full scan); later sessions check them. Both Soras start 600 units apart, facing each
other, with the camera behind each player's Sora. There are no enemies, no talk and no exits during a duel.

**The music:** while you are in the duel's arena (the lobby, the countdown and the fight) one of the game's
battle themes plays, a new one each match and the same on both PCs; it fades out at the result.

**The match:** a short shot of both Soras, then 3, 2, 1 and GO on both screens at the same moment; nobody
moves before GO. A knock-out ends a round (a knocked-out Sora stays down). *One KO* is one round; *Best of 3*
goes until someone wins two (the round and score show at the top); *Time limit* shows the clock, and at time
up the larger share of HP left wins. Between rounds both Soras are refilled and put back at the start. At the
end each screen shows VICTORY, DEFEAT or DRAW and how it was decided over the field, and the pause menu with
*Rematch*, *Leave* and *Results* (the rounds, the match time, the damage dealt and the hits landed); the
winner's camera pushes in, the loser's rises behind his Sora. *Rematch* on both
goes back to the lobby (new bans, the builds where they were); *Quick rematch* on both starts the next match
at once with the same builds and bans (either player's plain *Rematch* goes to the lobby); *Leave* closes the duel, and the launcher comes
back on its Multiplayer screen. If the other player leaves in the lobby or the camp, the camp closes without
keeping the duel's build and the room's own comes back. If the other player
leaves during a match, it is your win. Start during a match opens *Keep fighting* / *Forfeit* (the fight goes
on meanwhile); *Forfeit* asks once more, then gives the match to the other player. From a command line the host can set `--duel-mode stock|best3|time` and
`--duel-time 60|90|120|180|300`.

F3 → Sora → Duel → *Test vote* (host) tries the vote both players will use for later choices, in the pause
menu: different answers go to a coin flip that both screens show on the help line. The coin is both players':
each sends a hash of a random number, then the number, and the two make the seed, so neither can choose the
result.

**The duel build:** for a session both Soras fight at level 99 with every bonus level's HP, MP, Drive bars,
item slots and abilities, the top level of every spell, Valor, Wisdom, Master and Final Form at their level cap
(Limit Form is off), a full Drive gauge, and 99 of every consumable (Potion to Megalixir; the stat boosts, Tents
and Drive Recoveries are left out). Nothing is read from or written to a save; the room's own build comes back
when the session ends. Damage between the two Soras is the game's formula times the host's rules: attacks ×3
by default (`--duel-damage N` or F3 → Sora → Duel), magic ×1.5 (`--duel-magic-damage N`), and no single hit
takes more than 40 % of max HP (`--duel-hit-cap PCT`). The formula is Strength or Magic minus Defence, so at
level 99 the Keyblade's stats count for a lot: with the defaults a Thundaga does 24 on the Kingdom Key and 90 on
the Keyblade with the most Magic, and neither takes a full bar in one cast. The other Sora's HP stays on the target gauge (top right) for
the whole duel. The lobby's Keyblade, abilities and item slots go on top of it.

**KH3 ruleset (multiplayer only; needs your own KINGDOM HEARTS III):** set the game's folder on the launcher's
quick start, test list or Multiplayer screen (the *KINGDOM HEARTS III* row finds a Steam or Epic install, or lets
you browse to it). Then the lobby's Match page offers *Ruleset*: *KH2* (the game's own moves, the default), *KH3*, or
*Free pick* (`each`), where each player picks KH2 or KH3 under *Your ruleset* on their Ready page and the two
fight each other with their own moves. The host sets the policy (`--duel-ruleset kh2|kh3|each`, `--kh3-folder
<folder>` from a command line). While either install has no KH3, or the two differ, only *KH2* is offered and the
row says why. A match with KH3 in it needs
the same KH3 bytes on both PCs, checked by a hash of every file the fight reads, so other differences between two installs block nobody; only ids cross the network. Nothing from KH3 is
in this project or copied anywhere: its files are read from your install each time (the pak key is found in your own
executable and kept in memory). The room waits a few seconds while they load.

In *KH3* both Soras are KH3's Sora with the Kingdom Key, with KH3's animations and their blends, and fight with
KH3's data: its combos (with the abilities that change them), hit windows and shapes, attack rows and damage
formula, magic from Fire to Aero with its tiers, magic combos and Grand magic, KH3's effects and sounds. Moves:
KH3's jump arc and gravity (a held jump goes higher; holding Jump after the top glides), *Doubleflight* (Jump again
near the top), *Dodge Roll* and *Air Slide* (Guard with the stick), *Glide*, *Superslide*, and flowmotion: run or
fall into a wall with the stick on it to run up or along it (Jump leaves with a flow jump, Guard kicks off into a
Superslide, Attack is a wall attack), and poles where the arena has narrow pillars (Attack is Buzz Saw). Swings home
in on the other Sora and air combos hover; Jump during a chain that has landed a hit launches (Rising Spiral) or
strikes down (Groundbreaker); a chain only goes on once one of its hits landed. Attack right after a successful
guard is a counter (Counter Slash, Impact or Kick), one that blows the other away offers Final Blow, and Jump after
guarding a heavy hit is Risk Dodge with its counter. **Shotlock:** hold R2 (E on the keyboard) to aim. Locks build up
while focus drains (the bar above your HP); Attack fires King of Hearts, or *Ragnarok* at full locks. Guard while
aiming airsteps to the other Sora, and Attack on arrival is a flow attack. Enough `m_FormPoint`s from hits offer the
*Grand* spell of the element used most: Triangle casts it without MP. KH3's abilities and their AP are on the lobby's
pages, and on START in *Test KH3 mode* on the launcher's Multiplayer screen (or `--kh3-test`), a local room with a
training dummy to hit. Left out: Attractions, Keyblade transformations and Formchanges, Links, team attacks, rail
slides, status effects (freeze, discharge), and the survival, MP and system abilities. What is read from KH3 and what
is approximated: `docs/tasks/T282-kh3-combat-mod.md`.

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
