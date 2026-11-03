# Visuals and presentation

`src/pause_menu/` contains equipment, abilities, items and journal screens.
`hud.rs`, `gauge.rs` and `level_up.rs` draw the HUD; `room.rs` and `scenery.rs`
load scene presentation. `overlay.rs`/`overlay.wgsl` compose the 2D display.
`input.rs` and `controller.rs` adapt input. `lib.rs` lists the shared modules.
Menu/title state still lives here; see the architecture limitations.

See the [folder guide](../../docs/folder-guide.md) and [architecture](../../ARCHITECTURE.md).
