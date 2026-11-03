//! Compares two scripted runs: their JSON-line event logs, and their
//! `--state-hash` files. Two runs of one build can differ in when they stop,
//! in what threaded loading did and in what only shows; those are counted.
//! A hit, a score or a knock-out that differs is a real difference.
//!
//! Ticks are read from the run's script start (`duel_script_start`), so the
//! time a run spent loading cannot shift them. An event may sit `window` ticks
//! from the other run's; its fields must match exactly, apart from the ticks
//! and places listed in `FREE_FIELDS`.

use serde_json::Value;
use std::collections::BTreeMap;

/// What decides a match: compared one by one, in order.
pub const GAMEPLAY: [&str; 16] = [
    "duel_cast",
    "duel_lock_miss",
    "duel_forfeit",
    "duel_form_shown",
    "duel_guard",
    "duel_hit",
    "duel_hit_refused",
    "duel_knock_out",
    "duel_match",
    "duel_rematch",
    "duel_round",
    "duel_rulesets",
    "duel_score",
    "duel_stood_up",
    "kh3_grand",
    "kh3_hit",
];

/// Reading files, which threads finish when they finish.
const LOADING: [&str; 13] = [
    "duel_art",
    "duel_arena",
    "duel_bank",
    "duel_kept_build",
    "duel_loadout",
    "duel_music_failed",
    "duel_music_loaded",
    "duel_music_pool",
    "duel_session",
    "kh3_assets",
    "kh3_hud",
    "kh3_load_failed",
    "kh3_poles",
];

/// How a run ended.
const EXIT: [&str; 3] = ["duel_loadout_restored", "exit", "net_disconnected"];

/// Fields a gameplay event may differ in: the ticks, compared separately, and
/// where a hit landed, which `--state-hash` compares exactly.
const FREE_FIELDS: [&str; 11] = [
    "behind",
    "rewind",
    "host_tick",
    "instance",
    "tick",
    "at",
    "contact",
    "direction",
    "position",
    "started",
    "since",
];

/// The kinds of difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Exit,
    Loading,
    Cosmetic,
    Gameplay,
}

pub fn kind_of(event: &str) -> Kind {
    if GAMEPLAY.contains(&event) {
        Kind::Gameplay
    } else if LOADING.contains(&event) {
        Kind::Loading
    } else if EXIT.contains(&event) {
        Kind::Exit
    } else {
        Kind::Cosmetic
    }
}

/// One event of a log, its tick counted from the script's start.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    pub tick: i64,
    pub fields: BTreeMap<String, Value>,
}

/// A run's log.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub events: Vec<Event>,
    /// The last tick seen, from the script's start.
    pub end: i64,
    /// Lines that were not JSON (a run killed mid-write).
    pub bad_lines: usize,
}

/// Reads a log: every line a JSON object, `event` naming it. Events without a
/// tick take the tick before them.
pub fn parse_run(text: &str) -> Run {
    let values: Vec<Option<Value>> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).ok())
        .collect();
    let tick_of = |v: &Value| v.get("tick").and_then(Value::as_i64);
    let anchor = values
        .iter()
        .flatten()
        .find(|v| v.get("event").and_then(Value::as_str) == Some("duel_script_start"))
        .and_then(tick_of)
        .unwrap_or(0);
    let mut run = Run::default();
    let mut last = 0;
    for value in &values {
        let Some(value) = value else {
            run.bad_lines += 1;
            continue;
        };
        let Some(name) = value.get("event").and_then(Value::as_str) else {
            continue;
        };
        last = tick_of(value).unwrap_or(last);
        let fields = value
            .as_object()
            .map(|o| {
                o.iter()
                    .filter(|(k, _)| k.as_str() != "event")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();
        run.events.push(Event {
            name: name.to_owned(),
            tick: last - anchor,
            fields,
        });
        run.end = run.end.max(last - anchor);
    }
    run
}

/// One thing that differs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub kind: Kind,
    pub text: String,
}

/// What a comparison found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub differences: Vec<Difference>,
    /// Gameplay events compared one to one.
    pub matched: usize,
    /// The largest tick offset between matched gameplay events.
    pub largest_offset: i64,
    /// Matched events by offset.
    pub offsets: BTreeMap<i64, usize>,
}

impl Report {
    /// No real difference.
    pub fn passes(&self) -> bool {
        !self.differences.iter().any(|d| d.kind == Kind::Gameplay)
    }

    pub fn count(&self, kind: Kind) -> usize {
        self.differences.iter().filter(|d| d.kind == kind).count()
    }

    /// The readable diff.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{}: {} gameplay events matched, largest tick offset {}, differences: \
             {} gameplay, {} cosmetic, {} loading, {} exit\n",
            if self.passes() { "SAME" } else { "DIFFERENT" },
            self.matched,
            self.largest_offset,
            self.count(Kind::Gameplay),
            self.count(Kind::Cosmetic),
            self.count(Kind::Loading),
            self.count(Kind::Exit),
        );
        let histogram: Vec<String> = self
            .offsets
            .iter()
            .map(|(offset, n)| format!("{offset:+}: {n}"))
            .collect();
        if !histogram.is_empty() {
            out.push_str(&format!("offsets  {}\n", histogram.join(", ")));
        }
        // The first real differences, then one line for the others' kinds.
        let mut shown = 0;
        for d in &self.differences {
            if d.kind == Kind::Gameplay && shown < 20 {
                out.push_str(&format!("gameplay  {}\n", d.text));
                shown += 1;
            }
        }
        if self.count(Kind::Gameplay) > shown {
            out.push_str(&format!(
                "gameplay  ...and {} more\n",
                self.count(Kind::Gameplay) - shown
            ));
        }
        for d in &self.differences {
            if d.kind != Kind::Gameplay {
                out.push_str(&format!("{:?}  {}\n", d.kind, d.text).to_lowercase());
            }
        }
        out
    }
}

fn describe(e: &Event) -> String {
    let fields: Vec<String> = e
        .fields
        .iter()
        .filter(|(k, _)| !FREE_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    format!("{} at {} ({})", e.name, e.tick, fields.join(" "))
}

/// The fields that must match and do not.
fn field_differences(a: &Event, b: &Event) -> Vec<String> {
    let keys: std::collections::BTreeSet<&String> =
        a.fields.keys().chain(b.fields.keys()).collect();
    keys.into_iter()
        .filter(|k| !FREE_FIELDS.contains(&k.as_str()))
        .filter(|k| a.fields.get(*k) != b.fields.get(*k))
        .map(|k| {
            let show = |e: &Event| e.fields.get(k).map_or("-".to_owned(), Value::to_string);
            format!("{k}: {} / {}", show(a), show(b))
        })
        .collect()
}

/// Compares `a` and `b`: gameplay events by kind and order within `window`
/// ticks, the rest by how many of each there are.
pub fn compare(a: &Run, b: &Run, window: i64) -> Report {
    let mut report = Report::default();
    let gameplay = |r: &Run| -> BTreeMap<String, Vec<Event>> {
        let mut by_name: BTreeMap<String, Vec<Event>> = BTreeMap::new();
        for e in r
            .events
            .iter()
            .filter(|e| kind_of(&e.name) == Kind::Gameplay)
        {
            by_name.entry(e.name.clone()).or_default().push(e.clone());
        }
        by_name
    };
    let (left, right) = (gameplay(a), gameplay(b));
    let names: std::collections::BTreeSet<&String> = left.keys().chain(right.keys()).collect();
    let empty = Vec::new();
    for name in names {
        let (la, lb) = (
            left.get(name).unwrap_or(&empty),
            right.get(name).unwrap_or(&empty),
        );
        for (x, y) in la.iter().zip(lb) {
            // Before the script's start only the lobby's length is measured.
            let offset = if x.tick < 0 && y.tick < 0 {
                0
            } else {
                y.tick - x.tick
            };
            *report.offsets.entry(offset).or_default() += 1;
            report.largest_offset = report.largest_offset.max(offset.abs());
            report.matched += 1;
            if offset.abs() > window {
                report.differences.push(Difference {
                    kind: Kind::Gameplay,
                    text: format!(
                        "{name} {} ticks apart (window {window}): {} / {}",
                        offset.abs(),
                        x.tick,
                        y.tick
                    ),
                });
            }
            let fields = field_differences(x, y);
            if !fields.is_empty() {
                report.differences.push(Difference {
                    kind: Kind::Gameplay,
                    text: format!("{name} at {}: {}", x.tick, fields.join(", ")),
                });
            }
        }
        // One run went on longer: past the shorter's end that is how it
        // stopped, before it a real difference.
        let shared = la.len().min(lb.len());
        for (e, other_end, which) in la[shared..]
            .iter()
            .map(|e| (e, b.end, "first"))
            .chain(lb[shared..].iter().map(|e| (e, a.end, "second")))
        {
            let kind = if e.tick + window >= other_end {
                Kind::Exit
            } else {
                Kind::Gameplay
            };
            report.differences.push(Difference {
                kind,
                text: format!("only in the {which} run: {}", describe(e)),
            });
        }
    }
    // The rest by count.
    let counts = |r: &Run| -> BTreeMap<String, usize> {
        let mut c = BTreeMap::new();
        for e in r
            .events
            .iter()
            .filter(|e| kind_of(&e.name) != Kind::Gameplay)
        {
            *c.entry(e.name.clone()).or_default() += 1;
        }
        c
    };
    let (ca, cb) = (counts(a), counts(b));
    let names: std::collections::BTreeSet<&String> = ca.keys().chain(cb.keys()).collect();
    for name in names {
        let (x, y) = (
            ca.get(name).copied().unwrap_or(0),
            cb.get(name).copied().unwrap_or(0),
        );
        if x != y {
            report.differences.push(Difference {
                kind: kind_of(name),
                text: format!("{name}: {x} / {y}"),
            });
        }
    }
    if a.end != b.end {
        report.differences.push(Difference {
            kind: Kind::Exit,
            text: format!("last tick {} / {}", a.end, b.end),
        });
    }
    report
}

/// A `--state-hash` file: `tick,hash,group...` under a header line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HashRun {
    pub groups: Vec<String>,
    pub rows: BTreeMap<u64, Vec<String>>,
}

pub fn parse_hashes(text: &str) -> Result<HashRun, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or("no header line")?;
    let columns: Vec<&str> = header.split(',').collect();
    if columns.len() < 2 || columns[0] != "tick" || columns[1] != "hash" {
        return Err(format!("not a state-hash header: {header}"));
    }
    let mut run = HashRun {
        groups: columns[2..].iter().map(|s| (*s).to_owned()).collect(),
        rows: BTreeMap::new(),
    };
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        // A run killed mid-write leaves a short last line.
        if cells.len() != columns.len() {
            continue;
        }
        let tick = cells[0]
            .parse()
            .map_err(|e| format!("tick {}: {e}", cells[0]))?;
        run.rows
            .insert(tick, cells[1..].iter().map(|s| (*s).to_owned()).collect());
    }
    Ok(run)
}

/// Where two hash files first part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashReport {
    /// Ticks both have.
    pub compared: usize,
    /// The first tick that differs, and the groups that differ on it.
    pub first: Option<(u64, Vec<String>)>,
    /// How many of the shared ticks differ, and in each group.
    pub differing: usize,
    pub by_group: BTreeMap<String, usize>,
    /// The group that differs first in each group's own history.
    pub first_by_group: BTreeMap<String, u64>,
}

impl HashReport {
    pub fn text(&self) -> String {
        match &self.first {
            None => format!("IDENTICAL over {} ticks\n", self.compared),
            Some((tick, groups)) => {
                let mut out = format!(
                    "DIFFERENT from tick {tick} ({}): {} of {} ticks differ\n",
                    groups.join(", "),
                    self.differing,
                    self.compared
                );
                for (group, n) in &self.by_group {
                    out.push_str(&format!(
                        "  {group}: {n} ticks, first {}\n",
                        self.first_by_group.get(group).copied().unwrap_or(0)
                    ));
                }
                out
            }
        }
    }
}

pub fn compare_hashes(a: &HashRun, b: &HashRun) -> HashReport {
    let mut report = HashReport {
        compared: 0,
        first: None,
        differing: 0,
        by_group: BTreeMap::new(),
        first_by_group: BTreeMap::new(),
    };
    for (tick, x) in &a.rows {
        let Some(y) = b.rows.get(tick) else { continue };
        report.compared += 1;
        if x.first() == y.first() {
            continue;
        }
        report.differing += 1;
        let groups: Vec<String> = a
            .groups
            .iter()
            .enumerate()
            .filter(|(i, _)| x.get(i + 1) != y.get(i + 1))
            .map(|(_, g)| g.clone())
            .collect();
        for g in &groups {
            *report.by_group.entry(g.clone()).or_default() += 1;
            report.first_by_group.entry(g.clone()).or_insert(*tick);
        }
        if report.first.is_none() {
            report.first = Some((*tick, groups));
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic log: the script starts `shift` ticks in, then a hit, a
    /// knock-out and the score, each `offset` ticks late.
    fn log(shift: i64, offset: i64, hp: i32, last: i64) -> String {
        let t = |tick: i64| shift + tick + offset;
        [
            format!(
                r#"{{"event":"duel_art","error":null,"tick":{}}}"#,
                shift / 2
            ),
            format!(r#"{{"event":"duel_script_start","tick":{shift}}}"#),
            format!(
                r#"{{"event":"duel_hit","tick":{},"attacker":0,"hp":{hp},"at":[{offset},0,0]}}"#,
                t(100)
            ),
            format!(r#"{{"event":"duel_knock_out","tick":{},"slot":1}}"#, t(200)),
            format!(
                r#"{{"event":"duel_score","tick":{},"wins":[1,0],"dealt":[{},0]}}"#,
                t(210),
                170 - hp
            ),
            format!(r#"{{"event":"exit","tick":{},"code":0}}"#, shift + last),
        ]
        .join("\n")
    }

    #[test]
    fn identical_runs_are_the_same() {
        let a = parse_run(&log(1000, 0, 90, 300));
        let report = compare(&a, &a.clone(), 0);
        assert!(report.passes(), "{}", report.text());
        assert_eq!(report.matched, 3);
        assert_eq!(report.largest_offset, 0);
        assert!(report.differences.is_empty());
        assert!(report.text().starts_with("SAME"));
    }

    #[test]
    fn loading_time_and_small_offsets_are_noise() {
        // Loaded 400 ticks later, gameplay one tick later, stopped elsewhere.
        let a = parse_run(&log(1000, 0, 90, 300));
        let b = parse_run(&log(1400, 1, 90, 305));
        let report = compare(&a, &b, 2);
        assert!(report.passes(), "{}", report.text());
        assert_eq!(report.largest_offset, 1);
        assert_eq!(report.offsets.get(&1), Some(&3));
        assert_eq!(report.count(Kind::Exit), 1);
        // The same offset outside a tighter window is real.
        assert!(!compare(&a, &b, 0).passes());
    }

    #[test]
    fn a_different_hp_or_score_is_real() {
        let a = parse_run(&log(1000, 0, 90, 300));
        let b = parse_run(&log(1000, 0, 91, 300));
        let report = compare(&a, &b, 5);
        assert!(!report.passes());
        let text = report.text();
        assert!(text.starts_with("DIFFERENT"), "{text}");
        assert!(text.contains("duel_hit at 100: hp: 90 / 91"), "{text}");
        assert!(text.contains("duel_score at 210: dealt"), "{text}");
    }

    #[test]
    fn a_missing_event_is_real_unless_the_run_had_stopped() {
        let a = parse_run(&log(0, 0, 90, 300));
        // A knock-out gone from the middle of the other run.
        let gone = log(0, 0, 90, 300).replace("duel_knock_out", "duel_cosmetic");
        let report = compare(&a, &parse_run(&gone), 5);
        assert!(!report.passes());
        assert!(
            report
                .text()
                .contains("only in the first run: duel_knock_out")
        );
        // The other run stopped before the last events: how it stopped.
        let short: String = log(0, 0, 90, 120)
            .lines()
            .filter(|l| !l.contains("duel_knock_out") && !l.contains("duel_score"))
            .collect::<Vec<_>>()
            .join("\n");
        // ...but tick 200 is 80 past its end, outside no window but its own.
        let report = compare(&a, &parse_run(&short), 100);
        assert!(report.passes(), "{}", report.text());
        assert_eq!(report.count(Kind::Gameplay), 0);
    }

    #[test]
    fn places_may_differ_and_broken_lines_are_counted() {
        let a = parse_run(&log(0, 0, 90, 300));
        let b = parse_run(&(log(0, 0, 90, 300).replace("[0,0,0]", "[9,9,9]") + "\n{cut"));
        assert_eq!(b.bad_lines, 1);
        assert!(compare(&a, &b, 0).passes());
    }

    #[test]
    fn events_are_sorted_into_their_kinds() {
        assert_eq!(kind_of("duel_hit"), Kind::Gameplay);
        assert_eq!(kind_of("duel_bank"), Kind::Loading);
        assert_eq!(kind_of("exit"), Kind::Exit);
        assert_eq!(kind_of("duel_notice"), Kind::Cosmetic);
    }

    const HASHES: &str = "tick,hash,motion0,vitals0,match\n\
                          0,aa,1,2,3\n\
                          1,ab,1,2,3\n\
                          2,ac,4,2,3\n\
                          3,ad,4,5,3\n";

    #[test]
    fn identical_hash_files_say_so() {
        let a = parse_hashes(HASHES).unwrap();
        let report = compare_hashes(&a, &a);
        assert_eq!(report.compared, 4);
        assert_eq!(report.first, None);
        assert!(report.text().starts_with("IDENTICAL over 4"));
    }

    #[test]
    fn hash_files_report_the_first_tick_and_group() {
        let a = parse_hashes(HASHES).unwrap();
        let b = parse_hashes(
            "tick,hash,motion0,vitals0,match\n\
             0,aa,1,2,3\n\
             1,xx,9,2,3\n\
             2,ac,4,2,3\n\
             3,yy,8,7,3\n\
             4,zz,1,1,1\n",
        )
        .unwrap();
        let report = compare_hashes(&a, &b);
        assert_eq!(report.compared, 4);
        assert_eq!(report.first, Some((1, vec!["motion0".to_owned()])));
        assert_eq!(report.differing, 2);
        assert_eq!(report.first_by_group.get("vitals0"), Some(&3));
        assert!(report.text().contains("DIFFERENT from tick 1 (motion0)"));
    }

    #[test]
    fn a_hash_file_needs_its_header() {
        assert!(parse_hashes("").is_err());
        assert!(parse_hashes("a,b\n1,2\n").is_err());
        // A cut last line is left out.
        let run = parse_hashes("tick,hash,g\n1,aa,bb\n2,cc").unwrap();
        assert_eq!(run.rows.len(), 1);
    }
}
