//! When the game stops on an error: a report in the `crash` folder beside the
//! settings, and a word to the other player (KHC-028). The report comes first:
//! a panic can happen anywhere, and the message may never get out.
//!
//! The room notes what a report needs as it runs; none of it is read until a
//! panic. A helper thread's panic is reported too, but the game goes on (its
//! caller gets the join's error), so the other player is told only of a named
//! thread's: the main thread's or a task pool's.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// `duel_*` events kept: this many ticks back from the newest (ten seconds),
/// and no more than this many lines.
const EVENT_TICKS: u64 = 600;
const EVENT_LINES: usize = 2000;
/// `--net-log` lines kept: one a second.
const NET_LINES: usize = 10;

#[derive(Default)]
struct Context {
    build: Option<[u8; 32]>,
    rules: String,
    names: Vec<String>,
    events: VecDeque<(u64, String)>,
    newest: u64,
    net_lines: VecDeque<String>,
    notice: Option<net::session::StopNotice>,
}

static CONTEXT: Mutex<Option<Context>> = Mutex::new(None);
/// The first fatal panic has been reported (unwinding can panic again).
static REPORTED: AtomicBool = AtomicBool::new(false);

fn with_context(f: impl FnOnce(&mut Context)) {
    let mut guard = CONTEXT.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Context::default));
}

/// A `duel_*` event from the room's log.
pub fn note_event(value: &serde_json::Value) {
    let Some(name) = value.get("event").and_then(|e| e.as_str()) else {
        return;
    };
    if !name.starts_with("duel_") {
        return;
    }
    let tick = value.get("tick").and_then(|t| t.as_u64());
    let line = value.to_string();
    with_context(|c| {
        c.newest = tick.unwrap_or(c.newest).max(c.newest);
        c.events.push_back((tick.unwrap_or(c.newest), line));
        let oldest = c.newest.saturating_sub(EVENT_TICKS);
        while c.events.front().is_some_and(|(t, _)| *t < oldest) || c.events.len() > EVENT_LINES {
            c.events.pop_front();
        }
    });
}

/// This second's network line (duel_log.rs).
pub fn note_net_line(line: String) {
    with_context(|c| {
        c.net_lines.push_back(line);
        while c.net_lines.len() > NET_LINES {
            c.net_lines.pop_front();
        }
    });
}

/// The rules and both seats' rulesets, as a report shows them.
pub fn note_rules(rules: String) {
    with_context(|c| c.rules = rules);
}

/// A session with another player began: the build, the names to keep out of a
/// report, and how to tell that player.
pub fn note_session(build: [u8; 32], names: Vec<String>, notice: Option<net::session::StopNotice>) {
    with_context(|c| {
        c.build = Some(build);
        c.names = names;
        c.notice = notice;
    });
}

/// The other player is gone: nobody to tell.
pub fn session_over() {
    with_context(|c| c.notice = None);
}

/// A report's text before scrubbing.
#[derive(Default)]
pub struct Report<'a> {
    pub message: &'a str,
    pub location: &'a str,
    pub thread: &'a str,
    pub unix_ms: u128,
    pub version: &'a str,
    pub build: &'a str,
    pub rules: &'a str,
    pub events: Vec<&'a str>,
    pub net_lines: Vec<&'a str>,
}

pub fn format_report(r: &Report) -> String {
    let mut out = format!(
        "Kingdom Hearts: Connected stopped on an error.\n\
         message: {}\n\
         location: {}\n\
         thread: {}\n\
         unix_ms: {}\n\
         version: {}\n\
         build: {}\n\
         rules: {}\n",
        r.message, r.location, r.thread, r.unix_ms, r.version, r.build, r.rules
    );
    out.push_str(&format!(
        "\nThe last ten seconds of duel events ({}):\n",
        r.events.len()
    ));
    for e in &r.events {
        out.push_str(e);
        out.push('\n');
    }
    out.push_str(&format!(
        "\nThe last network lines ({}):\n",
        r.net_lines.len()
    ));
    for l in &r.net_lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// `text` with every `secret` gone, whatever its case.
fn without(text: &str, secret: &str, mark: &str) -> String {
    let (hay, needle) = (text.to_ascii_lowercase(), secret.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = hay[at..].find(&needle) {
        out.push_str(&text[at..at + found]);
        out.push_str(mark);
        at += found + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

/// Whatever follows a home folder's prefix, up to the next separator.
fn without_home_folders(text: &str) -> String {
    let mut out = text.to_owned();
    for prefix in ["\\users\\", "/users/", "\\\\users\\\\", "/home/"] {
        let mut from = 0;
        loop {
            let hay = out.to_ascii_lowercase();
            let Some(found) = hay[from..].find(prefix) else {
                break;
            };
            let start = from + found + prefix.len();
            let tail = &out[start..];
            let len = tail
                .find(['\\', '/', '"', '\'', '\n', ' ', ':'])
                .unwrap_or(tail.len());
            out.replace_range(start..start + len, "<user>");
            from = start + "<user>".len();
        }
    }
    out
}

/// Dotted quads, the other player's address among them.
fn without_addresses(text: &str) -> String {
    let bytes = text.as_bytes();
    let number = |b: u8| b.is_ascii_digit() || b == b'.';
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        if number(bytes[at]) && (at == 0 || !number(bytes[at - 1])) {
            let end = at + bytes[at..].iter().take_while(|b| number(**b)).count();
            let run = &text[at..end];
            let parts: Vec<&str> = run.split('.').collect();
            if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
                out.push_str("<address>");
            } else {
                out.push_str(run);
            }
            at = end;
        } else {
            let c = text[at..].chars().next().unwrap_or(' ');
            out.push(c);
            at += c.len_utf8();
        }
    }
    out
}

/// A report with user names and personal paths taken out: the `secrets` (the
/// profile folder, the user and player names), then any home folder's name
/// and any network address.
pub fn scrub(text: &str, secrets: &[String]) -> String {
    let mut secrets: Vec<&String> = secrets.iter().filter(|s| s.len() >= 2).collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let mut out = text.to_owned();
    for s in secrets {
        out = without(&out, s, "<private>");
        // The same path with the other separators.
        let other = s.replace('\\', "/");
        if other != *s {
            out = without(&out, &other, "<private>");
        }
        let doubled = s.replace('\\', "\\\\");
        if doubled != *s {
            out = without(&out, &doubled, "<private>");
        }
    }
    without_addresses(&without_home_folders(&out))
}

fn personal() -> Vec<String> {
    ["USERPROFILE", "HOME", "USERNAME", "USER", "COMPUTERNAME"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_report(text: &str, unix_ms: u128) -> Option<PathBuf> {
    let dir = crate::launcher::data_dir().join("crash");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("crash-{unix_ms}.txt"));
    std::fs::write(&path, text).ok()?;
    Some(path)
}

fn on_panic(info: &std::panic::PanicHookInfo) {
    let thread = std::thread::current();
    let helper = thread.name().is_none();
    if !helper && REPORTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    // A panic while the notes were being written leaves them unreadable.
    let mut guard = CONTEXT.try_lock().ok();
    let context = guard.as_mut().and_then(|g| g.as_mut());
    let (build, rules, names, events, net_lines, notice) = match context {
        Some(c) => (
            c.build,
            c.rules.clone(),
            c.names.clone(),
            c.events.iter().map(|(_, l)| l.clone()).collect(),
            c.net_lines.iter().cloned().collect(),
            c.notice.take(),
        ),
        None => (
            None,
            String::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
        ),
    };
    drop(guard);
    let events: Vec<String> = events;
    let net_lines: Vec<String> = net_lines;
    // With a session the hash was taken when it began; without one there is
    // nobody waiting while it is taken now.
    let build = build
        .or_else(|| crate::room::build_identity().ok())
        .map_or("unknown".to_owned(), |b| hex(&b));
    let message = info.payload_as_str().unwrap_or("(not text)");
    let location = info
        .location()
        .map_or("unknown".to_owned(), ToString::to_string);
    let thread_name = thread.name().unwrap_or("helper");
    let report = format_report(&Report {
        message,
        location: &location,
        thread: thread_name,
        unix_ms,
        version: crate::room::duel_net::VERSION,
        build: &build,
        rules: &rules,
        events: events.iter().map(String::as_str).collect(),
        net_lines: net_lines.iter().map(String::as_str).collect(),
    });
    let secrets: Vec<String> = personal().into_iter().chain(names).collect();
    write_report(&scrub(&report, &secrets), unix_ms);
    if !helper && let Some(notice) = notice {
        notice.send();
    }
}

/// Puts the report in front of the panic's usual message.
pub fn install() {
    let usual = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        on_panic(info);
        usual(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_has_what_the_owner_needs() {
        let text = format_report(&Report {
            message: "index out of bounds",
            location: "source/khc/src/room/duel_net.rs:12:5",
            thread: "main",
            unix_ms: 1234,
            version: "0.1.0",
            build: "abcd",
            rules: "One KO, host KH3, joiner KH2",
            events: vec![r#"{"event":"duel_hit","tick":7}"#],
            net_lines: vec![r#"{"rtt_ms":3}"#, r#"{"rtt_ms":4}"#],
        });
        assert_eq!(
            text,
            "Kingdom Hearts: Connected stopped on an error.\n\
             message: index out of bounds\n\
             location: source/khc/src/room/duel_net.rs:12:5\n\
             thread: main\n\
             unix_ms: 1234\n\
             version: 0.1.0\n\
             build: abcd\n\
             rules: One KO, host KH3, joiner KH2\n\
             \n\
             The last ten seconds of duel events (1):\n\
             {\"event\":\"duel_hit\",\"tick\":7}\n\
             \n\
             The last network lines (2):\n\
             {\"rtt_ms\":3}\n\
             {\"rtt_ms\":4}\n"
        );
    }

    #[test]
    fn personal_paths_and_names_are_scrubbed() {
        let secrets = vec![
            r"C:\Users\Riku Replica".to_owned(),
            "Riku Replica".to_owned(),
            "Kairi".to_owned(),
            "x".to_owned(),
        ];
        let text = scrub(
            "panicked at C:\\Users\\Riku Replica\\.cargo\\registry\\a.rs:1\n\
             c:/users/riku replica/saves and C:\\\\Users\\\\Riku Replica\\\\k\n\
             {\"name\":\"kairi\",\"host\":\"192.168.1.20:47620\"}\n\
             /home/sora/.cargo and D:\\Users\\Other\\x\n\
             version 0.1.0 at 1.5 and text with an x",
            &secrets,
        );
        assert_eq!(
            text,
            "panicked at <private>\\.cargo\\registry\\a.rs:1\n\
             <private>/saves and <private>\\\\k\n\
             {\"name\":\"<private>\",\"host\":\"<address>:47620\"}\n\
             /home/<user>/.cargo and D:\\Users\\<user>\\x\n\
             version 0.1.0 at 1.5 and text with an x"
        );
    }

    #[test]
    fn events_are_kept_for_ten_seconds() {
        with_context(|c| *c = Context::default());
        for tick in [1u64, 100, 900] {
            note_event(&serde_json::json!({"event":"duel_hit","tick":tick}));
        }
        note_event(&serde_json::json!({"event":"kh3_cue","tick":901}));
        for n in 0..12 {
            note_net_line(n.to_string());
        }
        with_context(|c| {
            let ticks: Vec<u64> = c.events.iter().map(|(t, _)| *t).collect();
            assert_eq!(ticks, [900]);
            assert_eq!(c.net_lines.len(), NET_LINES);
            assert_eq!(c.net_lines.front().map(String::as_str), Some("2"));
        });
    }
}
