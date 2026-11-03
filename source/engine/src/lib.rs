//! The engine: the room and the launcher's parts. `khc` builds on the room
//! through `test_room`'s public items.
#![allow(clippy::new_without_default, clippy::len_without_is_empty)]

pub mod area;
pub mod chests;
pub mod cutscene;
pub mod early_renderer;
pub mod image_inspector;
pub mod keyblades;
pub mod launcher;
pub mod load_timer;
pub mod model_inspector;
pub mod npcs;
pub mod pc_controls;
#[cfg(feature = "perf-diag")]
pub mod perf;
pub mod save_files;
pub mod sephiroth;
pub mod test_room;
pub mod world_map;
