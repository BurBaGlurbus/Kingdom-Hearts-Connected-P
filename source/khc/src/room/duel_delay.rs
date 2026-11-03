//! Equal input delay (the `latency_match` rule): the host's own buttons wait
//! about as long as the joiner's take to arrive, so neither player acts first
//! [duel choice, KHC-032]. The wait is half the smoothed round trip in ticks,
//! and it changes only while both Soras are held (the lobby, a countdown, a
//! round's end), never in a fight.

use super::rival::{Command, RivalPad};
use super::*;
use std::collections::VecDeque;

/// The longest wait, in ticks: 100 ms.
pub const MAX_DELAY_TICKS: u8 = 6;
/// One tick, in milliseconds.
const TICK_MS: f32 = 1000. / 60.;
/// About two seconds of ticks.
const SMOOTHING_TICKS: f32 = 120.;

/// The wait for a round trip: half of it, in whole ticks.
pub fn delay_ticks(rtt_ms: f32) -> u8 {
    if !rtt_ms.is_finite() {
        return 0;
    }
    (rtt_ms / 2. / TICK_MS)
        .round()
        .clamp(0., f32::from(MAX_DELAY_TICKS)) as u8
}

/// The round trip smoothed over about two seconds, with the ticks the joiner's
/// buttons wait in the host's queue on top: half the round trip alone left the
/// joiner up to 2.7 ticks behind at 80 ms [duel choice, KHC-032]. A queue tick
/// counts as two of round trip, which is halved.
#[derive(Clone, Copy, Debug, Default)]
pub struct DelayMeter {
    smoothed: Option<f32>,
}

impl DelayMeter {
    /// One tick's reading.
    pub fn update(&mut self, rtt_ms: f32, queued: usize) {
        if !rtt_ms.is_finite() || rtt_ms < 0. {
            return;
        }
        let reading = rtt_ms + 2. * TICK_MS * queued as f32;
        self.smoothed = Some(match self.smoothed {
            Some(s) => s + (reading - s) / SMOOTHING_TICKS,
            None => reading,
        });
    }

    pub fn ticks(&self) -> u8 {
        self.smoothed.map_or(0, delay_ticks)
    }
}

/// Buttons on their way to the Sora: what goes in comes out exactly `len`
/// ticks later.
#[derive(Clone, Debug, Default)]
pub struct PadFifo {
    waiting: VecDeque<RivalPad>,
}

impl PadFifo {
    /// This tick's buttons in, the ones due out. Until `len` ticks have gone
    /// in nothing is pressed. A shorter `len` drops the oldest, which is why
    /// it only changes while the Soras are held.
    pub fn push(&mut self, pad: RivalPad, len: usize) -> RivalPad {
        self.waiting.push_back(pad);
        let mut out = RivalPad::default();
        while self.waiting.len() > len {
            if let Some(p) = self.waiting.pop_front() {
                out = p;
            }
        }
        out
    }

    /// What has gone in and not come out, oldest first.
    pub fn waiting(&self) -> impl Iterator<Item = &RivalPad> {
        self.waiting.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }
}

/// The host's own Sora's buttons on their way to him.
#[derive(Clone, Debug, Default)]
pub struct OwnDelay {
    /// The wait now in force, in ticks.
    pub ticks: u8,
    pub fifo: PadFifo,
}

/// Start of a host's fixed step: the wait follows the link while the Soras are
/// held. Its reading stays with the session for the overlay and the lobby.
pub fn update(net: &mut duel_net::DuelNet, game: &mut Game) {
    if !net.session.is_connected() {
        net.host_delay_ticks = 0;
        return;
    }
    let queued = net.inputs.len();
    net.delay_meter
        .update(net.session.stats().rtt_ms as f32, queued);
    let online = game.ext.online();
    // KH3's seats are not predicted (duel_predict.rs): a KH3 host would feel
    // every tick of the wait, so his buttons do not wait [duel choice, KHC-032].
    let rules = online.duel_rules;
    let wanted = if rules.latency_match && !rules.seat_kh3(0) && !online.script.locked {
        net.delay_meter.ticks()
    } else {
        0
    };
    net.host_delay_ticks = wanted;
    if net.role != net::Role::Host || !online.duel_frozen || online.own_delay.ticks == wanted {
        return;
    }
    game.ext.online_mut().own_delay.ticks = wanted;
    let tick = game.tick;
    game.event(serde_json::json!({"event":"duel_host_delay","tick":tick,"ticks":wanted}));
}

/// The room Sora's buttons through the wait: `None` leaves them as they are
/// (no wait, the single path a LAN duel takes).
pub fn own_pad(game: &mut Game, mut pad: RivalPad, _frozen: bool) -> Option<RivalPad> {
    if game.replica {
        return None;
    }
    let delay = &game.ext.online().own_delay;
    if delay.ticks == 0 && delay.fifo.is_empty() {
        return None;
    }
    // A form asked for from the menu waits with the rest.
    if pad.command == Command::None
        && let Some(form) = game.drive.as_mut().and_then(|d| d.request.take())
    {
        pad.command = Command::Drive(form);
    }
    let delay = &mut game.ext.online_mut().own_delay;
    let len = usize::from(delay.ticks);
    Some(delay.fifo.push(pad, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(n: u32) -> RivalPad {
        RivalPad {
            direction: [n as f32 / 100., 0.],
            attack: true,
            ..RivalPad::default()
        }
    }

    #[test]
    fn the_wait_is_half_the_round_trip_in_ticks() {
        assert_eq!(delay_ticks(0.), 0);
        assert_eq!(delay_ticks(5.), 0, "a LAN rounds to no wait");
        assert_eq!(delay_ticks(40.), 1);
        assert_eq!(delay_ticks(80.), 2);
        assert_eq!(delay_ticks(100.), 3);
        assert_eq!(delay_ticks(150.), 5);
        assert_eq!(delay_ticks(230.), 6, "no more than 100 ms");
        assert_eq!(delay_ticks(f32::INFINITY), 0);
        assert_eq!(delay_ticks(f32::NAN), 0);
    }

    #[test]
    fn the_meter_smooths_over_about_two_seconds() {
        let mut m = DelayMeter::default();
        assert_eq!(m.ticks(), 0);
        m.update(80., 0);
        assert_eq!(m.ticks(), 2, "the first reading counts whole");
        // A spike of a tick moves it hardly at all.
        m.update(200., 0);
        assert_eq!(m.ticks(), 2);
        // Two seconds at the new round trip are most of the way there.
        for _ in 0..240 {
            m.update(200., 0);
        }
        assert!(m.ticks() >= 5, "{}", m.ticks());
        m.update(f32::NAN, 0);
        assert!(m.ticks() >= 5);
        // The buttons' wait in the host's queue counts tick for tick.
        let mut q = DelayMeter::default();
        q.update(80., 2);
        assert_eq!(q.ticks(), 4);
    }

    #[test]
    fn the_fifo_gives_each_press_back_exactly_n_ticks_later() {
        for n in 0..=usize::from(MAX_DELAY_TICKS) {
            let mut f = PadFifo::default();
            for t in 0..40u32 {
                let out = f.push(press(t), n);
                if (t as usize) < n {
                    assert_eq!(out, RivalPad::default(), "nothing yet at {t}, wait {n}");
                } else {
                    assert_eq!(out, press(t - n as u32), "tick {t}, wait {n}");
                }
                assert_eq!(f.waiting().count(), n.min(t as usize + 1));
            }
        }
    }

    #[test]
    fn a_shorter_wait_drops_the_oldest() {
        let mut f = PadFifo::default();
        for t in 0..4 {
            f.push(press(t), 4);
        }
        assert_eq!(f.push(press(4), 1), press(3));
        assert_eq!(f.waiting().count(), 1);
    }
}
