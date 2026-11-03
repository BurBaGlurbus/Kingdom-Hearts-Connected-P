# KHC-062: Ready to go public? The pre-publish review

| | |
|---|---|
| Gate | burba: prepare the repository to go public and open source. Checks only: nothing pushed, no visibility, tag or GitHub setting changed |
| Owner | burba |
| Prerequisites | KHC-061 |
| Source | Every commit reachable from `main` at `70a8172` (51 commits), the tree at that commit, and KH2-Re-Written's public tree for its licence |
| Wire | none |

## What was done

- Added `SECURITY.md` (private reporting, what a room code reveals, host authority, no encryption, unverified ids), whitelisted in `.gitignore`.
- README: a new top (what KHC is, the trademark line, what you need, status, quick start with KHC-060's menu names, security), no logo image, the stale *Multiplayer* screen wording replaced, and a fuller licence line. The rest is unchanged.
- The third-party licence texts NOTICE and CREDITS name (`docs/release/bevy-mikktspace-MIT.txt`, `bevy-mikktspace-Zlib.txt`, `dotnet-runtime-LICENSE.txt`, `zlib-rs-LICENSE.txt`) were ignored, so a public clone would not have had them. They are now whitelisted and committed, copied unchanged from the same files in KH2-Re-Written.
- Licence: unchanged. `LICENSE` is byte-identical to KH2-Re-Written's Apache-2.0.

## Findings

### Blocking (before the repository is made public)

1. **The logo is Square Enix and Disney artwork.** `assets/khc-logo.png` (commit `866a83e`, in every commit after it) carries the Disney and Square Enix logos and Kingdom Hearts logo art. It cannot be redistributed under Apache-2.0. Removing it from the tree is not enough: it stays in history. Either rewrite history without it (`git filter-repo --path assets/khc-logo.png --invert-paths`, then a force push, which changes every hash after `866a83e`), or publish a fresh repository from one squashed commit. The launcher already runs without it (`launcher.rs` `logo_file` only looks for it), so a local copy beside the exe keeps working. **Removed** from the tree (third commit) at burba's word; it stays in this private history only.
2. **The relay's codes and keys can be predicted.** `source/relay` makes room codes and room keys with xorshift64\* from one seed. Its output is the state times a known odd constant, so one `Registered` answer (which hands the host its key) gives the state back, and with it every later code and key. With a predicted key, a stranger can send `Relayed` datagrams that move a room's host or joiner address to themselves (`Relay::forward`) and take over or cut someone's match, or look up rooms not yet shared. Fix: draw each code and key from the OS (`getrandom`). Only matters once a public relay runs; none does today. **Fixed** (second commit): codes and keys are now SipHash of a counter under a key `RandomState` draws from the OS, so one answer tells nothing about the next.

### Needs fixing (soon after, or before inviting strangers)

3. **LAN discovery answers the internet.** A host answers any `Discover` with an `Announce` (`session.rs`), from any address. With a direct code the router has opened the port to the internet, so a spoofed 7-byte `Discover` gets an `Announce` of 40 bytes plus the 32-byte build, version and name (up to about 120 bytes) sent to a victim: 10x or more amplification, and it shows the host's name and build. Fix: answer `Discover` only from private and link-local sources, or pad `Discover` to the size of the answer, as the relay's asks already are. **Fixed** (second commit): a host answers `Discover` only from private, link-local or loopback addresses.
4. **UPnP trusts any answer's URL.** `portmap::discover` takes the first SSDP reply's `LOCATION` and connects to whatever IP and port it names. A device on the same LAN can point it elsewhere. Fix: require the URL's host to be the address that answered, and a private one.
5. **Session nonces and the rejoin token** come from `RandomState` hashing (`random_bits`), which is not meant as a secret source. Prefer `getrandom` there too. Traffic is unencrypted, so anyone on the path can see the token anyway (see SECURITY.md).

### Recommended, not blocking

6. **KH3-derived numbers and names.** NOTICE says the repository stays private until they are reviewed. What is there: the recipe tables in `source/kh3_data/src/recipes` (all 26 hold only pointers into the player's install, `P|`/`T|`/`C|` asset paths and `X|f@` executable offsets, with no copied values), reflected enum names (`kh3_sora/src/gen`), tuning numbers taken from StrawHatPro's public MIT reimplementation, Unreal particle class defaults (`effect_defaults.rs`, mostly `bEnabled`), and executable addresses in comments. These are facts, names and addresses rather than game assets, the same kind of material public projects such as OpenKH and StrawHatPro's already publish. Low risk; burba's call. If accepted, change NOTICE's last sentence ("private until that has been reviewed").
7. **No game assets in history.** Apart from the logo, the only non-text files ever committed are `source/engine/assets/development-checker.bmp` (an 8x8 checker) and the source files. No paks, models, textures, sounds, saves or captures.
8. **No GPL code.** No Skate 3 file was ever committed. `skate_source` appears only as a package name in old `Cargo.lock` revisions (KHC-003 to KHC-005, when the engine was a git dependency); the remaining `skate` strings are KH2's own script and trap names (`n_ex740_skate_r`, `trap_skateboard_*`). All third-party code is Apache-2.0, MIT or Zlib, and NOTICE and CREDITS cover it.
9. **No secrets or private addresses.** No tokens, keys, passwords or private key blocks in any commit. IP addresses are loopback, documentation ranges or example LAN addresses in tests. No personal Windows paths (the only user path is the test fixture `Riku Replica`). All commits use the GitHub noreply address.
10. **What a malicious player can do, by design:** the joiner only sends buttons, and every decoder bounds its counts and lengths, so a joiner can at most send bad buttons. The host simulates the match, so a modified host can decide it. Build hashes and player ids are claims, not proof. SECURITY.md says so.
11. The KHC-003 to KHC-005 commits name KH2-Re-Written as a git dependency (`Cargo.lock`, rev `f3b1bf1`). Not a licence problem, as both are Apache-2.0; a squashed fresh repository would also drop it, if the link should not show.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Every blob in all 51 commits listed by path and size; non-text files viewed | Agent | PASS: one problem file (the logo) |
| Recipe tables: every value is a pointer, none a copied number | Agent | PASS |
| History search for Skate 3, GPL, LGPL and copyleft text | Agent | PASS |
| History search for tokens, keys, passwords, private key blocks, IP addresses, user paths, author emails | Agent | PASS |
| Read of `source/relay`, `net::wire`, `net::direct`, `net::session` (Hello, Discover, relayed, STUN), `net::portmap`, `duel_wire` decoders | Agent | Findings 2 to 5 |
| `cargo` builds and tests (first commit) | Agent | NOT RUN: documents and `.gitignore` only |
| Second commit: `cargo fmt`, `cargo clippy -p relay -p net --all-targets --locked -- -D warnings`, `cargo test -p relay -p net --locked` (Linux) | Agent | PASS |
| Second commit: a full Windows `cargo test` and `build.bat` | Agent | NOT RUN |

## Uncertainty

This is not legal advice. Trademark use in the name "Kingdom Hearts: Connected" is a risk every fan project takes; the README says it is unofficial. The relay findings were read from code, not exploited.

## Verdict

PASS for the tree: burba had the logo removed (third commit), so nothing left blocks publishing but burba's go-ahead. Finding 2 and 3 fixed. burba chose to publish as a fresh repository from one squashed commit, which leaves the logo's history behind; the logo itself must still be left out of that tree.
