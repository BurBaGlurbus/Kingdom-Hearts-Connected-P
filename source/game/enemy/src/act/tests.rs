//! Queue and start/clear behavior, from the act-manager routines.
use super::*;

fn entry(name: &str, priority: u16, bits: u16, entry: u16) -> Entry {
    Entry {
        name: name.as_bytes().to_vec(),
        priority,
        flag_bits: bits,
        entry,
        ..Entry::default()
    }
}

fn manager() -> Manager {
    let mut m = Manager::default();
    m.init_table();
    m.add(entry("mode_battle", 100, 1, 10));
    m.add(entry("btl_attack", 100, 1, 20));
    m.add(entry("leave", 300, 3, 30));
    m.add(entry("claw", 200, 1, 40));
    m
}

#[test]
fn start_creates_the_thread_and_marks_it_just_started() {
    let mut m = manager();
    let out = m.start(b"mode_battle");
    assert_eq!(out, [Action::Spawn { entry: 10 }]);
    assert_eq!(m.priority, 100);
    assert_eq!(m.flags, JUST_STARTED);
    assert!(m.is_exec());
}

#[test]
fn locked_acts_ignore_start_push_and_clear() {
    let mut m = manager();
    m.start(b"leave");
    assert!(m.locked());
    assert!(m.start(b"claw").is_empty());
    m.push(b"claw");
    assert_eq!(m.queued(), 0);
    assert!(m.clear().is_empty());
    assert_eq!(m.current, m.find(b"leave"));
}

#[test]
fn start_discards_the_running_act_and_the_queue() {
    let mut m = manager();
    m.start(b"mode_battle");
    m.push(b"btl_attack");
    assert_eq!(m.queued(), 1);
    let out = m.start(b"claw");
    assert_eq!(out, [Action::Kill, Action::Spawn { entry: 40 }]);
    assert_eq!(m.queued(), 0);
    assert_eq!(m.priority, 200);
}

#[test]
fn queue_pops_only_when_no_act_runs_and_holds_fifteen() {
    let mut m = manager();
    for _ in 0..20 {
        m.push(b"btl_attack");
    }
    assert_eq!(m.queued(), 15);
    let mut out = Vec::new();
    assert!(m.pop(&mut out));
    assert_eq!(out, [Action::Spawn { entry: 20 }]);
    assert_eq!(m.queued(), 14);
    // An act is running: nothing is popped.
    out.clear();
    assert!(!m.pop(&mut out));
    assert_eq!(m.queued(), 14);
    // The act's thread ends: the priority drops and the next entry may run.
    m.flags = 0;
    m.thread_ended(&mut out);
    assert_eq!(m.priority, 0);
    assert!(!m.is_exec() || m.queued() > 0);
    out.clear();
    assert!(m.pop(&mut out));
    assert_eq!(m.queued(), 13);
}

#[test]
fn a_thread_that_ended_after_a_restart_does_not_clear_the_new_act() {
    let mut m = manager();
    m.start(b"mode_battle");
    m.start(b"claw"); // inside the old thread
    let mut out = Vec::new();
    m.thread_ended(&mut out);
    assert!(out.is_empty());
    assert_eq!(m.priority, 200);
}

#[test]
fn end_callback_runs_before_the_thread_is_discarded() {
    let mut m = manager();
    let mut e = entry("timed", 100, 1, 50);
    e.end = 77;
    m.add(e);
    m.start(b"timed");
    let out = m.clear();
    assert_eq!(
        out,
        [
            Action::Callback {
                word: 77,
                argument: Argument::Object
            },
            Action::Kill
        ]
    );
    assert!(!m.is_exec());
}

#[test]
fn same_name_overwrites_and_unknown_names_are_ignored() {
    let mut m = manager();
    m.add(entry("claw", 250, 1, 41));
    assert_eq!(m.table.len(), 4);
    m.start(b"claw");
    assert_eq!(m.priority, 250);
    assert!(m.start(b"nonexistent").is_empty());
    assert_eq!(m.current, m.find(b"claw"));
}

#[test]
fn priority_gated_restart() {
    let mut m = manager();
    m.start(b"claw");
    assert!(m.start_if_not_outranked(b"mode_battle").is_empty());
    assert_eq!(m.priority, 200);
    assert!(!m.start_if_not_outranked(b"leave").is_empty());
}
