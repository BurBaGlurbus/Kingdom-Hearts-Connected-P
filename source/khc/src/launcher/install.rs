//! The launcher's non-visual half: the engine's folder check and made configs
//! (`engine::launcher::install`), with what is this launcher's own: its private
//! folder, its settings and the duel's arguments.
//!
//! The launcher's check only decides what to offer; every program still
//! admits its source itself (`ProbeSource::bind_session` +
//! `verify_session`) when it starts.
pub use engine::launcher::install::{
    Bound, detect_installs, find_game_below, has_executable, steam_libraries, supported_identity,
};
use engine::launcher::install::{
    CONFIG_LIMIT, SETTINGS, config_identity, launch_file, made_config, read_limited,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The private folder for launch files, configs, manifests and the
/// launcher's settings: `.local/test-room` beside the checkout the
/// executable was built in, else `%LOCALAPPDATA%\KHC`.
pub fn data_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        for up in exe.ancestors().skip(1).take(4) {
            let dir = up.join(".local/test-room");
            if dir.is_dir() {
                return dir;
            }
        }
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|local| PathBuf::from(local).join("KHC"))
        .unwrap_or_else(|| PathBuf::from(".local/test-room"))
}

/// What the launcher remembers between runs.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub game_folder: Option<PathBuf>,
    pub manifest: Option<PathBuf>,
    /// Quick start has been finished once.
    #[serde(default)]
    pub guide_seen: bool,
    /// The name the other player sees in a LAN duel (the Windows user name
    /// until changed) and the last host address typed.
    #[serde(default)]
    pub player_name: Option<String>,
    #[serde(default)]
    pub last_host: Option<String>,
    /// The rendezvous and relay server for internet duels (`HOST:PORT`), and the
    /// last room code typed.
    #[serde(default)]
    pub relay_server: Option<String>,
    #[serde(default)]
    pub last_room: Option<String>,
    /// [KHC-016] A rules code to host with (the duel lobby shows the host's).
    #[serde(default)]
    pub duel_code: Option<String>,
    /// [KHC-058] Preload everything when a game starts; None: not asked yet.
    #[serde(default)]
    pub preload: Option<bool>,
    /// The duel's devices (`--input`): auto, kbm, pad or both.
    #[serde(default)]
    pub duel_input: Option<String>,
    /// The player's KINGDOM HEARTS III folder (the duel's KH3 option reads
    /// it), and where its pak key sits in the executable. The key itself is
    /// never kept.
    #[serde(default)]
    pub kh3_folder: Option<PathBuf>,
    #[serde(default)]
    pub kh3_key_offset: Option<u64>,
    /// What each game's files last hashed to, with what identifies the install
    /// it was read from: only for a refusal to name before they are read again.
    #[serde(default)]
    pub kh3_digest: Option<(String, String)>,
    #[serde(default)]
    pub bbs_digest: Option<(String, String)>,
}

pub fn load_settings(dir: &Path) -> Settings {
    read_limited(&dir.join(SETTINGS), CONFIG_LIMIT)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Written beside and renamed over, so a crash never leaves half a file.
pub fn save_settings(dir: &Path, settings: &Settings) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let next = dir.join(format!("{SETTINGS}.new"));
    fs::write(&next, serde_json::to_vec_pretty(settings)?)?;
    fs::rename(next, dir.join(SETTINGS))
}

/// `KHC room` for a LAN duel: the arena config (the developer's own
/// launch file, else the made one) with the session's options after it.
pub fn duel_args(dir: &Path, b: &Bound, session: &[String]) -> Result<Vec<String>, String> {
    let own = launch_file(dir, "launch")
        .ok()
        .map(|l| (dir.join(&l.config), l.options))
        .filter(|(config, _)| config_identity(config).as_deref() == Some(b.identity.as_str()));
    let (config, options) = match own {
        Some(own) => own,
        None => (made_config(dir, b, "room")?, Vec::new()),
    };
    if config_identity(&config).as_deref() != Some(b.identity.as_str()) {
        return Err(
            "The duel's config was made for a different build than the chosen folder.".into(),
        );
    }
    let mut args = vec![
        "room".to_owned(),
        b.folder.to_string_lossy().into_owned(),
        b.manifest.to_string_lossy().into_owned(),
        config.to_string_lossy().into_owned(),
    ];
    args.extend(options);
    args.extend(session.iter().cloned());
    Ok(args)
}
