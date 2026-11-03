//! KH3's own command deck and shotlock display: a small Scaleform GFx timeline player, engine-free.
//! Ported from StrawHatPro's kh-ui (MIT; see NOTICE). The movies, textures and fonts come from the
//! player's own install at run time (`load`); none of them is in this repository.
#![allow(clippy::all, unused)]

pub mod command;
pub mod controls;
pub mod def;
pub mod frames;
pub mod geom;
pub mod gfx;
pub mod hud_command;
pub mod hud_status;
pub mod hud_target;
pub mod list;
pub mod load;
pub mod player;
pub mod raster;
pub mod render;

pub use def::{MovieDef, Pack, PackError};
pub use geom::{Cx, Matrix, Rect};
pub use player::Movie;
pub use raster::Canvas;
pub use render::{Fonts, Renderer};

/// A UI sound request: a static SE index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSound {
    /// One shot.
    Play(u32),
    /// A loop that runs until `LoopStop` of the same index.
    LoopStart(u32),
    LoopStop(u32),
}
