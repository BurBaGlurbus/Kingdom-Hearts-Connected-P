//! [Test] The duel's state as hashes (`--state-hash <path>`, KHC-019): after
//! each fixed tick, what the host's simulation holds of both Soras and the
//! match, one column per group of fields, so two runs can be compared tick by
//! tick. Written on the host, or alone with a scripted rival; a joiner's state
//! is the host's.

use super::duel_match::Match;
use super::duel_match_room::MatchRoom;
use super::duel_net::fighter_slots;
use super::duel_script;
use super::duel_sync;
use super::duel_wire::FighterState;
use super::*;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::hint::black_box;
use std::io::{LineWriter, Write};
use std::path::Path;

/// The columns after `tick,hash`.
pub const GROUPS: [&str; 9] = [
    "motion0", "vitals0", "items0", "kh3_0", "motion1", "vitals1", "items1", "kh3_1", "match",
];
/// Ticks between two measures of what a copy of the state costs.
const COPY_EVERY: u64 = 600;

#[derive(Resource)]
pub struct HashFile {
    out: LineWriter<File>,
    /// `--state-dump`: the text each column hashes, to read what differs.
    dump: Option<LineWriter<File>>,
}

impl HashFile {
    pub fn create(path: &Path, dump: Option<&Path>) -> Result<HashFile, String> {
        let file = File::create(path).map_err(|e| format!("state hash {}: {e}", path.display()))?;
        let mut out = LineWriter::new(file);
        writeln!(out, "tick,hash,{}", GROUPS.join(","))
            .map_err(|e| format!("state hash {}: {e}", path.display()))?;
        let dump = dump
            .map(|p| {
                File::create(p)
                    .map(LineWriter::new)
                    .map_err(|e| format!("state dump {}: {e}", p.display()))
            })
            .transpose()?;
        Ok(HashFile { out, dump })
    }
}

/// The workspace's SHA-256, truncated: a tick's few hashes stay short.
fn digest(text: &str) -> u64 {
    let hash = Sha256::digest(text.as_bytes());
    let mut first = [0; 8];
    first.copy_from_slice(&hash[..8]);
    u64::from_le_bytes(first)
}

/// A Sora's groups. Debug prints a float's shortest exact digits, so equal
/// text is equal bits. His action's start is an absolute tick, so left out.
fn fighter_groups(f: Option<&FighterState>) -> [String; 4] {
    let Some(f) = f else {
        return Default::default();
    };
    [
        format!(
            "{:?}",
            (f.position, f.yaw, f.action, f.frame, f.armed, f.blend)
        ),
        format!("{:?}", (f.hp, f.max_hp, f.mp, f.drive, f.invulnerable)),
        format!("{:?}", (f.form, &f.items, &f.bullets)),
        format!("{:?}", f.kh3),
    ]
}

/// The match's fields that do not name an absolute tick.
fn match_group(m: Option<&Match>) -> String {
    format!(
        "{:?}",
        m.map(|m| (m.phase, m.round, m.wins, m.winner, m.why, m.dealt, m.hits))
    )
}

/// The text of a tick's columns, from what the host would send.
pub fn texts(fighters: &[FighterState], m: Option<&Match>) -> [String; GROUPS.len()] {
    let seat = |slot| fighters.iter().find(|f| f.slot == slot);
    let [a0, a1, a2, a3] = fighter_groups(seat(fighter_slots::HOST));
    let [b0, b1, b2, b3] = fighter_groups(seat(fighter_slots::JOIN));
    [a0, a1, a2, a3, b0, b1, b2, b3, match_group(m)]
}

/// A tick's columns.
pub fn groups(fighters: &[FighterState], m: Option<&Match>) -> [u64; GROUPS.len()] {
    texts(fighters, m).map(|t| if t.is_empty() { 0 } else { digest(&t) })
}

/// One line of the file.
pub fn row(tick: u64, columns: &[u64]) -> String {
    let cells: Vec<String> = columns.iter().map(|c| format!("{c:016x}")).collect();
    format!(
        "{tick},{:016x},{}",
        digest(&cells.join(",")),
        cells.join(",")
    )
}

/// What copying the duel's state costs now: the parts of `Game` that hold
/// Sora-against-Sora rules and can be cloned as they are. `DriveRt`, `ItemRt`
/// and the spells' runtime hold loaded files and cannot, so the record lists
/// them as work a rollback would have to do first.
fn measure_copy(game: &mut Game) {
    let started = std::time::Instant::now();
    let mut bytes = 0;
    {
        let mut copy = |f: &Fighter| {
            let player = black_box(f.player.clone());
            let vitals = black_box(f.vitals.clone());
            let kh3 = black_box(f.ext.fighter().kh3.clone());
            bytes += size_of_val(&player) + size_of_val(&vitals) + size_of_val(&kh3);
        };
        copy(&game.sora);
        for r in &game.rivals {
            copy(&r.fighter);
        }
    }
    let ns = started.elapsed().as_nanos() as u64;
    let tick = game.tick;
    game.event(serde_json::json!({"event":"duel_copy_cost","tick":tick,"ns":ns,"bytes":bytes}));
}

/// After a fixed tick.
pub fn tick(file: Option<ResMut<HashFile>>, room: Option<Res<MatchRoom>>, mut game: ResMut<Game>) {
    let Some(mut file) = file else { return };
    if game.replica {
        return;
    }
    let Some(tick) = duel_script::now(&game, game.tick) else {
        return;
    };
    let fighters = duel_sync::fighters_of(&game);
    let m = room.as_deref().and_then(|r| r.m);
    let columns = groups(&fighters, m.as_ref());
    // A full disk costs the file, not the run.
    let _ = writeln!(file.out, "{}", row(tick, &columns));
    if let Some(dump) = file.dump.as_mut() {
        for (group, text) in GROUPS.iter().zip(texts(&fighters, m.as_ref())) {
            let _ = writeln!(dump, "{tick} {group} {text}");
        }
    }
    if tick.is_multiple_of(COPY_EVERY) {
        measure_copy(&mut game);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(slot: u8, hp: i32) -> FighterState {
        FighterState {
            slot,
            position: [1.5, 0., -300.25],
            yaw: 0.5,
            action: Action::Idle,
            frame: 12,
            armed: false,
            blend: 1.,
            started: 77,
            hp,
            max_hp: 170,
            mp: [100, 180],
            drive: [9, 0, 9, 9],
            invulnerable: 0.,
            form: 0,
            items: vec![1, 2, 3],
            bullets: Vec::new(),
            flights: Vec::new(),
            kh3: None,
            motion: None,
        }
    }

    #[test]
    fn a_fixed_state_hashes_to_a_fixed_value() {
        let columns = groups(&[state(0, 170), state(1, 90)], None);
        assert_eq!(
            row(42, &columns),
            "42,123d56f59a26cecb,a2bc9de405509b2c,0199d438a6a6aae5,b034f81ab9361fc2,f5042689597b93dc,a2bc9de405509b2c,41f277a419ca818c,b034f81ab9361fc2,f5042689597b93dc,f5042689597b93dc"
        );
    }

    #[test]
    fn a_change_shows_in_its_own_group_only() {
        let a = groups(&[state(0, 170), state(1, 90)], None);
        let b = groups(&[state(0, 170), state(1, 89)], None);
        let changed: Vec<&str> = (0..GROUPS.len())
            .filter(|&i| a[i] != b[i])
            .map(|i| GROUPS[i])
            .collect();
        assert_eq!(changed, ["vitals1"]);
        // Absolute ticks are left out: a run that loaded longer hashes the same.
        let mut later = state(1, 90);
        later.started = 5000;
        assert_eq!(a, groups(&[state(0, 170), later], None));
    }
}
