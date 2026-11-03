# KHC-044: Identity and profile

| | |
|---|---|
| Gate | Brief 6, WP-23: a stable player id, a name hard to confuse, a local profile |
| Owner | burba |
| Prerequisites | none |
| Source | No game data |
| Wire | PROTOCOL 14: Hello and Welcome carry the player id |

## What was done

- `hub::identity`: a random 256-bit secret from the system's randomness (`getrandom`, already in the lock through other crates), and the id: the first 128 bits of its SHA-256. A recovery code: the secret and a two-byte check in Crockford's base 32, in groups of five. The secret's debug output is redacted.
- `hub::names`: 3 to 16 of ASCII letters, digits, space, `_` and `-`; no space at either end or doubled; a small blocklist (`source/hub/blocklist.txt`, whole-name and anywhere entries, folding case and digit swaps), checked by the launcher and the server. `Name#4821` from the id.
- `khc/src/profile.rs`: `profile.json` in the data folder, version 1: the id and secret, the name and whether it was confirmed, hub settings, card choices (favorite Keyblade, title, frame, badge, visibility), friends, muted and blocked ids, recent opponents. Made on first launch; the launcher's old name is kept only if it passes the rules, else `Player` and four digits. A file of another version, or whose id is not its secret's, is refused and left as it is.
- The launcher: the Name row shows `Name#4821` and the rules, types only allowed characters and says why a name is refused; Recovery code, Restore from a code, Reset identity and Delete my profile (the last two need a second press). The Windows user name is never the default any more (`duel_net::default_name` reads the profile).
- PROTOCOL 13 to 14: Hello and Welcome carry the 16-byte id; `Peer::player` has the other side's.

## Signature: proposed, not added

Nothing proves a player holds the secret behind the id they send. Proposed: Ed25519 with `ed25519-dalek` (RustCrypto family, pure Rust, MSVC): the secret seeds the key, the public key's hash becomes the id, and the hub's Join and the duel's Hello carry a signature over a server nonce. This is the same approval Brief 3's encryption waits on (KHC-018), so one answer covers both. Until then ids are unverified (`Welcome::verified` false) and a public hub keeps levels and friends off.

## Evidence

Unit tests only; no game data is involved.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Name rules and blocklist, the `#number`, the default name | Agent | PASS |
| Migration from the launcher's name, kept only when it passes | Agent | PASS |
| The secret never in debug output | Agent | PASS |
| A profile of an unknown version, or with a foreign id, refused and left alone | Agent | PASS |
| Recovery code round trip; one character changed is refused | Agent | PASS |
| Every duel datagram round trips at PROTOCOL 14 | Agent | PASS |
| The launcher shows the name field, its rules and the default | Local | NOT RUN: no capture of the launcher |
| Ask for a name the first time the hub opens | Local | NOT RUN: the launcher says the name is a default; the hub does not ask yet |
| The signature dependency | Owner | BLOCKED: waits for the answer above |
| "Delete my profile" asks the server to forget its record | Agent | PASS on the server; the launcher deletes the file only, offline |
| `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --locked` | Agent | PASS, in the commit's run |
| `build.bat` | Local | NOT RUN: it waits for a key; the same release build ran through `brief6_build.sh` |

## Verdict

PARTIAL. Ids, names and the profile work; signing is BLOCKED on the owner.
