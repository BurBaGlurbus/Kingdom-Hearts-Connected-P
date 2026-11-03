//! When the joiner shows each snapshot (KHC-026). A fixed hold of two snapshots
//! that skips ahead at eight works on a LAN; over the internet jitter stalls it and
//! drift between the two PCs' clocks walks it into the skip. Here a playhead in host
//! ticks follows an estimate of the host's clock, a little behind it: far enough to
//! cover this link's jitter, measured over two seconds, and never by more than 5%
//! faster or slower than real time. It knows nothing of the room: arrivals go in and
//! a playhead comes out.
//!
//! All times are in the joiner's own fixed ticks.

use std::collections::VecDeque;

/// How far back the link is measured: two seconds.
const WINDOW_TICKS: u64 = 120;
/// The hold, in snapshots: at least enough to have one to glide to, at most
/// 133 ms at 60 a second [duel choice, KHC-026: the brief's target].
const MIN_SNAPSHOTS: f64 = 2.;
const MAX_SNAPSHOTS: f64 = 8.;
/// The hold changes by one snapshot at most this often: a second.
const CHANGE_EVERY: u64 = 60;
/// The most the playhead's speed is off real time, and how much a tick of error
/// asks for: a tick behind is the whole of it.
pub const MAX_SCALE: f64 = 0.05;
const SCALE_PER_TICK: f64 = 0.05;
/// Arrival times come in whole ticks, so less than this is no error.
const DEAD_BAND: f64 = 0.25;
/// The playhead goes this far past the newest snapshot, then waits.
pub const MAX_AHEAD: f64 = 6.;
/// More behind than this (half a second) is no longer worth catching up on at 5%:
/// a stall on this PC, say. The playhead jumps, as the fixed hold did.
const JUMP_BEYOND: f64 = 30.;
/// The host's tick runs at twice the rate while its menu is up (the lobby, the
/// result): over this many of ours, faster than `FAST_CLOCK` means that. Nothing moves
/// then, so nothing is held back.
const PACE_TICKS: u64 = 30;
const FAST_CLOCK: f64 = 1.5;

/// One tick's result.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Advance {
    /// The host tick on screen, between two snapshots.
    pub play: f64,
    /// The playhead jumped this many ticks ahead this tick.
    pub jumped: Option<u64>,
    /// Past `MAX_AHEAD`: nothing new to show and no more to guess.
    pub frozen: bool,
}

#[derive(Clone, Debug)]
pub struct Playout {
    now: u64,
    /// (arrival, host tick), oldest first.
    arrivals: VecDeque<(u64, u64)>,
    newest: Option<u64>,
    /// Ticks between snapshots, as they come.
    stride: f64,
    /// The hold now in force, in ticks, and when it may next change.
    hold: f64,
    next_change: u64,
    play: Option<f64>,
    /// The speed last used, for the overlay.
    speed: f64,
}

impl Default for Playout {
    fn default() -> Self {
        Playout {
            now: 0,
            arrivals: VecDeque::new(),
            newest: None,
            stride: 1.,
            hold: MIN_SNAPSHOTS,
            next_change: 0,
            play: None,
            speed: 1.,
        }
    }
}

impl Playout {
    /// A snapshot arrived this tick.
    pub fn arrive(&mut self, host_tick: u64) {
        self.arrivals.push_back((self.now, host_tick));
        self.newest = Some(self.newest.map_or(host_tick, |n| n.max(host_tick)));
    }

    /// The hold in force: Brief 2's rewind and the overlay read it.
    pub fn hold_ticks(&self) -> f64 {
        self.hold
    }

    /// How far off real time the playhead runs now: 1 is real time.
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Lateness of each arrival against the earliest of the window: (earliest
    /// arrival minus host tick, the 95th percentile's lateness). A lost snapshot
    /// widens nothing, as it would with the gaps between arrivals, and drift over
    /// two seconds is well under a tick [duel choice, KHC-026].
    fn measure(&self) -> Option<(i64, f64)> {
        let mut late: Vec<i64> = self
            .arrivals
            .iter()
            .map(|&(at, host)| at as i64 - host as i64)
            .collect();
        late.sort_unstable();
        let earliest = *late.first()?;
        let at = (late.len() - 1) * 95 / 100;
        Some((earliest, (late[at] - earliest) as f64))
    }

    /// One tick of the joiner: the playhead's new place, or None before the first
    /// snapshot.
    pub fn advance(&mut self) -> Option<Advance> {
        self.now += 1;
        while self
            .arrivals
            .front()
            .is_some_and(|(at, _)| at + WINDOW_TICKS < self.now)
        {
            self.arrivals.pop_front();
        }
        let newest = self.newest? as f64;
        if self.host_clock_is_fast() {
            let play = (newest - MIN_SNAPSHOTS).max(self.play.unwrap_or(0.));
            self.play = Some(play);
            self.speed = 1.;
            return Some(Advance {
                play,
                ..Advance::default()
            });
        }
        self.note_stride();
        let measured = self.measure();
        if let Some((_, spread)) = measured {
            self.change_hold(spread);
        }
        // A snapshot of the host's clock that took the shortest way would be
        // arriving now. Silence leaves the last estimate running.
        let wanted = match measured {
            Some((earliest, _)) => (self.now as i64 - earliest) as f64 - self.hold,
            None => self.play.unwrap_or(newest),
        };
        let Some(play) = self.play else {
            let play = wanted.min(newest);
            self.play = Some(play);
            return Some(Advance {
                play,
                ..Advance::default()
            });
        };
        // Where a tick at real time would put it, against where it should be.
        let behind = wanted - (play + 1.);
        if behind > JUMP_BEYOND {
            // What was measured before a stall says nothing of after it.
            self.arrivals.clear();
            self.play = Some(wanted);
            self.speed = 1.;
            return Some(Advance {
                play: wanted,
                jumped: Some(behind as u64),
                frozen: false,
            });
        }
        self.speed = if behind.abs() < DEAD_BAND {
            1.
        } else {
            1. + (behind * SCALE_PER_TICK).clamp(-MAX_SCALE, MAX_SCALE)
        };
        let limit = newest + MAX_AHEAD;
        let next = play + self.speed;
        let frozen = next > limit;
        // Never backward: a snapshot already shown stays shown.
        let play = if frozen { play.max(limit) } else { next };
        self.play = Some(play);
        Some(Advance {
            play,
            jumped: None,
            frozen,
        })
    }

    /// Whether the host's clock runs far faster than ours. Only the last moments
    /// are kept while it does.
    fn host_clock_is_fast(&mut self) -> bool {
        let recent = self
            .arrivals
            .iter()
            .rev()
            .take_while(|(at, _)| at + PACE_TICKS >= self.now);
        let (mut first, mut last) = (None, None);
        let mut hosts = Vec::new();
        for &(at, host) in recent {
            hosts.push(host);
            last = last.or(Some((at, host)));
            first = Some((at, host));
        }
        // A burst after a stall comes a tick apart in host ticks: not this. Sorted,
        // as jitter can bring them in any order.
        hosts.sort_unstable();
        hosts.dedup();
        let mut steps: Vec<u64> = hosts.windows(2).map(|p| p[1] - p[0]).collect();
        steps.sort_unstable();
        let usual = steps.get(steps.len() / 2).copied().unwrap_or(1);
        let (Some(first), Some(last)) = (first, last) else {
            return false;
        };
        if last.0 < first.0 + PACE_TICKS / 2 {
            return false;
        }
        let pace = last.1.abs_diff(first.1) as f64 / (last.0 - first.0) as f64;
        let fast = pace > FAST_CLOCK && usual >= 2;
        if fast {
            let keep_from = self.now.saturating_sub(PACE_TICKS);
            self.arrivals.retain(|(at, _)| *at >= keep_from);
        }
        fast
    }

    /// The ticks between snapshots: the shortest step the window holds.
    fn note_stride(&mut self) {
        let settled = self
            .arrivals
            .front()
            .is_some_and(|(at, _)| at + PACE_TICKS / 2 <= self.now);
        if !settled {
            return;
        }
        let mut hosts: Vec<u64> = self.arrivals.iter().map(|a| a.1).collect();
        hosts.sort_unstable();
        hosts.dedup();
        if let Some(step) = hosts.windows(2).map(|p| p[1] - p[0]).min() {
            self.stride = step as f64;
        }
    }

    fn change_hold(&mut self, spread: f64) {
        // Past the spread by one, for the tick an arrival time is rounded by.
        let wanted = (spread + 1.).clamp(MIN_SNAPSHOTS * self.stride, MAX_SNAPSHOTS * self.stride);
        if self.now < self.next_change || (wanted - self.hold).abs() < 0.5 {
            return;
        }
        self.hold += (wanted - self.hold).clamp(-self.stride, self.stride);
        self.next_change = self.now + CHANGE_EVERY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xorshift64*: a uniform value in 0..1.
    struct Rng(u64);

    impl Rng {
        fn unit(&mut self) -> f64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[derive(Default)]
    struct Run {
        jumps: usize,
        frozen: usize,
        longest_frozen: usize,
        /// Host ticks already past when their snapshot arrived.
        late: usize,
        arrived: usize,
        /// (tick, hold).
        holds: Vec<(u64, f64)>,
        speeds: Vec<f64>,
        last_play: f64,
    }

    /// Plays `ticks` joiner ticks. `clock` is the joiner's ticks per host tick, and
    /// `delay` the transit in host ticks of a snapshot, or None when it is lost.
    fn run(ticks: u64, clock: f64, mut delay: impl FnMut(u64) -> Option<f64>) -> Run {
        let mut p = Playout::default();
        let mut queue: Vec<(u64, u64)> = Vec::new();
        let mut out = Run::default();
        let mut host = 0u64;
        let mut frozen_run = 0;
        let mut play = None;
        for now in 0..ticks {
            // Sent up to this joiner tick.
            while host as f64 * clock <= now as f64 {
                if let Some(d) = delay(host) {
                    let at = ((host as f64 + d) * clock).floor() as u64;
                    queue.push((at, host));
                }
                host += 1;
            }
            queue.sort_unstable();
            while queue.first().is_some_and(|q| q.0 <= now) {
                let (_, tick) = queue.remove(0);
                out.arrived += 1;
                if play.is_some_and(|at: f64| tick as f64 <= at.floor()) {
                    out.late += 1;
                }
                p.arrive(tick);
            }
            let Some(a) = p.advance() else { continue };
            if let Some(before) = play {
                assert!(a.play >= before, "backward at {now}");
            }
            play = Some(a.play);
            out.last_play = a.play;
            out.jumps += usize::from(a.jumped.is_some());
            if a.frozen {
                out.frozen += 1;
                frozen_run += 1;
                out.longest_frozen = out.longest_frozen.max(frozen_run);
            } else {
                frozen_run = 0;
            }
            assert!(
                (p.speed() - 1.).abs() <= MAX_SCALE + 1e-9,
                "{} at {now}",
                p.speed()
            );
            out.speeds.push(p.speed());
            if out.holds.last().is_none_or(|h| h.1 != p.hold_ticks()) {
                out.holds.push((now, p.hold_ticks()));
            }
        }
        out
    }

    fn hold_changes_stay_slow(r: &Run) {
        for pair in r.holds.windows(2).skip(1) {
            assert!(
                pair[1].0 - pair[0].0 >= CHANGE_EVERY,
                "changed twice in a second: {pair:?}"
            );
            assert!((pair[1].1 - pair[0].1).abs() <= 1., "{pair:?}");
        }
        for (_, hold) in &r.holds {
            assert!((MIN_SNAPSHOTS..=MAX_SNAPSHOTS).contains(hold), "{hold}");
        }
    }

    #[test]
    fn a_steady_link_holds_two_snapshots_at_real_time() {
        let r = run(36_000, 1., |_| Some(4.));
        assert_eq!(r.jumps, 0);
        assert_eq!(r.frozen, 0);
        assert_eq!(r.late, 0);
        assert_eq!(r.holds.last().map(|h| h.1), Some(MIN_SNAPSHOTS));
        // Once it has settled nothing is scaled.
        assert!(r.speeds[600..].iter().all(|&s| s == 1.));
    }

    #[test]
    fn jitter_grows_the_hold_to_cover_it_with_no_stalls() {
        let mut rng = Rng(77);
        // 25 ms of spread either way: three ticks.
        let r = run(36_000, 1., |_| Some(4. + rng.unit() * 6.));
        assert_eq!(r.jumps, 0);
        assert_eq!(r.frozen, 0);
        hold_changes_stay_slow(&r);
        let hold = r.holds.last().unwrap().1;
        assert!((5. ..=8.).contains(&hold), "{hold}");
        // The 95th percentile is what it covers.
        assert!(
            (r.late as f64) < r.arrived as f64 * 0.05,
            "{} of {}",
            r.late,
            r.arrived
        );
    }

    #[test]
    fn the_hold_stays_within_its_limits_on_a_terrible_link() {
        let mut rng = Rng(5);
        let r = run(18_000, 1., |_| Some(rng.unit() * 40.));
        hold_changes_stay_slow(&r);
        assert_eq!(r.holds.last().unwrap().1, MAX_SNAPSHOTS);
        assert_eq!(r.jumps, 0);
    }

    #[test]
    fn a_burst_of_loss_guesses_six_ticks_and_then_waits() {
        // A fifth of a second lost every five seconds.
        let r = run(36_000, 1., |host| (host % 300 >= 12).then_some(4.));
        assert_eq!(r.jumps, 0);
        assert!(r.frozen > 0, "the gap outlasts the guess");
        // The gap of 12, less the hold of 2 and the guess of 6, and the speed that
        // gets it back: no freeze near a tenth of a second.
        assert!(r.longest_frozen <= 5, "{}", r.longest_frozen);
    }

    #[test]
    fn a_short_burst_of_loss_never_shows() {
        let r = run(36_000, 1., |host| (host % 300 >= 5).then_some(4.));
        assert_eq!((r.jumps, r.frozen), (0, 0));
    }

    #[test]
    fn a_tenth_of_a_percent_of_clock_drift_never_jumps_or_stalls() {
        for clock in [0.999, 1.001] {
            let r = run(216_000, clock, |_| Some(4.));
            assert_eq!(r.jumps, 0, "{clock}");
            assert_eq!(r.frozen, 0, "{clock}");
            assert_eq!(r.late, 0, "{clock}");
        }
    }

    #[test]
    fn a_stall_on_this_pc_jumps_once() {
        let mut p = Playout::default();
        let mut host = 0;
        for _ in 0..300 {
            p.arrive(host);
            host += 1;
            assert_eq!(p.advance().unwrap().jumped, None);
        }
        // Two seconds of nothing here, then all of it at once.
        for _ in 0..120 {
            p.arrive(host);
            host += 1;
        }
        let a = p.advance().unwrap();
        assert!(a.jumped.is_some(), "{a:?}");
        assert!((host as f64 - a.play) < 5., "{a:?}");
    }

    #[test]
    fn a_host_s_menu_doubling_its_clock_is_no_reason_to_jump() {
        let mut p = Playout::default();
        let mut host = 1000;
        let mut jumps = 0;
        for _ in 0..600 {
            p.arrive(host);
            host += 2;
            jumps += usize::from(p.advance().unwrap().jumped.is_some());
        }
        assert!(jumps <= 1, "{jumps}");
        assert!((host as f64 - p.play.unwrap()) < 8., "{host} {:?}", p.play);
        // Before the first half second has shown it for what it is, one step at most.
        assert!(p.hold_ticks() <= MIN_SNAPSHOTS + 1., "{}", p.hold_ticks());
        // The fight starts: back to one a tick, and the hold is two again.
        for _ in 0..900 {
            p.arrive(host);
            host += 1;
            assert_eq!(p.advance().unwrap().jumped, None);
        }
        assert_eq!(p.hold_ticks(), MIN_SNAPSHOTS);
    }

    #[test]
    fn half_the_snapshots_hold_for_as_many_snapshots() {
        let r = run(18_000, 1., |host| host.is_multiple_of(2).then_some(4.));
        assert_eq!((r.jumps, r.frozen, r.late), (0, 0, 0));
        // Two snapshots at 30 a second: about 67 ms.
        assert_eq!(r.holds.last().unwrap().1, 2. * MIN_SNAPSHOTS);
    }
}
