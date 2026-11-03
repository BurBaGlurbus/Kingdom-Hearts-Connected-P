//! The duel's state over the session, once per fixed tick. The host takes
//! the joiner's buttons for its Sora, then sends a snapshot of both Soras
//! and the tick's events; the joiner shows the snapshots a little behind (to
//! ride out jitter, duel_playout.rs), gliding between them, and plays the events'
//! sounds and sparks itself. Nothing cosmetic is replicated.

use super::duel_net::{DuelNet, fighter_slots, warn};
use super::duel_wire::{self, DuelEvent, FighterState, Snapshot};
use super::rival::{Command, RivalPad};
use super::*;
use net::{RELIABLE, Role, UNRELIABLE};

/// Joiner ticks of buttons the host lets wait before it catches up.
const INPUT_BACKLOG: usize = 4;
/// A replay's and a locked run's queue of snapshots, which go one a tick: how many
/// it holds back, and the most it lets pile up. A live session has its playhead
/// (duel_playout.rs).
const SNAPSHOT_DELAY: usize = 2;
const SNAPSHOT_BACKLOG: usize = 8;
/// A Sora who moves further than this between two snapshots was put there (a
/// round's start): he does not glide.
const PUT_THERE: f32 = 200.;

/// The buttons that only last one tick are kept when ticks are merged.
fn merge(older: RivalPad, newer: RivalPad) -> RivalPad {
    RivalPad {
        jump: older.jump || newer.jump,
        attack: older.attack || newer.attack,
        guard: older.guard || newer.guard,
        flow: older.flow || newer.flow,
        // A menu command is never dropped either.
        command: match newer.command {
            Command::None => older.command,
            c => c,
        },
        ..newer
    }
}

/// Without new buttons the stick and the held buttons stay; presses do not
/// repeat.
fn held(pad: RivalPad) -> RivalPad {
    RivalPad {
        jump: false,
        attack: false,
        guard: false,
        flow: false,
        command: Command::None,
        ..pad
    }
}

/// The joiner's buttons for this host tick: the oldest unused one, older
/// ones merged in when too many wait.
pub fn next_input(
    inputs: &mut std::collections::BTreeMap<u32, RivalPad>,
) -> Option<(u32, RivalPad)> {
    while inputs.len() > INPUT_BACKLOG {
        let (_, old) = inputs.pop_first()?;
        if let Some(mut next) = inputs.first_entry() {
            let merged = merge(old, *next.get());
            *next.get_mut() = merged;
        }
    }
    inputs.pop_first()
}

/// Host, before the rivals move: the joiner's buttons for its Sora.
pub fn host_take_inputs(net: Option<ResMut<DuelNet>>, mut game: ResMut<Game>) {
    let Some(mut net) = net else { return };
    if net.role != Role::Host || !net.playing() {
        return;
    }
    while let Some(m) = net.session.receive(UNRELIABLE) {
        let Some((note, list)) = duel_wire::decode_input(&m) else {
            continue;
        };
        net.encoder.ack(note.snapshot_ack);
        net.peer_predicts = note.predicting;
        for (tick, pad) in list {
            if net.used_input.is_none_or(|used| tick > used) {
                net.inputs.entry(tick).or_insert(pad);
                net.input_shown.entry(tick).or_insert(note.shown_tick);
            }
        }
    }
    let slot = fighter_slots::JOIN;
    // [Test] `--script-locked`: the buttons answering the snapshot of exactly
    // LOCK_DELAY ticks ago (duel_script.rs).
    let clock = game.ext.online().script;
    if clock.locked {
        let tick = game.tick;
        let key = tick.saturating_sub(duel_script::LOCK_DELAY);
        let pad = net.inputs.remove(&(key as u32));
        net.inputs.retain(|k, _| u64::from(*k) > key);
        let used = !game.ext.online().duel_frozen && clock.host_start().is_some_and(|f| key >= f);
        if let Some(rival) = game.rivals.iter_mut().find(|r| r.slot == slot) {
            rival.pad = pad.filter(|_| used).unwrap_or_default();
        }
        if used && pad.is_none() {
            net.skipped += 1;
            game.event(serde_json::json!({"event":"duel_lock_miss","tick":tick,"key":key}));
        }
        return;
    }
    let Some(rival) = game.rivals.iter_mut().find(|r| r.slot == slot) else {
        return;
    };
    let last = rival.pad;
    let before = net.inputs.len();
    let mut shown = None;
    match next_input(&mut net.inputs) {
        Some((tick, pad)) => {
            shown = net.input_shown.get(&tick).copied();
            net.input_shown.retain(|k, _| *k > tick);
            if before > INPUT_BACKLOG + 1 {
                net.skipped += (before - INPUT_BACKLOG - 1) as u64;
            }
            net.used_input = Some(tick);
            rival.pad = pad;
            net.starve_run = 0;
        }
        None => {
            rival.pad = held(last);
            net.starved += 1;
            net.starve_run += 1;
            if net.starve_run == LATE_WARNING {
                let tick = game.tick;
                warn(
                    &mut net,
                    format!("Tick {tick}: no buttons from the other player for half a second"),
                );
            }
        }
    }
    if shown.is_some() {
        game.ext.online_mut().rewind.shown = shown;
    }
}

/// Ticks without new buttons or snapshots before the overlay warns.
const LATE_WARNING: u32 = 30;

fn state_of(
    slot: u8,
    fighter: &Fighter,
    kh3: bool,
    started: u64,
    drive: Option<&drive_room::DriveRt>,
    items: Option<&item_room::ItemRt>,
    book: Option<&kh3_player::attack::Book>,
) -> FighterState {
    let (player, vitals) = (&fighter.player, &fighter.vitals);
    FighterState {
        slot,
        position: player.position,
        yaw: player.yaw,
        action: player.action,
        frame: player.frame,
        armed: player.armed,
        blend: player.blend,
        started,
        hp: vitals.hp,
        max_hp: vitals.target.max_hp,
        mp: [player.mp.cur, player.mp.max],
        drive: [
            player.drive.level,
            player.drive.partial,
            player.drive.max,
            player.drive.bars,
        ],
        invulnerable: vitals.invulnerable,
        form: drive.map_or(0, |d| d.shown),
        items: items.map(|i| i.slots.clone()).unwrap_or_default(),
        bullets: drive
            .map(|d| d.bullets.iter().map(|b| (b.key, b.pos, b.dir)).collect())
            .unwrap_or_default(),
        flights: Vec::new(),
        kh3: kh3.then(|| match book {
            Some(book) => fighter.ext.fighter().kh3.shown_in(book),
            None => fighter.ext.fighter().kh3.shown(),
        }),
        motion: (!kh3 && duel_predict::carries_motion(slot))
            .then(|| duel_predict::motion_of(player)),
    }
}

/// Both Soras as the host's simulation has them this tick.
pub fn fighters_of(game: &Game) -> Vec<FighterState> {
    let rules = game.ext.online().duel_rules;
    let book = game.ext.online().kh3_book.clone();
    let book = book.as_deref();
    let mut fighters = vec![state_of(
        fighter_slots::HOST,
        &game.sora,
        rules.seat_kh3(fighter_slots::HOST),
        game.action_started,
        game.drive.as_ref(),
        game.items.as_ref(),
        book,
    )];
    fighters.extend(game.rivals.iter().map(|r| {
        state_of(
            r.slot,
            &r.fighter,
            rules.seat_kh3(r.slot),
            r.started,
            r.own.drive.as_ref(),
            r.own.items.as_ref(),
            book,
        )
    }));
    fighters
}

/// Host, after the field's tick: both Soras and the tick's events.
pub fn host_send(net: Option<ResMut<DuelNet>>, mut game: ResMut<Game>) {
    // Out of a LAN duel nobody reads the events.
    let events = std::mem::take(&mut game.ext.online_mut().duel_events);
    let Some(mut net) = net else { return };
    if net.role != Role::Host || !net.playing() {
        return;
    }
    // The rules, whenever the joiner does not have the current ones.
    if net.sent_rules != Some(game.ext.online().duel_rules) {
        net.sent_rules = Some(game.ext.online().duel_rules);
        net.session.send(
            RELIABLE,
            duel_wire::encode_rules(&game.ext.online().duel_rules),
        );
    }
    net.send_count = net.send_count.wrapping_add(1);
    // [Test] `--script-locked` answers each snapshot, so it has them all.
    let every = if game.ext.online().script.locked {
        1
    } else {
        net.snapshot_every.max(1)
    };
    if net.send_count.is_multiple_of(every) {
        let mut fighters = fighters_of(&game);
        // Only a joiner predicting its Sora reads this: 3.6 KB/s otherwise spent.
        if !net.peer_predicts {
            for f in &mut fighters {
                f.motion = None;
            }
        }
        let snapshot = Snapshot {
            tick: game.tick,
            input_ack: net.used_input.unwrap_or(0),
            fighters,
        };
        let encoded = net.encoder.encode(&snapshot);
        net.flight_bytes += encoded.flight_bytes as u64;
        net.session.send(UNRELIABLE, encoded.bytes);
    }
    if !events.is_empty() {
        net.session
            .send(RELIABLE, duel_wire::encode_events(game.tick, &events));
    }
}

/// Puts a snapshot's numbers on a Sora. Returns whether a new action began
/// (the presenter restarts its blend). A Sora this side predicts keeps his own
/// motion (`moved` false): the rest is the host's.
fn apply_state(
    s: &FighterState,
    fighter: &mut Fighter,
    book: Option<&kh3_player::attack::Book>,
    moved: bool,
) -> bool {
    if let Some(k) = &s.kh3 {
        fighter.ext.fighter_mut().kh3.apply_shown(k);
        if let Some(book) = book {
            fighter.ext.fighter_mut().kh3.apply_actors(k, book);
        }
    }
    let (player, vitals) = (&mut fighter.player, &mut fighter.vitals);
    if let Some(k) = &s.kh3 {
        // MP Charge, for the HUD's gauge and the deck's rows.
        player.mp.total = if k.charge > 0 { 255. } else { 0. };
        player.mp.remaining = f32::from(k.charge);
    }
    let new_action = moved && (player.action != s.action || player.frame > s.frame);
    if moved {
        player.position = s.position;
        player.yaw = s.yaw;
        player.action = s.action;
        player.frame = s.frame;
        player.armed = s.armed;
        player.blend = s.blend;
    }
    player.mp.cur = s.mp[0];
    player.mp.max = s.mp[1];
    [
        player.drive.level,
        player.drive.partial,
        player.drive.max,
        player.drive.bars,
    ] = s.drive;
    vitals.hp = s.hp;
    vitals.target.max_hp = s.max_hp;
    vitals.invulnerable = s.invulnerable;
    new_action
}

/// What the joiner does with the host's events: the same sounds and
/// sparks it would have played itself.
fn play_event(game: &mut Game, event: &DuelEvent) {
    match *event {
        DuelEvent::Sound(id) => game.sound_queue.push((id, "duel")),
        DuelEvent::OwnSwing(id) => {
            if !duel_predict::already_played(game, id) {
                game.sound_queue.push((id, "duel"));
            }
        }
        DuelEvent::Spark {
            effect,
            random,
            contact,
            direction,
            own,
        } => game.effect_queue.push(EffectRequest::Hit {
            effect,
            random,
            contact: Vec3::from_array(contact),
            direction: Vec3::from_array(direction),
            own,
            second: false,
        }),
        DuelEvent::Guard(at, flash) => game.effect_queue.push(EffectRequest::System {
            id: flash,
            at: Vec3::from_array(at),
        }),
        DuelEvent::KnockOut(slot) => {
            let tick = game.tick;
            game.event(serde_json::json!({"event":"duel_knock_out","tick":tick,"slot":slot}));
        }
        DuelEvent::StoodUp(slot) => {
            let tick = game.tick;
            game.event(serde_json::json!({"event":"duel_stood_up","tick":tick,"slot":slot}));
        }
        DuelEvent::Spell { .. } | DuelEvent::DriveFx { .. } => duel_fx::play(game, event),
        DuelEvent::Look {
            look,
            slot,
            size,
            at,
        } => duel_kh3::play_look(game, look, slot, size, at),
        DuelEvent::Kh3Fx {
            effect,
            sound,
            at,
            yaw,
            attach,
        } => {
            let name = |id: u16| {
                game.ext
                    .online()
                    .kh3_book
                    .as_ref()
                    .and_then(|b| b.names.get(usize::from(id)).cloned())
            };
            let request = kh3_player::spell::FxRequest {
                effect: name(effect),
                sound: name(sound),
                at,
                yaw,
                attach: attach.map(|a| kh3_player::spell::FxAttach {
                    seat: a.seat,
                    bone: name(a.bone),
                    offset: a.offset,
                    handle: a.handle,
                    group: a.group,
                    stop: match a.stop {
                        0 => None,
                        1 => Some(false),
                        _ => Some(true),
                    },
                }),
            };
            game.ext.online_mut().kh3_fx_remote.push(request);
        }
    }
}

/// What the joiner is gliding between: the snapshot last shown, and how each Sora
/// was moving at it (units a tick), for a gap with nothing to glide to.
#[derive(Clone, Debug, Default)]
pub struct Glide {
    from: Option<Snapshot>,
    velocity: Vec<(u8, [f32; 3])>,
}

impl Glide {
    /// The tick of the snapshot last shown.
    fn shown(&self) -> Option<u64> {
        self.from.as_ref().map(|s| s.tick)
    }

    fn step(&mut self, next: &Snapshot) {
        self.velocity.clear();
        if let Some(from) = &self.from {
            let span = next.tick.saturating_sub(from.tick).max(1) as f32;
            for f in &next.fighters {
                if let Some(old) = from.fighters.iter().find(|o| o.slot == f.slot) {
                    let d = [0, 1, 2].map(|k| (f.position[k] - old.position[k]) / span);
                    self.velocity.push((f.slot, d));
                }
            }
        }
        self.from = Some(next.clone());
    }
}

/// Where a Sora is at host tick `play`, and which way he faces, frame on: between
/// `from` and `to`, or carried on from `from` when no newer one has come. Actions
/// stay `from`'s [duel choice, KHC-026].
pub fn glided(
    from: (u64, &FighterState),
    to: Option<(u64, &FighterState)>,
    velocity: Option<[f32; 3]>,
    play: f64,
) -> ([f32; 3], f32, u32) {
    let (at, a) = from;
    let past = (play - at as f64).max(0.) as f32;
    let Some((to_at, b)) = to else {
        let v = velocity.unwrap_or([0.; 3]);
        return (
            [0, 1, 2].map(|k| a.position[k] + v[k] * past),
            a.yaw,
            a.frame,
        );
    };
    let span = to_at.saturating_sub(at).max(1) as f32;
    let t = (past / span).clamp(0., 1.);
    let far = (0..3)
        .map(|k| (b.position[k] - a.position[k]).powi(2))
        .sum::<f32>()
        .sqrt();
    if far > PUT_THERE * span {
        return (a.position, a.yaw, a.frame);
    }
    let turn = (b.yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let same = a.action == b.action && b.frame >= a.frame;
    let frame = if same {
        a.frame + ((b.frame - a.frame) as f32 * t).round() as u32
    } else {
        a.frame
    };
    (
        [0, 1, 2].map(|k| a.position[k] + (b.position[k] - a.position[k]) * t),
        a.yaw + turn * t,
        frame,
    )
}

/// The Soras between the snapshot shown and the next, this tick.
fn glide_soras(net: &DuelNet, game: &mut Game, play: f64, predicting: bool) {
    let Some(from) = &net.glide.from else { return };
    let next = net.snapshots.first_key_value();
    for f in &from.fighters {
        let to = next.and_then(|(t, s)| {
            s.fighters
                .iter()
                .find(|o| o.slot == f.slot)
                .map(|o| (*t, o))
        });
        let velocity = net
            .glide
            .velocity
            .iter()
            .find(|v| v.0 == f.slot)
            .map(|v| v.1);
        let (position, yaw, frame) = glided((from.tick, f), to, velocity, play);
        let player = if f.slot == fighter_slots::JOIN {
            if predicting {
                continue;
            }
            &mut game.sora.player
        } else if let Some(r) = game.rivals.iter_mut().find(|r| r.slot == f.slot) {
            &mut r.fighter.player
        } else {
            continue;
        };
        player.position = position;
        player.yaw = yaw;
        player.frame = frame;
    }
}

/// Joiner, before its field tick: the host's snapshots and events.
pub fn join_apply(net: Option<ResMut<DuelNet>>, mut game: ResMut<Game>) {
    let Some(mut net) = net else { return };
    if net.role != Role::Join || !net.playing() {
        return;
    }
    let queued = game.ext.online().script.locked || net.session.is_playback();
    // A replay's fast speed feeds several a tick: all of them are decoded, as later
    // ones may start from them, and the newest is what is shown.
    let playback = net.session.is_playback();
    let mut batch = None;
    while let Some(m) = net.session.receive(UNRELIABLE) {
        let Some(s) = net.decoder.decode(&m) else {
            continue;
        };
        if playback {
            batch = Some(s);
            continue;
        }
        let fresh = if queued {
            net.shown_snapshot.is_none_or(|shown| s.tick > shown)
        } else {
            net.glide.shown().is_none_or(|shown| s.tick > shown)
        };
        if fresh {
            if !queued {
                net.playout.arrive(s.tick);
            }
            net.snapshots.insert(s.tick, s);
        }
    }
    if let Some(s) = batch
        && net.shown_snapshot.is_none_or(|shown| s.tick > shown)
    {
        net.snapshots.insert(s.tick, s);
    }
    for m in std::mem::take(&mut net.inbox) {
        if let Some((tick, events)) = duel_wire::decode_events(&m) {
            // Played with the snapshot of the tick they happened on.
            net.events.push_back((tick, events));
        } else if let Some(rules) = duel_wire::decode_rules(&m) {
            game.ext.online_mut().duel_rules = rules;
        } else {
            net.inbox.push(m);
        }
    }
    // This player's own Sora answers to the newest snapshot, not to the one
    // being shown: nothing of his needs to ride out jitter.
    let predicting = duel_predict::active(&game);
    if predicting && let Some(newest) = net.snapshots.last_key_value().map(|(_, s)| s.clone()) {
        duel_predict::compare(&mut game, &newest);
    }
    // What goes on screen this tick, and the host tick it stands at.
    let (snapshot, clock, play) = if queued {
        // [Test] `--script-locked` answers every snapshot from the script's start
        // on; before it a joiner still loading catches up as usual.
        let answers = {
            let clock = &game.ext.online().script;
            clock.locked && clock.host_start().is_some()
        };
        if net.snapshots.len() > SNAPSHOT_BACKLOG && !answers {
            let skip = net.snapshots.len() - SNAPSHOT_DELAY;
            for _ in 0..skip {
                net.snapshots.pop_first();
            }
            net.skipped += skip as u64;
            let tick = game.tick;
            warn(
                &mut net,
                format!("Tick {tick}: skipped {skip} snapshots to catch up"),
            );
        }
        let due = net.snapshots.len() > SNAPSHOT_DELAY
            || (net.shown_snapshot.is_none() && !net.snapshots.is_empty());
        let next = due.then(|| net.snapshots.pop_first()).flatten();
        let clock = next.as_ref().map(|n| n.0);
        (next.map(|n| n.1), clock, None)
    } else {
        let Some(advance) = net.playout.advance() else {
            return;
        };
        if let Some(jumped) = advance.jumped {
            net.skipped += jumped;
            net.jumps += 1;
            let tick = game.tick;
            warn(
                &mut net,
                format!("Tick {tick}: skipped {jumped} snapshots to catch up"),
            );
        }
        let clock = advance.play.max(0.).floor() as u64;
        // The newest come due; those it passed in one tick are not shown.
        let mut latest = None;
        while net
            .snapshots
            .first_key_value()
            .is_some_and(|(t, _)| *t <= clock)
        {
            latest = net.snapshots.pop_first().map(|n| n.1);
        }
        if advance.frozen {
            net.stale += 1;
            net.stale_run += 1;
            net.froze_longest = net.froze_longest.max(net.stale_run);
        } else {
            net.stale_run = 0;
        }
        // Nothing has been shown yet: the clock waits with it.
        let clock = (latest.is_some() || net.glide.shown().is_some()).then_some(clock);
        (latest, clock, Some(advance.play))
    };
    if queued && snapshot.is_none() {
        net.stale += 1;
        net.stale_run += 1;
    }
    if net.stale_run >= LATE_WARNING {
        if net.stale_run == LATE_WARNING {
            let tick = game.tick;
            warn(
                &mut net,
                format!("Tick {tick}: no snapshot from the host for half a second"),
            );
        }
        // Without snapshots the events still come, a little late.
        for (_, events) in std::mem::take(&mut net.events) {
            for e in &events {
                play_event(&mut game, e);
            }
        }
    }
    let Some(clock) = clock else {
        return;
    };
    net.shown_snapshot = Some(clock);
    if let Some(snapshot) = snapshot {
        if queued {
            net.stale_run = 0;
        } else {
            net.glide.step(&snapshot);
        }
        show_snapshot(&mut game, &snapshot, predicting);
    }
    if let Some(play) = play {
        glide_soras(&net, &mut game, play, predicting);
    }
    while net.events.front().is_some_and(|(t, _)| *t <= clock) {
        let (_, events) = net.events.pop_front().unwrap();
        for e in &events {
            play_event(&mut game, e);
        }
    }
}

/// A snapshot's whole state on both Soras.
fn show_snapshot(game: &mut Game, snapshot: &Snapshot, predicting: bool) {
    let now = game.tick;
    // Every form's kit loads ahead, for the host's changes to show at once;
    // a new Keyblade (the lobby's builds) replaces the old once loaded.
    if let Some(d) = game.drive.as_mut() {
        d.preload();
    }
    drive_room::rebuild_weapons(game);
    // [KH3 ruleset] Wisdom's container is named by the host's KH3 cues
    // (the airstep's afterimage, the bolts' flash and sparks): ready on
    // both Soras before an event names it.
    let kh3 = game.ext.online().duel_rules.kh3();
    if kh3 && let Some(d) = game.drive.as_mut() {
        d.wisdom_pax();
    }
    let mut rivals = std::mem::take(&mut game.rivals);
    for r in &mut rivals {
        if let Some(d) = r.own.drive.as_mut() {
            d.preload();
            if kh3 {
                d.wisdom_pax();
            }
        }
        rival::swap(game, r);
        drive_room::rebuild_weapons(game);
        rival::swap(game, r);
    }
    game.rivals = rivals;
    for s in &snapshot.fighters {
        if s.slot == fighter_slots::JOIN {
            // The form's kit first: the action names one of its motions.
            if !predicting {
                show_form(game, s.slot, s.form);
            }
            if let Some(it) = game.items.as_mut() {
                it.slots.clone_from(&s.items);
            }
            if let Some(d) = game.drive.as_mut() {
                d.show_bullets(&s.bullets, &mut game.fx_styled);
            }
            let book = game.ext.online().kh3_book.clone();
            if apply_state(s, &mut game.sora, book.as_deref(), !predicting) {
                game.action_started = now;
            }
        } else if let Some(i) = game.rivals.iter().position(|r| r.slot == s.slot) {
            let mut r = game.rivals.swap_remove(i);
            rival::swap(game, &mut r);
            show_form(game, s.slot, s.form);
            rival::swap(game, &mut r);
            if let Some(d) = r.own.drive.as_mut() {
                d.show_bullets(&s.bullets, &mut game.fx_styled);
            }
            let book = game.ext.online().kh3_book.clone();
            if apply_state(s, &mut r.fighter, book.as_deref(), true) {
                r.started = now;
            }
            game.rivals.insert(i, r);
        }
    }
}

/// The form the host's snapshot names, on the Sora swapped in; the change
/// goes in the log.
pub fn show_form(game: &mut Game, slot: u8, form: u8) {
    let before = game.drive.as_ref().map(|d| d.shown);
    drive_room::show_form(game, form);
    let after = game.drive.as_ref().map(|d| d.shown);
    if after != before {
        let tick = game.tick;
        game.event(serde_json::json!({"event":"duel_form_shown","tick":tick,
            "slot":slot,"form":after}));
    }
}

/// Joiner, after its field tick: this tick's buttons, with the last few.
pub fn join_send(net: Option<ResMut<DuelNet>>, mut game: ResMut<Game>) {
    let Some(mut net) = net else { return };
    if net.role != Role::Join || !net.playing() {
        return;
    }
    // [Test] `--script-locked`: one answer to each snapshot, by its tick.
    let locked = game.ext.online().script.locked;
    let tick = if locked {
        net.shown_snapshot.unwrap_or(0) as u32
    } else {
        game.tick as u32
    };
    if !locked || net.recent.back().is_none_or(|(t, _)| *t != tick) {
        if net.recent.len() >= duel_wire::INPUT_REDUNDANCY {
            net.recent.pop_front();
        }
        let pad = game.ext.online().net_pad;
        net.recent.push_back((tick, pad));
        if locked {
            let at = game.tick;
            game.event(
                serde_json::json!({"event":"duel_lock_pad","tick":at,"key":tick,
                "pad":format!("{pad:?}")}),
            );
        }
    }
    if !locked {
        let pad = game.ext.online().net_pad;
        duel_predict::remember(&mut game, tick, pad);
    }
    let recent: Vec<_> = net.recent.iter().copied().collect();
    let note = duel_wire::InputNote {
        shown_tick: net.shown_snapshot.unwrap_or(0),
        snapshot_ack: net.decoder.newest().unwrap_or(0),
        predicting: game.ext.online().predict.on,
    };
    net.session
        .send(UNRELIABLE, duel_wire::encode_input(&recent, note));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn press(attack: bool, x: f32) -> RivalPad {
        RivalPad {
            direction: [x, 0.],
            attack,
            ..RivalPad::default()
        }
    }

    fn fighter(position: [f32; 3], yaw: f32, action: Action, frame: u32) -> FighterState {
        FighterState {
            slot: 0,
            position,
            yaw,
            action,
            frame,
            armed: false,
            blend: 0.,
            started: 0,
            hp: 1,
            max_hp: 1,
            mp: [0; 2],
            drive: [0; 4],
            invulnerable: 0.,
            form: 0,
            items: Vec::new(),
            bullets: Vec::new(),
            flights: Vec::new(),
            kh3: None,
            motion: None,
        }
    }

    #[test]
    fn a_sora_glides_between_snapshots_and_his_action_does_not() {
        let a = fighter([0., 0., 0.], 3., Action::Run, 10);
        let b = fighter([10., 0., 20.], -3., Action::Run, 12);
        let (p, yaw, frame) = glided((100, &a), Some((102, &b)), None, 101.);
        assert_eq!(p, [5., 0., 10.]);
        // The short way round, through the back.
        let turn = (std::f32::consts::TAU - 6.) / 2.;
        assert!((yaw - (3. + turn)).abs() < 1e-5, "{yaw}");
        assert_eq!(frame, 11);
        // Another action waits for its snapshot.
        let c = fighter([10., 0., 20.], 3., Action::Attack(1), 0);
        assert_eq!(glided((100, &a), Some((102, &c)), None, 101.).2, 10);
    }

    #[test]
    fn a_sora_put_somewhere_does_not_glide_there() {
        let a = fighter([0., 0., 0.], 0., Action::Idle, 0);
        let b = fighter([600., 0., 0.], 0., Action::Idle, 0);
        assert_eq!(glided((5, &a), Some((6, &b)), None, 5.5).0, [0., 0., 0.]);
    }

    #[test]
    fn a_gap_carries_him_on_at_his_speed() {
        let a = fighter([1., 2., 3.], 1., Action::Run, 7);
        let (p, yaw, frame) = glided((10, &a), None, Some([2., 0., -1.]), 13.);
        assert_eq!(p, [7., 2., 0.]);
        assert_eq!((yaw, frame), (1., 7));
        assert_eq!(glided((10, &a), None, None, 13.).0, [1., 2., 3.]);
    }

    #[test]
    fn inputs_come_one_per_tick_in_order() {
        let mut q: BTreeMap<u32, RivalPad> = [(5, press(false, 0.1)), (6, press(true, 0.2))].into();
        assert_eq!(next_input(&mut q).unwrap().0, 5);
        assert_eq!(next_input(&mut q).unwrap().0, 6);
        assert!(next_input(&mut q).is_none());
    }

    #[test]
    fn a_backlog_is_merged_without_losing_presses() {
        let mut q: BTreeMap<u32, RivalPad> =
            (0..8u32).map(|t| (t, press(t == 1, t as f32))).collect();
        let (tick, pad) = next_input(&mut q).unwrap();
        // Four wait at most: ticks 0..=3 were folded into tick 4.
        assert_eq!(tick, 4);
        assert!(pad.attack, "the press of tick 1 survives");
        assert_eq!(pad.direction, [4., 0.]);
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn missing_ticks_hold_the_stick_but_not_presses() {
        let p = RivalPad {
            direction: [0.5, 0.5],
            jump: true,
            jump_held: true,
            attack: true,
            guard: true,
            guard_held: true,
            shotlock_held: true,
            flow: true,
            command: Command::Cast(1),
        };
        let h = held(p);
        assert_eq!(h.command, Command::None);
        assert_eq!(h.direction, p.direction);
        assert!(h.jump_held && h.guard_held && h.shotlock_held);
        assert!(!h.jump && !h.attack && !h.guard && !h.flow);
    }
}
