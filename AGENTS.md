# Kingdom Hearts: Connected

Multiplayer for Kingdom Hearts. Read `README.md` and the latest record in `docs/tasks/` before work.

- All the code is here: `source/engine` and the crates around it are the room, and `source/khc`, `source/net` and the `kh3_*` crates add multiplayer and the KH3 ruleset. New multiplayer code goes through `extension.rs` (`RoomHooks`, `RoomExtension`, `RoomSet`) where it can, so the room stays readable.
- Never run a writer or mod manager against an installed game. No game data, executable extracts, captures, saves, secrets or personal paths in Git. Tests use generated synthetic bytes.
- Do not push or publish until the owner says so. This repository holds KH3-derived numbers and names (see `NOTICE`).
- Checks: `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `build.bat`.
- Nontrivial work gets a `KHC-NNN` record in `docs/tasks/`: gate, owner, source, steps, evidence, checks, uncertainty and verdict. Use PASS, FAIL, NOT RUN and BLOCKED honestly.
- Display name `Kingdom Hearts: Connected`; short name and prefix `KHC`.
