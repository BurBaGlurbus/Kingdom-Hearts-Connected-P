//! Enemy and boss behavior driven by the object scripts of the installed game.
//!
//! The scripts (BAR type 3 entries of `obj/*.mdlx`) are loaded from the user's
//! install at runtime; nothing here contains game data. `script` is the script
//! interpreter translated from the executable; the rest of the crate supplies
//! the engine services the scripts call.

pub mod act;
pub mod attack;
pub mod damage;
pub mod env;
pub mod friend;
pub mod limit_scene;
pub mod motion;
pub mod object;
pub mod reaction;
pub mod route;
pub mod rtn;
pub mod script;
pub mod sysobj;
pub mod trap_table;
pub mod traps;
pub mod world;
