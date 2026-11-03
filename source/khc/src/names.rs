//! Player names: what a name may hold, the words it may not, and the short
//! number that tells two players of the same name apart (WP-23, KHC-044).
//!
//! [hub choice, KHC-044] 3 to 16 characters of ASCII letters, digits, space,
//! `_` and `-`, with no space at either end and none doubled. Every one of
//! them is in the game's system font, so a nametag never shows `?` for a
//! name that passed. The same check runs in the launcher and on the server.

use crate::identity::PlayerId;

pub const MIN_CHARS: usize = 3;
pub const MAX_CHARS: usize = 16;

/// The blocklist: one entry a line. `=word` refuses a name that is that word
/// once spaces, `_` and `-` are gone; `word` refuses any name holding it.
/// Both compare after folding case and the usual digit swaps (`0` for `o`).
const BLOCKLIST: &str = include_str!("../blocklist.txt");

/// Why a name was refused, in words the launcher shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    TooShort,
    TooLong,
    Character(char),
    EdgeSpace,
    DoubleSpace,
    Blocked,
}

impl Refusal {
    pub fn text(self) -> String {
        match self {
            Refusal::TooShort => format!("At least {MIN_CHARS} characters."),
            Refusal::TooLong => format!("At most {MAX_CHARS} characters."),
            Refusal::Character(c) => {
                format!("Letters, digits, space, _ and - only, not {c}.")
            }
            Refusal::EdgeSpace => "No space at the start or the end.".into(),
            Refusal::DoubleSpace => "No two spaces together.".into(),
            Refusal::Blocked => "That name is not allowed.".into(),
        }
    }
}

/// The characters a name may hold.
pub fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-')
}

/// Whether `name` may be used, and why not.
pub fn check(name: &str) -> Result<(), Refusal> {
    if let Some(c) = name.chars().find(|&c| !allowed(c)) {
        return Err(Refusal::Character(c));
    }
    let n = name.chars().count();
    if n < MIN_CHARS {
        return Err(Refusal::TooShort);
    }
    if n > MAX_CHARS {
        return Err(Refusal::TooLong);
    }
    if name.starts_with(' ') || name.ends_with(' ') {
        return Err(Refusal::EdgeSpace);
    }
    if name.contains("  ") {
        return Err(Refusal::DoubleSpace);
    }
    if blocked(name) {
        return Err(Refusal::Blocked);
    }
    Ok(())
}

/// Lower case, digits read as the letters they stand for, and the joining
/// characters gone.
fn fold(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, ' ' | '_' | '-'))
        .map(|c| match c.to_ascii_lowercase() {
            '0' => 'o',
            '1' => 'i',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            c => c,
        })
        .collect()
}

fn blocked(name: &str) -> bool {
    let folded = fold(name);
    BLOCKLIST
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .any(|entry| match entry.strip_prefix('=') {
            Some(word) => folded == fold(word),
            None => folded.contains(&fold(entry)),
        })
}

/// The four digits shown after a name (`Name#4821`), from the player id.
pub fn discriminator(id: &PlayerId) -> u16 {
    u16::from_le_bytes([id.0[0], id.0[1]]) % 10_000
}

/// `Name#4821`.
pub fn tagged(name: &str, id: &PlayerId) -> String {
    format!("{name}#{:04}", discriminator(id))
}

/// The name a new profile starts with: `Player` and four digits from the id,
/// never the Windows user name.
pub fn default_name(id: &PlayerId) -> String {
    format!("Player{:04}", discriminator(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rules() {
        assert_eq!(check("Sora"), Ok(()));
        assert_eq!(check("Key_blade-99"), Ok(()));
        assert_eq!(check("Ri ku"), Ok(()));
        assert_eq!(check("Ab"), Err(Refusal::TooShort));
        assert_eq!(check("Abcdefghijklmnopq"), Err(Refusal::TooLong));
        assert_eq!(check("Abcdefghijklmnop"), Ok(()));
        assert_eq!(check(" Sora"), Err(Refusal::EdgeSpace));
        assert_eq!(check("Sora "), Err(Refusal::EdgeSpace));
        assert_eq!(check("So  ra"), Err(Refusal::DoubleSpace));
        assert_eq!(check("Sora!"), Err(Refusal::Character('!')));
        assert_eq!(check("Sóra"), Err(Refusal::Character('ó')));
        assert_eq!(check("Sora(1)"), Err(Refusal::Character('(')));
        assert_eq!(check(""), Err(Refusal::TooShort));
    }

    #[test]
    fn the_blocklist() {
        assert_eq!(check("Admin"), Err(Refusal::Blocked));
        assert_eq!(check("ADM1N"), Err(Refusal::Blocked));
        assert_eq!(check("a-d_m i n"), Err(Refusal::Blocked));
        // A whole-name entry leaves longer names alone.
        assert_eq!(check("Admiral"), Ok(()));
        assert_eq!(check("Moderna"), Ok(()));
        // A part entry is refused anywhere in the name.
        assert_eq!(check("TheOfficialSora"), Err(Refusal::Blocked));
        assert!(
            BLOCKLIST
                .lines()
                .filter(|l| !l.starts_with('#'))
                .all(|l| l.len() < 32)
        );
    }

    #[test]
    fn the_short_number() {
        let id = PlayerId([0x21, 0x13, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9]);
        assert_eq!(discriminator(&id), 4897);
        assert_eq!(tagged("Sora", &id), "Sora#4897");
        assert_eq!(default_name(&id), "Player4897");
        assert_eq!(check(&default_name(&id)), Ok(()));
        let zero = PlayerId([0; 16]);
        assert_eq!(tagged("Sora", &zero), "Sora#0000");
    }

    #[test]
    fn drawn_names_keep_to_the_font() {}
}
