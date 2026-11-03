//! The duel's network log (`--net-log <path>`, KHC-031): one JSON line a
//! second, from the link's numbers and the duel's own counters. The same line
//! is kept for a crash report, with or without the flag.

use super::duel_net::DuelNet;
use super::*;
use net::{Role, Stats};
use std::fs::File;
use std::io::{LineWriter, Write};
use std::path::Path;

/// How often a line goes out: by the clock, because a line measures the link
/// and not the simulation.
const EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// Bytes of messages sent and received, by what they are; `flights` is the part of
/// the snapshots that is flying things (KHC-017).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ByKind {
    pub input: u64,
    pub snapshot: u64,
    pub events: u64,
    pub flights: u64,
    pub other: u64,
}

impl ByKind {
    fn of(tally: &[u64; 32], flights: u64) -> ByKind {
        let total: u64 = tally.iter().sum();
        let (input, snapshot, events) = (
            tally[usize::from(duel_wire::INPUT)],
            tally[usize::from(duel_wire::SNAPSHOT)] + tally[usize::from(duel_wire::DELTA)],
            tally[usize::from(duel_wire::EVENTS)],
        );
        ByKind {
            input,
            snapshot,
            events,
            flights,
            other: total - input - snapshot - events,
        }
    }

    fn since(&self, then: &ByKind) -> ByKind {
        ByKind {
            input: self.input - then.input,
            snapshot: self.snapshot - then.snapshot,
            events: self.events - then.events,
            flights: self.flights - then.flights,
            other: self.other - then.other,
        }
    }
}

/// The duel's side of a line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub tick: u64,
    pub host: bool,
    /// Snapshots skipped to catch up, and inputs dropped as too old.
    pub skipped: u64,
    /// Joiner: snapshots waiting to be shown.
    pub snapshots: usize,
    /// Host: joiner's buttons waiting to be used.
    pub input_backlog: usize,
    /// Joiner: the snapshots' hold in force in ticks (KHC-026), the speed they
    /// play at in thousandths, the jumps ahead so far, and the longest wait since
    /// the last line with nothing to show, in ticks.
    pub hold_ticks: u32,
    pub speed: u32,
    pub jumps: u64,
    pub froze: u32,
    /// Bytes since the last line, by kind.
    pub by_kind: ByKind,
}

/// One line.
pub fn line(stats: &Stats, c: &Counters) -> String {
    serde_json::json!({
        "tick": c.tick,
        "role": if c.host { "host" } else { "join" },
        "rtt_ms": stats.rtt_ms,
        "packet_loss": stats.packet_loss,
        "sent_bytes_per_sec": stats.sent_bytes_per_sec,
        "received_bytes_per_sec": stats.received_bytes_per_sec,
        "silent_ms": stats.silent_ms,
        "simulated_drops": stats.simulated_drops,
        "skipped": c.skipped,
        "snapshots": c.snapshots,
        "input_backlog": c.input_backlog,
        "hold_ticks": c.hold_ticks,
        "speed": c.speed,
        "jumps": c.jumps,
        "froze": c.froze,
        "by_kind": c.by_kind,
    })
    .to_string()
}

/// `--net-log`'s file, written a line at a time so a crash loses none.
pub fn open(path: &Path) -> Result<LineWriter<File>, String> {
    File::create(path)
        .map(LineWriter::new)
        .map_err(|e| format!("network log {}: {e}", path.display()))
}

/// The rules and seats as a crash report shows them.
fn rules_text(game: &Game) -> String {
    let rules = game.ext.online().duel_rules;
    format!(
        "{rules:?}; ruleset {}; host on {}, joiner on {}",
        rules.ruleset_name(),
        if rules.seat_kh3(0) { "KH3" } else { "KH2" },
        if rules.seat_kh3(1) { "KH3" } else { "KH2" },
    )
}

/// End of a fixed tick: this second's line, when it is due.
pub fn tick(net: &mut DuelNet, game: &Game) {
    if net.log_at.elapsed() < EVERY {
        return;
    }
    net.log_at = std::time::Instant::now();
    crate::crash::note_rules(rules_text(game));
    if !net.session.is_connected() {
        return;
    }
    let total = ByKind::of(net.session.tally(), net.flight_bytes);
    let by_kind = total.since(&net.tallied);
    net.tallied = total;
    let text = line(
        &net.session.stats(),
        &Counters {
            tick: game.tick,
            host: net.role == Role::Host,
            skipped: net.skipped,
            snapshots: net.snapshots.len(),
            input_backlog: net.inputs.len(),
            hold_ticks: net.playout.hold_ticks() as u32,
            speed: (net.playout.speed() * 1000.) as u32,
            jumps: net.jumps,
            froze: std::mem::take(&mut net.froze_longest),
            by_kind,
        },
    );
    if let Some(file) = net.net_log.as_mut() {
        // A full disk costs the log, not the duel.
        let _ = writeln!(file, "{text}");
    }
    crate::crash::note_net_line(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_holds_the_numbers_of_a_fixed_second() {
        let stats = Stats {
            rtt_ms: 12.5,
            packet_loss: 0.25,
            sent_bytes_per_sec: 1024.,
            received_bytes_per_sec: 2048.,
            simulated_drops: 3,
            silent_ms: 16.5,
        };
        let c = Counters {
            tick: 600,
            host: false,
            skipped: 7,
            snapshots: 2,
            input_backlog: 1,
            hold_ticks: 3,
            speed: 1050,
            jumps: 1,
            froze: 4,
            by_kind: ByKind {
                input: 1,
                snapshot: 2,
                events: 3,
                flights: 4,
                other: 5,
            },
        };
        assert_eq!(
            line(&stats, &c),
            "{\"by_kind\":{\"events\":3,\"flights\":4,\"input\":1,\"other\":5,\"snapshot\":2},\"froze\":4,\"hold_ticks\":3,\"input_backlog\":1,\"jumps\":1,\
             \"packet_loss\":0.25,\"received_bytes_per_sec\":2048.0,\"role\":\"join\",\
             \"rtt_ms\":12.5,\"sent_bytes_per_sec\":1024.0,\"silent_ms\":16.5,\
             \"simulated_drops\":3,\"skipped\":7,\"snapshots\":2,\"speed\":1050,\"tick\":600}"
        );
    }

    #[test]
    fn a_line_is_valid_json_with_its_bytes_by_kind() {
        let v: serde_json::Value =
            serde_json::from_str(&line(&Stats::default(), &Counters::default())).unwrap();
        assert_eq!(v["by_kind"]["snapshot"], 0);
        assert_eq!(v["role"], "join");
    }

    #[test]
    fn bytes_are_told_apart_by_kind() {
        let mut tally = [0; 32];
        tally[usize::from(duel_wire::INPUT)] = 10;
        tally[usize::from(duel_wire::SNAPSHOT)] = 20;
        tally[usize::from(duel_wire::DELTA)] = 5;
        tally[usize::from(duel_wire::EVENTS)] = 7;
        tally[usize::from(duel_wire::LOBBY)] = 1;
        let total = ByKind::of(&tally, 3);
        assert_eq!(
            total,
            ByKind {
                input: 10,
                snapshot: 25,
                events: 7,
                flights: 3,
                other: 1
            }
        );
        assert_eq!(total.since(&total), ByKind::default());
    }
}
