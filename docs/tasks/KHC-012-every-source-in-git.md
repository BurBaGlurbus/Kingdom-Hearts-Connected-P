# KHC-012: every source file in Git

| | |
|---|---|
| Gate | None; the owner's answer of 2026-11-01: "yes add them and the other thing" |
| Owner | burba |
| Prerequisites | KHC-005 |
| Source | No game files |

## What was done

KHC-005 said all the code was in this repository, but the default-deny `.gitignore` allowed only `source/*/Cargo.toml` and `source/*/src/**/*.rs`. All of `source/game`, `source/visual/effect`, `source/visual/benches`, the `.wgsl` files, the READMEs and the checker image stayed out, so a fresh clone could not build. The file list now allows them, and `target/` is anchored to the root because it also hid `source/game/enemy/src/traps/target/tests.rs`.

The owner also accepted `kh3_data/src/effect_defaults.rs` staying in Git: property names and default values read from the KH3 executable's constructors (KHC-011).

## Checks

| Check | Verdict |
|---|---|
| The 153 files by type: 131 `.rs`, 8 `.toml`, 7 `.wgsl`, 6 `.md`, 1 small `.bmp` | PASS, no assets or extracts |
| Search of them for personal paths, the owner's email, keys | PASS, only the word "animation" matched |
| `cargo build --release --locked -p khc` from what Git holds alone | PASS |
| `cargo test --locked` from what Git holds alone | PASS, 1,385 tests |
| A real `git clone` from GitHub | NOT RUN: not pushed |

## Uncertainty

The Rust files include routines translated from the KH2 executable, which the owner has allowed. Their provenance was checked by file type and search, not read line by line.

## Verdict

PASS for the checks above.

## Next

Push when the owner chooses; CI then builds these crates too.
