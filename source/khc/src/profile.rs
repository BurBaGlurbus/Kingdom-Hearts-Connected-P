//! The player's profile: `profile.json` in the data folder (WP-23, KHC-044).
//!
//! It holds the identity (a random secret made on first launch and the id
//! from it), the name, and the friends, muted and blocked players. The secret never reaches a log: its
//! debug output is redacted, and the file is the only place it is written.
//! A file with a version this build does not know is refused, not rewritten.

use crate::identity::{PlayerId, SECRET_BYTES, Secret};
use crate::names;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROFILE_VERSION: u32 = 1;
pub const FILE: &str = "profile.json";

/// A friend as last seen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Friend {
    pub id: String,
    pub name: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    /// The secret, in hex. Only this file holds it.
    secret: String,
    /// The id, in hex: checked against the secret on reading.
    pub id: String,
    pub name: String,
    /// The player confirmed the name.
    #[serde(default)]
    pub name_confirmed: bool,
    #[serde(default)]
    pub friends: Vec<Friend>,
    #[serde(default)]
    pub muted: Vec<String>,
    #[serde(default)]
    pub blocked: Vec<String>,
    /// The last 20 opponents (id, name), newest first.
    #[serde(default)]
    pub recent: Vec<Friend>,
}

impl std::fmt::Debug for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("version", &self.version)
            .field("secret", &"redacted")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("name_confirmed", &self.name_confirmed)
            .field("friends", &self.friends.len())
            .field("muted", &self.muted.len())
            .field("blocked", &self.blocked.len())
            .finish()
    }
}

/// A fresh secret from the system's randomness.
pub fn new_secret() -> Result<Secret, String> {
    let mut s = [0; SECRET_BYTES];
    getrandom::fill(&mut s).map_err(|e| format!("no system randomness: {e}"))?;
    Ok(Secret(s))
}

impl Profile {
    /// A new profile for `secret`, named from the launcher's old name when it
    /// passes the rules, else `Player` and four digits.
    pub fn new(secret: Secret, old_name: Option<&str>) -> Profile {
        let id = secret.id();
        let migrated = old_name
            .map(str::trim)
            .filter(|n| names::check(n).is_ok())
            .map(str::to_owned);
        Profile {
            version: PROFILE_VERSION,
            secret: secret.hex(),
            id: id.hex(),
            name: migrated.unwrap_or_else(|| names::default_name(&id)),
            name_confirmed: false,
            friends: Vec::new(),
            muted: Vec::new(),
            blocked: Vec::new(),
            recent: Vec::new(),
        }
    }

    pub fn secret(&self) -> Option<Secret> {
        Secret::from_hex(&self.secret)
    }

    pub fn player_id(&self) -> PlayerId {
        PlayerId::from_hex(&self.id).unwrap_or_default()
    }

    /// `Name#4821`.
    pub fn tagged(&self) -> String {
        names::tagged(&self.name, &self.player_id())
    }

    pub fn recovery_code(&self) -> Option<String> {
        self.secret().map(|s| s.recovery_code())
    }

    /// Sets the name if it passes the rules.
    pub fn rename(&mut self, name: &str) -> Result<(), String> {
        names::check(name).map_err(|r| r.text())?;
        self.name = name.to_owned();
        self.name_confirmed = true;
        Ok(())
    }

    /// A new id and secret; friends and blocks belong to the old id and go.
    pub fn reset_identity(&mut self, secret: Secret) {
        let name = self.name.clone();
        *self = Profile::new(secret, Some(&name));
        self.name_confirmed = true;
    }

    /// The secret from a recovery code, typed on a new PC.
    pub fn restore(&mut self, code: &str) -> Result<(), String> {
        let s = Secret::from_recovery_code(code).ok_or("That recovery code does not read.")?;
        self.secret = s.hex();
        self.id = s.id().hex();
        Ok(())
    }

    pub fn parse(text: &str) -> Result<Profile, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let version = v.get("version").and_then(|v| v.as_u64());
        if version != Some(u64::from(PROFILE_VERSION)) {
            return Err(format!(
                "profile version {} is not one this build reads ({PROFILE_VERSION})",
                version.map_or("none".into(), |v| v.to_string())
            ));
        }
        let p: Profile = serde_json::from_value(v).map_err(|e| e.to_string())?;
        let secret = p.secret().ok_or("the profile's secret does not read")?;
        if secret.id().hex() != p.id {
            return Err("the profile's id is not its secret's".into());
        }
        Ok(p)
    }
}

/// The folder the profile lives in: the data folder, or [Test]
/// `KHC_PROFILE_DIR`, so two programs on one PC are two players.
pub fn dir() -> PathBuf {
    std::env::var_os("KHC_PROFILE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::launcher::data_dir)
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// The profile in `dir`, made (and saved) on first use from the launcher's
/// old name. An unreadable file is an error and is left as it is.
pub fn load_or_create(dir: &Path, old_name: Option<&str>) -> Result<Profile, String> {
    let file = path(dir);
    match std::fs::read_to_string(&file) {
        Ok(text) => Profile::parse(&text).map_err(|e| format!("{}: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let p = Profile::new(new_secret()?, old_name);
            save(dir, &p)?;
            Ok(p)
        }
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

pub fn save(dir: &Path, p: &Profile) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    // Beside the file, then over it: a crash never leaves half a file.
    let file = path(dir);
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &file).map_err(|e| format!("{}: {e}", file.display()))
}

/// "Delete my profile": the file goes.
pub fn delete(dir: &Path) -> Result<(), String> {
    match std::fs::remove_file(path(dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(n: u8) -> Secret {
        Secret([n; SECRET_BYTES])
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("khc-profile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_new_profile_takes_a_valid_old_name_or_a_default() {
        let p = Profile::new(secret(1), Some("Sora"));
        assert_eq!(p.name, "Sora");
        assert!(!p.name_confirmed);
        // A Windows user name with characters the rules refuse is not kept.
        let q = Profile::new(secret(1), Some("jean.dupont@corp"));
        assert_eq!(q.name, names::default_name(&secret(1).id()));
        assert!(q.name.starts_with("Player"));
        let r = Profile::new(secret(1), None);
        assert_eq!(r.name, q.name);
        assert_eq!(r.player_id(), secret(1).id());
        assert!(p.tagged().starts_with("Sora#"));
    }

    #[test]
    fn the_secret_stays_out_of_debug_output() {
        let p = Profile::new(secret(9), Some("Sora"));
        let shown = format!("{p:?}");
        assert!(!shown.contains(&secret(9).hex()));
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn files_round_trip_and_other_versions_are_refused() {
        let dir = temp("files");
        let p = load_or_create(&dir, Some("Riku")).unwrap();
        assert_eq!(p.name, "Riku");
        assert_eq!(load_or_create(&dir, Some("Other")).unwrap(), p);
        let text = std::fs::read_to_string(path(&dir)).unwrap();
        let other = text.replacen("\"version\": 1", "\"version\": 2", 1);
        std::fs::write(path(&dir), &other).unwrap();
        assert!(load_or_create(&dir, None).is_err());
        // The refused file is left as it was.
        assert_eq!(std::fs::read_to_string(path(&dir)).unwrap(), other);
        // A secret that is not the id's.
        let mut q = p.clone();
        q.id = PlayerId([1; 16]).hex();
        assert!(Profile::parse(&serde_json::to_string(&q).unwrap()).is_err());
        delete(&dir).unwrap();
        assert!(!path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_reset_and_restore() {
        let mut p = Profile::new(secret(2), Some("Sora"));
        assert!(p.rename("No!").is_err());
        p.rename("Kairi").unwrap();
        assert!(p.name_confirmed);
        let code = p.recovery_code().unwrap();
        let old = p.player_id();
        p.friends.push(Friend {
            id: "00".into(),
            name: "Riku".into(),
        });
        p.reset_identity(secret(3));
        assert_ne!(p.player_id(), old);
        assert!(p.friends.is_empty());
        assert_eq!(p.name, "Kairi");
        p.restore(&code).unwrap();
        assert_eq!(p.player_id(), old);
        assert!(p.restore("nonsense").is_err());
    }

    #[test]
    fn system_randomness_gives_distinct_secrets() {
        let (a, b) = (new_secret().unwrap(), new_secret().unwrap());
        assert_ne!(a, b);
    }
}
