# Game file readers

This contains code, not game assets. `src/model_*.rs` reads models/textures,
`motion*.rs` reads animation, `room_*.rs` reads room geometry/collision, and
`*_table.rs` reads tables. `message*.rs`/`font*.rs` handle text. `object_effect.rs`
reads effects. `bar.rs`, `ard.rs`, `ptya.rs` and `scd.rs` identify game formats;
see the folder guide. `src/bin/` holds inspection commands; `examples/` holds
generated boundary checks. Gameplay rules belong in `game/`.

See the [folder guide](../../docs/folder-guide.md) and [architecture](../../ARCHITECTURE.md).
