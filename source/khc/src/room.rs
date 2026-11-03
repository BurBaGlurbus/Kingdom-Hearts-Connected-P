//! The room with multiplayer and the KH3 ruleset on it: the engine's room
//! (`engine::test_room`, seen from here as if these modules were its own), and
//! what Kingdom Hearts: Connected adds through the room's hooks.

pub use duel_net::build_identity;
pub use engine::test_room::*;
pub use online::Slot;

pub mod duel_arena;
pub mod duel_bench;
pub mod duel_camp;
pub mod duel_delay;
pub mod duel_fx;
pub mod duel_hash;
pub mod duel_hits;
pub mod duel_kh3;
pub mod duel_loadout;
pub mod duel_lobby;
pub mod duel_lobby_room;
pub mod duel_log;
pub mod duel_match;
pub mod duel_match_room;
pub mod duel_music;
pub mod duel_net;
pub mod duel_playout;
pub mod duel_predict;
pub mod duel_prefs;
pub mod duel_replay;
pub mod duel_rewind;
pub mod duel_rules;
pub mod duel_script;
pub mod duel_sync;
pub mod duel_vote;
pub mod duel_wire;
pub mod kh3_autose;
pub mod kh3_book;
pub mod kh3_camera;
pub mod kh3_core;
pub mod kh3_deck;
pub mod kh3_fx;
pub mod kh3_hits;
pub mod kh3_menu;
pub mod kh3_model;
pub mod kh3_post;
pub mod kh3_retarget;
pub mod kh3_shaded;
pub mod mirage_arena;
pub mod online;
pub mod preload;
pub mod rival_play;
pub mod ruleset;

/// Runs the room with these options; `online.rs` takes its own out of them.
pub fn main(args: Vec<PathBuf>) -> std::process::ExitCode {
    match run(args, Some(Box::new(online::OnlineRoom::default()))) {
        Ok(AppExit::Success) => std::process::ExitCode::SUCCESS,
        Ok(_) => std::process::ExitCode::FAILURE,
        Err(e) => {
            eprintln!("The room rejected: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
