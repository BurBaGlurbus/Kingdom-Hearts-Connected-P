# KHC-029: Matching builds and installs

| | |
|---|---|
| Gate | Brief 3, WP-18: two players can see whether their builds and installs can play together, before they try |
| Owner | burba |
| Prerequisites | None. Must land before KHC-018 |
| Source | The player's Steam KH 1.5+2.5 and KH3, only read, at run time. One Windows PC, loopback. No Epic install here |
| Wire | PROTOCOL 8 |

## What was done

- `KHC.exe --version`, Hello, Welcome and Announce carry a readable version: the workspace version and a short commit (`source/khc/build.rs`). The launcher shows it on *Host a duel* and on a host of another version; the room's log has both on `net_connected`.
- Hello, Welcome and the new `CAPS` message carry each side's capabilities: KH3 and BBS, each absent, unread or a hash. `CAPS` goes both ways when the session starts and whenever a file finishes being read; `LobbyRoom::peer_caps` holds the other player's for Brief 5's lobby.
- A refusal has one line per mismatch: protocol, executable (both versions and both hashes' first bytes), KH3 files, BBS arena files (`net::wire::refusal`). No parentheses: the game's font has none.
- Another protocol's Hello is read as far as its name, so a player on an older or newer build gets a refusal and not a silent timeout. Hosts answer Discover from any protocol, so the list shows them with their version.
- **KH3's match hash narrowed.** It was the executable's size and every pak's index digest. It is now `Paks::read_digest`: a SHA-256 of each package read for the simulation (head, exports and bulk, whole, by path), the digest of the executable bytes the recipes read, and the book's names, which is what crosses the wire as indices. Files never read do not count. `Install::identity` remains as the key for what is remembered, and for a replay's header.
- **BBS arena hash added.** A SHA-256 of `arc/map/vs03.arc`, meshes and collision. Two installs that differ there stay in the test room's arena, with the reason.
- [duel choice, KHC-029] Effects, the HUD's movies and the arena's HD images are read after or apart from the hash: they are each player's own to draw, and no game rule reads them.
- Hashes come from what this session read. The last ones are remembered in the launcher's settings, only so a refusal, which comes before anything is read, can name them. The lobby decides on this session's alone.
- A draft `release` job in `ci.yml` runs `build.bat` and keeps `KHC.exe` with its SHA-256. It is `if: false`, and nothing is published.

## Results

Loopback, two instances of one build, KH3 ruleset: both sides logged `duel_caps` with the arena's hash at connect, then KH3's once read, about 10 s later. The two KH3 hashes agree, the KH3 ruleset stays on and the fight starts.

Between this build as host and the one before it as joiner, both on PROTOCOL 8, the joiner was refused at once with the executable's line and both versions.

**Found on the way, for the owner:** `kh3_data::sheets` already refuses any executable whose recipe bytes differ from one fixed digest (`EXE_DIGEST`), with offsets made on the Steam build. So an Epic player's KH3 ruleset is only available if the Epic executable has those same bytes at those same offsets. If it does not, the narrowed hash cannot help: that Epic install reports KH3 as absent and the duel stays on KH2. The answer needs an Epic install.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief3\`: `wp18-kh3`, `wp18-refuse`. Recipes: `scripts\brief3_build.sh`, `scripts\brief3_duo.ps1`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| New fields encode and decode, every datagram kind and `CAPS` | Agent | PASS |
| A refusal names both versions and each mismatch on its own line | Agent | PASS |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `build.bat`'s build | Agent | PASS, in the commit's run |
| Loopback refusal between two different builds shows both versions | Local | PASS: the joiner on bcb1b24 was told "Host: 0.1.0 2ea27de id 54ec46ef" and "You: 0.1.0 bcb1b24 id ddb21269" (`wp18-refuse`) |
| A Steam and an Epic KH3 install compared with the narrowed hash, differing files recorded | Owner | NOT RUN: no Epic install here |
| Frame rate within 5% (`--perf-overlay`) | Local | NOT RUN: no gameplay path changed |

## Questions for the owner, and what is in force

| Question | In force until answered |
|---|---|
| Should the internet beta keep the exact executable-hash match, or accept a protocol version plus a content hash? | The exact match |
| May a Steam and an Epic KH3 install play together when the bytes the ruleset reads match? | Yes, in the code; see the finding above |
| Where should release builds be published, if anywhere? | Nowhere: the job is off |

## Uncertainty

Which of KH3's files differ between stores is not known. The narrowed hash reads what this build's load reads: a later change to what the simulation reads changes the hash with it, as it should, and both players need the same build anyway. Hashing costs what SHA-256 costs over the packages read, on the loading thread; it was not timed apart.

## Verdict

PARTIAL. Built and PASS on loopback; the Epic comparison is the owner's.
