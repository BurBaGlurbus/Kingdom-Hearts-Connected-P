//! Kingdom Hearts: Connected: multiplayer for Kingdom Hearts. Started
//! without arguments (a double-click) it opens the launcher; `KHC room
//! <install> <manifest> <config> [options]` runs the room directly.
//!
//! A Windows-subsystem program, so no console window opens behind the
//! launcher; output still reaches a pipe or redirect (the launcher reads the
//! room's output that way).
#![cfg_attr(windows, windows_subsystem = "windows")]
mod crash;
mod identity;
mod launcher;
mod names;
mod profile;
mod room;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    crash::install();
    // The engine's saves and settings live in this program's folder.
    engine::launcher::set_data_dir(launcher::data_dir());
    let mut args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.is_empty() {
        return launcher::main();
    }
    let program = args.remove(0);
    match program.to_string_lossy().as_ref() {
        "room" => room::main(args),
        "--version" => {
            println!("KHC {}", room::duel_net::VERSION);
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: KHC [room|--version] [room arguments]");
            ExitCode::FAILURE
        }
    }
}
