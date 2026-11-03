//! Saving and loading the way the Windows build of KH2 Final Mix does it:
//! one PNG container per Steam account holding a virtual memory card
//! ([`container`]), one 0x10fc0-byte save image per slot ([`data`]), and the
//! list of 99 slots the file menu shows ([`slots`]).
pub mod cipher;
pub mod container;
pub mod data;
pub mod md5;
pub mod menu;
pub mod slots;
pub mod state;
pub mod store;

/// Name of save slot `slot` (0-based) inside the container
/// (`/BISLPM-66675FM-%02d/BISLPM-66675FM-%02d`, data at 0x1405ba4c1).
pub fn slot_name(slot: usize) -> String {
    format!("BISLPM-66675FM-{slot:02}")
}

/// The system file next to the slots (data at 0x1405ba679).
pub const SYSTEM_NAME: &str = "BISLPM-66675FM-SYS";
