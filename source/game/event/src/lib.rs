//! Event (cutscene) playback state, independent of presentation: which
//! camera, actor animation, position, subtitle and sound is current at an
//! event frame. Built from a decoded script (`data::event_script`).
pub mod event_camera;
pub mod timeline;

pub use timeline::Timeline;
