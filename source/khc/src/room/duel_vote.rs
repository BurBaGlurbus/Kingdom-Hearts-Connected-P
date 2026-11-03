//! A vote between the two players, for any choice a duel puts to both
//! (the party, the arena, the rules, later). Each player casts a ballot;
//! the same choice wins, different choices go to a coin flip, sent over the
//! reliable channel with its seed so both screens play the same flip. The
//! seed is both players' [KHC-016]: each sends a hash of a random number,
//! then the number once both hashes are in, and the seed is a hash of the
//! two, so neither can choose how the coin lands; a number that does not
//! match its hash is refused. On screen the vote is the game's pause menu (duel_camp.rs):
//! the choices are its entries and the help line says where the vote
//! stands, the coin's face as it spins. Only the F3 test vote uses it for
//! now.

use super::duel_net::{DuelNet, Status};
use super::duel_wire::{BALLOT, COIN_COMMIT, COIN_REVEAL, VOTE};
use super::*;
use net::wire::{Reader, Writer};
use net::{RELIABLE, Role};

/// se000: cursor, decide.
const SE_MOVE: u32 = 1;
const SE_DECIDE: u32 = 2;
/// Logical pad bits of mode 7.
const UP: u64 = 0x10;
const DOWN: u64 = 0x20;
const DECIDE: u64 = 0x4;

/// Ticks the coin spins before it lands, and how often it turns over.
pub const FLIP_TICKS: u64 = 90;
const TURN_TICKS: u64 = 6;
/// Ticks the result stays on screen.
const RESULT_TICKS: u64 = 150;

/// A vote's question and choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    /// What the vote is for (a caller's own number).
    pub topic: u8,
    pub question: String,
    pub options: Vec<String>,
    /// Each seat's ballot (0 the host, 1 the joiner).
    pub ballots: [Option<u8>; 2],
    pub outcome: Option<Outcome>,
}

/// How a vote ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub choice: u8,
    /// The coin flip's seed when the ballots differed.
    pub flip: Option<u32>,
}

impl Vote {
    pub fn new(topic: u8, question: &str, options: &[&str]) -> Self {
        Vote {
            topic,
            question: question.to_string(),
            options: options.iter().map(|o| o.to_string()).collect(),
            ballots: [None; 2],
            outcome: None,
        }
    }

    /// A seat's ballot; once cast it stands.
    pub fn cast(&mut self, seat: usize, choice: u8) -> Result<(), String> {
        if self.outcome.is_some() {
            return Err("the vote is over".into());
        }
        if usize::from(choice) >= self.options.len() {
            return Err("no such choice".into());
        }
        let ballot = self.ballots.get_mut(seat).ok_or("no such player")?;
        if ballot.is_some() {
            return Err("already voted".into());
        }
        *ballot = Some(choice);
        Ok(())
    }

    /// Host: once both have voted, the outcome; a tie flips the coin with
    /// `seed`.
    pub fn decide(&mut self, seed: u32) -> Option<Outcome> {
        if self.outcome.is_none()
            && let [Some(a), Some(b)] = self.ballots
        {
            self.outcome = Some(if a == b {
                Outcome {
                    choice: a,
                    flip: None,
                }
            } else {
                Outcome {
                    choice: flip_result(seed, a, b),
                    flip: Some(seed),
                }
            });
        }
        self.outcome
    }
}

/// The coin: the seed's low bit picks one of the two ballots.
pub fn flip_result(seed: u32, a: u8, b: u8) -> u8 {
    if seed & 1 == 0 { a } else { b }
}

/// Which of the two tied ballots the spinning coin shows at a tick of its
/// flip (it turns every few ticks and lands on the result).
pub fn flip_face(seed: u32, a: u8, b: u8, ticks: u64) -> u8 {
    if ticks >= FLIP_TICKS {
        return flip_result(seed, a, b);
    }
    // Turns still to come; the last one shows the result.
    let turns = (FLIP_TICKS - ticks - 1) / TURN_TICKS;
    let land = flip_result(seed, a, b);
    if turns.is_multiple_of(2) {
        land
    } else if land == a {
        b
    } else {
        a
    }
}

/// [Duel choice] The tie's coin as it spins and lands: its width as a share
/// of its height (edge-on as each turn ends, when the face changes) and its
/// size (a short pop on landing), by the tick (`frac`: the part of a tick
/// since), so it turns at the same speed at any frame rate.
pub fn coin(room: &VoteRoom, game: &Game, frac: f32) -> Option<(f32, f32)> {
    let v = room.vote.as_ref()?;
    let (o, at) = (v.outcome?, room.decided_at?);
    o.flip?;
    let t = game.tick.saturating_sub(at) as f32 + frac;
    let flip = FLIP_TICKS as f32;
    if t >= flip + 45. {
        return None;
    }
    if t >= flip {
        let pop = (1. - (t - flip) / 12.).max(0.);
        return Some((1., 1. + 0.15 * pop));
    }
    let u = (t % TURN_TICKS as f32) / TURN_TICKS as f32;
    Some(((u * std::f32::consts::PI).sin().max(0.06), 1.))
}

pub fn encode_vote(vote: &Vote) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(VOTE);
    w.u8(vote.topic);
    w.text(&vote.question, 80);
    w.u8(vote.options.len().min(8) as u8);
    for o in vote.options.iter().take(8) {
        w.text(o, 40);
    }
    for b in vote.ballots {
        w.u8(b.unwrap_or(u8::MAX));
    }
    match vote.outcome {
        Some(o) => {
            w.u8(1);
            w.u8(o.choice);
            match o.flip {
                Some(seed) => {
                    w.u8(1);
                    w.u32(seed);
                }
                None => w.u8(0),
            }
        }
        None => w.u8(0),
    }
    w.finish()
}

pub fn decode_vote(bytes: &[u8]) -> Option<Vote> {
    let mut r = Reader::new(bytes);
    if r.u8()? != VOTE {
        return None;
    }
    let topic = r.u8()?;
    let question = r.text(80)?;
    let n = r.u8()? as usize;
    if !(2..=8).contains(&n) {
        return None;
    }
    let options = (0..n).map(|_| r.text(40)).collect::<Option<Vec<_>>>()?;
    let mut ballots = [None; 2];
    for b in &mut ballots {
        let v = r.u8()?;
        *b = (v != u8::MAX).then_some(v);
    }
    let outcome = match r.u8()? {
        0 => None,
        _ => {
            let choice = r.u8()?;
            let flip = match r.u8()? {
                0 => None,
                _ => Some(r.u32()?),
            };
            Some(Outcome { choice, flip })
        }
    };
    let valid = |c: &u8| usize::from(*c) < n;
    (r.is_empty()
        && ballots.iter().flatten().all(valid)
        && outcome.is_none_or(|o| valid(&o.choice)))
    .then_some(Vote {
        topic,
        question,
        options,
        ballots,
        outcome,
    })
}

/// [KHC-016] One vote's coin, by commit and reveal: this player's secret number, and the other's commitment and
/// number as they arrive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coin {
    topic: u8,
    secret: [u8; 16],
    peer_commit: Option<[u8; 32]>,
    peer_secret: Option<[u8; 16]>,
    /// This player's commitment and number went out.
    committed: bool,
    revealed: bool,
}

impl Coin {
    pub fn new(topic: u8, secret: [u8; 16]) -> Self {
        Coin {
            topic,
            secret,
            peer_commit: None,
            peer_secret: None,
            committed: false,
            revealed: false,
        }
    }

    /// A random secret: the standard library's per-process random hasher keys over the clock.
    pub fn random_secret() -> [u8; 16] {
        use std::hash::{BuildHasher, Hasher};
        let mut out = [0; 16];
        for half in out.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos()),
            );
            half.copy_from_slice(&h.finish().to_le_bytes());
        }
        out
    }

    /// What a secret commits to for a vote's topic.
    pub fn commitment(topic: u8, secret: &[u8; 16]) -> [u8; 32] {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(b"khc coin");
        h.update([topic]);
        h.update(secret);
        h.finalize().into()
    }

    /// This player's commitment, to send first.
    pub fn own_commit(&self) -> [u8; 32] {
        Self::commitment(self.topic, &self.secret)
    }

    /// This player's number, to send once the other's commitment is in.
    pub fn own_secret(&self) -> Option<[u8; 16]> {
        self.peer_commit.map(|_| self.secret)
    }

    /// The other player's commitment; the first one stands.
    pub fn take_commit(&mut self, topic: u8, hash: [u8; 32]) {
        if topic == self.topic && self.peer_commit.is_none() {
            self.peer_commit = Some(hash);
        }
    }

    /// The other player's number: refused unless it is what their commitment named.
    pub fn take_reveal(&mut self, topic: u8, secret: [u8; 16]) -> Result<(), &'static str> {
        if topic != self.topic {
            return Err("a coin number for another vote");
        }
        match self.peer_commit {
            None => Err("a coin number before its commitment"),
            Some(c) if c != Self::commitment(topic, &secret) => {
                Err("a coin number that does not match its commitment")
            }
            Some(_) => {
                self.peer_secret = Some(secret);
                Ok(())
            }
        }
    }

    /// The coin's seed once both numbers are known: the host's and the joiner's, hashed together.
    pub fn seed(&self, host: bool) -> Option<u32> {
        use sha2::Digest;
        let peer = self.peer_secret?;
        let (h, j) = if host {
            (self.secret, peer)
        } else {
            (peer, self.secret)
        };
        let mut d = sha2::Sha256::new();
        d.update(h);
        d.update(j);
        let sum = d.finalize();
        Some(u32::from_le_bytes([sum[0], sum[1], sum[2], sum[3]]))
    }
}

/// A coin message: its kind, the vote's topic, then the bytes.
fn coin_message(kind: u8, topic: u8, bytes: &[u8]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(kind);
    w.u8(topic);
    w.bytes(bytes);
    w.finish()
}

/// The duel's vote in the room: the one running (if any), this player's
/// cursor, and when its outcome arrived.
#[derive(Resource, Default)]
pub struct VoteRoom {
    pub vote: Option<Vote>,
    cursor: usize,
    /// The tick the outcome reached this instance (the flip's clock).
    decided_at: Option<u64>,
    /// Host: the vote the joiner has.
    sent: Option<Vote>,
    /// Votes this instance has seen (the pause menu's version).
    serial: u32,
    /// The running vote's coin.
    coin: Option<Coin>,
}

/// The F3 test vote's topic.
pub const TOPIC_TEST: u8 = 0;

impl VoteRoom {
    /// The vote holds the pad: one is open and this player has not voted.
    pub fn menu_up(&self, seat: usize) -> bool {
        self.vote
            .as_ref()
            .is_some_and(|v| v.ballots[seat].is_none())
    }

    /// Host: puts a question to both players.
    pub fn start(&mut self, vote: Vote) {
        *self = VoteRoom {
            coin: Some(Coin::new(vote.topic, Coin::random_secret())),
            vote: Some(vote),
            serial: self.serial + 1,
            ..VoteRoom::default()
        };
    }
}

fn seat_of(net: &DuelNet) -> usize {
    usize::from(net.role == Role::Join)
}

/// One fixed step: ballots and outcomes over the wire, this player's
/// choice, the result's time on screen.
pub fn tick(
    net: Option<ResMut<DuelNet>>,
    room: Option<ResMut<VoteRoom>>,
    mut camp: Option<ResMut<CampRes>>,
    mut game: ResMut<Game>,
) {
    let (Some(mut net), Some(mut room)) = (net, room) else {
        return;
    };
    let (net, room, game) = (&mut *net, &mut *room, &mut *game);
    if !matches!(net.status, Status::Playing) {
        *room = VoteRoom {
            serial: room.serial,
            ..VoteRoom::default()
        };
        game.ext.online_mut().duel_vote_up = false;
        if let Some(c) = camp.as_deref_mut() {
            duel_camp::hold(c, game, duel_camp::VOTE, None, || unreachable!());
        }
        return;
    }
    let seat = seat_of(net);
    // Host: F3's test question.
    if std::mem::take(&mut game.ext.online_mut().duel_test_vote) && net.role == Role::Host {
        room.start(Vote::new(TOPIC_TEST, "Test vote", &["Left", "Right"]));
        game.sound_queue.push((SE_DECIDE, "vote"));
    }
    for m in std::mem::take(&mut net.inbox) {
        match (net.role, m.first().copied()) {
            (Role::Host, Some(BALLOT)) => {
                let mut r = Reader::new(&m);
                let _ = r.u8();
                if let (Some(topic), Some(choice), Some(v)) = (r.u8(), r.u8(), room.vote.as_mut())
                    && v.topic == topic
                {
                    let _ = v.cast(1, choice);
                }
            }
            (Role::Join, Some(VOTE)) => {
                if let Some(v) = decode_vote(&m) {
                    if room
                        .vote
                        .as_ref()
                        .is_none_or(|old| old.topic != v.topic || old.question != v.question)
                    {
                        room.cursor = 0;
                        room.decided_at = None;
                        room.serial += 1;
                        room.coin = Some(Coin::new(v.topic, Coin::random_secret()));
                    }
                    room.vote = Some(v);
                }
            }
            (_, Some(COIN_COMMIT)) => {
                let mut r = Reader::new(&m);
                let _ = r.u8();
                if let (Some(topic), Some(hash), Some(c)) = (r.u8(), r.array(), room.coin.as_mut())
                {
                    c.take_commit(topic, hash);
                }
            }
            (_, Some(COIN_REVEAL)) => {
                let mut r = Reader::new(&m);
                let _ = r.u8();
                if let (Some(topic), Some(secret), Some(c)) =
                    (r.u8(), r.array(), room.coin.as_mut())
                    && let Err(why) = c.take_reveal(topic, secret)
                {
                    let tick = game.tick;
                    game.event(serde_json::json!({"event":"coin_refused","tick":tick,"why":why}));
                    // The host lets the vote go: its coin cannot be trusted.
                    if net.role == Role::Host {
                        room.vote = None;
                        room.coin = None;
                    }
                }
            }
            _ => net.inbox.push(m),
        }
    }
    // The coin: this player's commitment, then its number once the other's commitment is in. The host's
    // commitment waits until the vote has gone out, so the joiner has a coin to take it (one reliable channel
    // keeps the order).
    let vote_out = net.role == Role::Join || room.sent.is_some();
    if let Some(c) = room.coin.as_mut() {
        if !c.committed && vote_out {
            c.committed = true;
            net.session.send(
                RELIABLE,
                coin_message(COIN_COMMIT, c.topic, &c.own_commit()),
            );
        }
        if !c.revealed
            && let Some(secret) = c.own_secret()
        {
            c.revealed = true;
            net.session
                .send(RELIABLE, coin_message(COIN_REVEAL, c.topic, &secret));
        }
    }
    // The pause menu: the vote's choices, its state on the help line.
    game.ext.online_mut().duel_vote_up = room.vote.is_some();
    // [Test] `--duel-shots`: the coin as it spins.
    if let Some(at) = room.decided_at.filter(|&at| game.tick == at + 60) {
        duel_camp::shot(game, (4, at, 0));
    }
    let mut picked = None;
    if let Some(c) = camp.as_deref_mut() {
        let want = room.vote.as_ref().map(|_| room.serial);
        let status = status(room, net, game);
        let options = room
            .vote
            .as_ref()
            .map(|v| v.options.clone())
            .unwrap_or_default();
        let menu = duel_camp::hold(c, game, duel_camp::VOTE, want, || duel_camp::Request {
            entries: options
                .iter()
                .enumerate()
                .map(|(i, o)| duel_camp::entry(pause_menu::CALLER_ENTRY + i as u8, o, &status))
                .collect(),
            on_config: false,
            pages: pause_menu::DuelPages::default(),
        });
        if let Some(menu) = menu {
            for i in 0..options.len() {
                duel_camp::set_help(menu, pause_menu::CALLER_ENTRY + i as u8, &status);
            }
            picked = duel_camp::picked(menu).map(|id| usize::from(id - pause_menu::CALLER_ENTRY));
        }
    }
    let Some(vote) = room.vote.as_mut() else {
        return;
    };
    // This player's ballot.
    if vote.ballots[seat].is_none() {
        let pressed = if game.autoplay.is_some() || camp.is_some() {
            // [Test] Unattended: the host takes the first choice, the
            // joiner the second (a tie, so the coin flips). With the pause
            // menu, its pick is the ballot.
            0
        } else {
            game.pad.pressed | game.pad.repeat & (UP | DOWN)
        };
        let n = vote.options.len();
        let choice = if game.autoplay.is_some() {
            Some(seat.min(n - 1))
        } else if picked.is_some() {
            picked.filter(|&c| c < n)
        } else if pressed & UP != 0 {
            room.cursor = (room.cursor + n - 1) % n;
            game.sound_queue.push((SE_MOVE, "vote"));
            None
        } else if pressed & DOWN != 0 {
            room.cursor = (room.cursor + 1) % n;
            game.sound_queue.push((SE_MOVE, "vote"));
            None
        } else {
            (pressed & DECIDE != 0).then_some(room.cursor)
        };
        if let Some(c) = choice {
            game.sound_queue.push((SE_DECIDE, "vote"));
            if net.role == Role::Host {
                let _ = vote.cast(0, c as u8);
            } else {
                vote.ballots[1] = Some(c as u8);
                let mut w = Writer::default();
                w.u8(BALLOT);
                w.u8(vote.topic);
                w.u8(c as u8);
                net.session.send(RELIABLE, w.finish());
            }
        }
    }
    // Host: the outcome; a tie waits for both players' coin numbers.
    let tied = matches!(vote.ballots, [Some(a), Some(b)] if a != b);
    let seed = room
        .coin
        .as_ref()
        .and_then(|c| c.seed(net.role == Role::Host));
    if net.role == Role::Host
        && vote.outcome.is_none()
        && let Some(seed) = if tied { seed } else { Some(0) }
        && let Some(o) = vote.decide(seed)
    {
        let tick = game.tick;
        game.event(serde_json::json!({"event":"vote_decided","tick":tick,
            "topic":vote.topic,"ballots":vote.ballots,"choice":o.choice,"flip":o.flip}));
    }
    if let Some(o) = vote.outcome
        && room.decided_at.is_none()
    {
        room.decided_at = Some(game.tick);
        // The joiner checks the host's coin against its own reckoning of the two numbers.
        if net.role == Role::Join
            && let Some(flip) = o.flip
            && seed != Some(flip)
        {
            let tick = game.tick;
            game.event(serde_json::json!({"event":"coin_mismatch","tick":tick,
                "host":flip,"own":seed}));
        }
    }
    // The coin turns over with a cursor sound and lands with a decide.
    if let (Some(at), Some(o)) = (room.decided_at, vote.outcome) {
        let t = game.tick - at;
        if o.flip.is_some() && t < FLIP_TICKS && t % TURN_TICKS == 0 {
            game.sound_queue.push((SE_MOVE, "coin"));
        }
        // [Test] `--duel-shots`: the coin mid-turn.
        if o.flip.is_some() && t == 40 {
            duel_camp::shot(game, (6, at, t));
        }
        if o.flip.is_some() && t == FLIP_TICKS {
            game.sound_queue.push((SE_DECIDE, "coin"));
            let tick = game.tick;
            game.event(serde_json::json!({"event":"coin_landed","tick":tick,
                "topic":vote.topic,"choice":o.choice}));
        }
        let shown = if o.flip.is_some() { FLIP_TICKS } else { 0 } + RESULT_TICKS;
        if t >= shown && net.role == Role::Host {
            room.vote = None;
        }
    }
    // Host: the vote to the joiner on every change.
    if net.role == Role::Host && room.sent != room.vote {
        room.sent = room.vote.clone();
        if let Some(v) = &room.vote {
            net.session.send(RELIABLE, encode_vote(v));
        }
    }
    // The joiner lets a finished vote go once its result has been shown.
    if net.role == Role::Join
        && let (Some(at), Some(v)) = (room.decided_at, &room.vote)
        && let Some(o) = v.outcome
    {
        let shown = if o.flip.is_some() { FLIP_TICKS } else { 0 } + RESULT_TICKS;
        if game.tick - at >= shown {
            room.vote = None;
            room.decided_at = None;
        }
    }
}

/// Where the vote stands, for the pause menu's help line.
fn status(room: &VoteRoom, net: &DuelNet, game: &Game) -> String {
    let Some(v) = &room.vote else {
        return String::new();
    };
    let seat = seat_of(net);
    let them = net.peer.as_deref().unwrap_or("Opponent");
    match (v.outcome, room.decided_at) {
        (Some(o), Some(at)) => {
            let t = game.tick - at;
            match o.flip {
                Some(seed) if t < FLIP_TICKS => {
                    let (a, b) = (v.ballots[0].unwrap_or(0), v.ballots[1].unwrap_or(0));
                    let face = flip_face(seed, a, b, t);
                    format!(
                        "A tie: the coin decides... {}",
                        v.options[usize::from(face)]
                    )
                }
                Some(_) => format!(
                    "{}: {}, by coin flip.",
                    v.question,
                    v.options[usize::from(o.choice)]
                ),
                None => format!("{}: {}.", v.question, v.options[usize::from(o.choice)]),
            }
        }
        _ => match v.ballots[seat] {
            None => {
                let theirs = if v.ballots[1 - seat].is_some() {
                    "has voted"
                } else {
                    "is voting"
                };
                format!(
                    "{}: choose one. {them} {theirs}. A tie goes to a coin flip.",
                    v.question
                )
            }
            Some(c) => format!(
                "You chose {}. Waiting for {them}.",
                v.options[usize::from(c)]
            ),
        },
    }
}

/// The vote's lines on screen: (text, highlighted).
pub fn lines(room: &VoteRoom, net: &DuelNet, game: &Game) -> Vec<(String, bool)> {
    let Some(v) = &room.vote else {
        return Vec::new();
    };
    let seat = seat_of(net);
    let them = net.peer.as_deref().unwrap_or("Opponent");
    let mut out = vec![(format!("Vote: {}", v.question), false)];
    match (v.outcome, room.decided_at) {
        (Some(o), Some(at)) => {
            let t = game.tick - at;
            match o.flip {
                Some(seed) if t < FLIP_TICKS => {
                    let (a, b) = (v.ballots[0].unwrap_or(0), v.ballots[1].unwrap_or(0));
                    let face = flip_face(seed, a, b, t);
                    out.push(("Tie: the coin decides".to_string(), false));
                    out.push((format!("   * {} *", v.options[usize::from(face)]), true));
                }
                _ => {
                    let how = if o.flip.is_some() {
                        ", by coin flip"
                    } else {
                        ""
                    };
                    out.push((
                        format!("Result: {}{how}", v.options[usize::from(o.choice)]),
                        true,
                    ));
                }
            }
        }
        _ => {
            if v.ballots[seat].is_none() {
                for (i, o) in v.options.iter().enumerate() {
                    let on = i == room.cursor;
                    out.push((format!("   {o}"), on));
                }
            } else {
                let mine = v.ballots[seat].map_or("", |c| v.options[usize::from(c)].as_str());
                out.push((format!("   You voted {mine}"), false));
            }
            let theirs = if v.ballots[1 - seat].is_some() {
                "voted"
            } else {
                "voting..."
            };
            out.push((format!("   {them}: {theirs}"), false));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// KHC-016: both players reckon the same seed from the two numbers, and a number that is not the committed one
    /// is refused.
    #[test]
    fn the_coin_is_both_players_and_a_wrong_reveal_is_refused() {
        let mut host = Coin::new(0, [1; 16]);
        let mut join = Coin::new(0, [2; 16]);
        assert_eq!(
            host.own_secret(),
            None,
            "no number before the other's commitment"
        );
        host.take_commit(0, join.own_commit());
        join.take_commit(0, host.own_commit());
        assert_eq!(host.seed(true), None);
        host.take_reveal(0, join.own_secret().unwrap()).unwrap();
        join.take_reveal(0, host.own_secret().unwrap()).unwrap();
        assert!(host.seed(true).is_some());
        assert_eq!(host.seed(true), join.seed(false), "the same coin on both");
        // Another number than the committed one, or one for another vote.
        let mut cheat = Coin::new(0, [1; 16]);
        cheat.take_commit(0, Coin::commitment(0, &[2; 16]));
        assert!(cheat.take_reveal(0, [3; 16]).is_err());
        assert!(cheat.take_reveal(1, [2; 16]).is_err());
        assert_eq!(cheat.seed(true), None);
        let mut early = Coin::new(0, [1; 16]);
        assert!(
            early.take_reveal(0, [2; 16]).is_err(),
            "before its commitment"
        );
        // Each number of the joiner's lands a different seed: the host alone does not choose it.
        let seeds: std::collections::BTreeSet<u32> = (0..8u8)
            .map(|n| {
                let mut h = Coin::new(0, [1; 16]);
                h.take_commit(0, Coin::commitment(0, &[n; 16]));
                h.take_reveal(0, [n; 16]).unwrap();
                h.seed(true).unwrap()
            })
            .collect();
        assert_eq!(seeds.len(), 8);
        assert_ne!(Coin::random_secret(), Coin::random_secret());
    }

    #[test]
    fn the_same_choice_wins_and_a_tie_flips_the_coin() {
        let mut v = Vote::new(TOPIC_TEST, "Arena?", &["Room", "Map"]);
        assert!(v.decide(7).is_none(), "nobody has voted");
        v.cast(0, 1).unwrap();
        assert!(v.cast(0, 0).is_err(), "a ballot stands");
        assert!(v.cast(1, 5).is_err());
        v.cast(1, 1).unwrap();
        assert_eq!(
            v.decide(7),
            Some(Outcome {
                choice: 1,
                flip: None
            })
        );
        let mut tie = Vote::new(TOPIC_TEST, "Arena?", &["Room", "Map"]);
        tie.cast(0, 0).unwrap();
        tie.cast(1, 1).unwrap();
        assert_eq!(
            tie.decide(2),
            Some(Outcome {
                choice: 0,
                flip: Some(2)
            })
        );
        assert_eq!(
            tie.decide(3),
            Some(Outcome {
                choice: 0,
                flip: Some(2)
            }),
            "decided once"
        );
        assert!(tie.cast(1, 0).is_err());
        assert_eq!(flip_result(3, 0, 1), 1);
    }

    #[test]
    fn the_coin_turns_over_and_lands_on_the_result() {
        let (seed, a, b) = (5, 0, 1);
        let land = flip_result(seed, a, b);
        assert_eq!(flip_face(seed, a, b, FLIP_TICKS), land);
        assert_eq!(
            flip_face(seed, a, b, FLIP_TICKS - 1),
            land,
            "the last turn shows it"
        );
        let faces: Vec<u8> = (0..FLIP_TICKS)
            .step_by(TURN_TICKS as usize)
            .map(|t| flip_face(seed, a, b, t))
            .collect();
        assert!(
            faces.windows(2).all(|w| w[0] != w[1]),
            "each turn shows the other side"
        );
    }

    #[test]
    fn votes_round_trip() {
        let mut v = Vote::new(3, "Party?", &["On", "Off", "Host decides"]);
        v.cast(0, 2).unwrap();
        assert_eq!(decode_vote(&encode_vote(&v)), Some(v.clone()));
        v.cast(1, 0).unwrap();
        v.decide(9);
        let bytes = encode_vote(&v);
        assert_eq!(decode_vote(&bytes), Some(v));
        assert_eq!(decode_vote(&bytes[..bytes.len() - 1]), None);
    }
}
