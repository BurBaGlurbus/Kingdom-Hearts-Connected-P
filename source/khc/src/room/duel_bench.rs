//! [Test, KHC-015] `--bench-matrix N`: a one-process duel (the room's Sora and the scripted rival, each on his
//! seat's ruleset) played as N rounds to a knock-out. Each round's row goes to stdout, to the log (`bench_round`)
//! and as CSV to `--bench-out` (else `%LOCALAPPDATA%\KHC\bench\`): the winner, the round's length, and each seat's
//! damage and hits a second and best damage in any 5 s. A knock-out ends the round; both Soras stand up full where
//! the arena puts them 2 s later, and the run ends after the last round.

use super::*;

/// The rows' columns.
pub const HEADER: &str = "pair,round,winner,seconds,dps0,dps1,hits0,hits1,burst0,burst1";
/// The wait after a knock-out before the next round, in ticks.
const BETWEEN: u64 = 120;
/// The burst window, in ticks (5 s).
const BURST: usize = 300;

/// The bench's state.
#[derive(Default)]
pub struct Bench {
    /// Rounds to play.
    pub rounds: u32,
    /// Where the rows go (else `%LOCALAPPDATA%\KHC\bench\`).
    pub out: Option<PathBuf>,
    /// `--bench-idle`: the rival stands (and a KH3 room Sora repeats his script's acting half), so a round is the
    /// room Sora's time to kill.
    pub idle: bool,
    played: u32,
    /// The round's first tick and each seat's tally then.
    start: Option<(u64, [u32; 2], [u16; 2])>,
    /// Each seat's damage, tick by tick, this round.
    dealt: Vec<[u32; 2]>,
    last: [u32; 2],
    /// A knock-out's tick, and the row it made.
    ko: Option<u64>,
}

/// The best sum of `window` consecutive ticks of each seat's damage.
pub fn burst(dealt: &[[u32; 2]], window: usize) -> [u32; 2] {
    let mut best = [0; 2];
    for seat in 0..2 {
        let mut sum = 0u32;
        for (i, d) in dealt.iter().enumerate() {
            sum += d[seat];
            if i >= window {
                sum -= dealt[i - window][seat];
            }
            best[seat] = best[seat].max(sum);
        }
    }
    best
}

/// One round's row.
#[allow(clippy::too_many_arguments)]
pub fn row(
    pair: &str,
    round: u32,
    winner: Option<u8>,
    ticks: u64,
    dealt: [u32; 2],
    landed: [u16; 2],
    burst: [u32; 2],
) -> String {
    let s = (ticks as f32 / 60.).max(1. / 60.);
    format!(
        "{pair},{round},{},{s:.2},{:.3},{:.3},{:.3},{:.3},{},{}",
        winner.map_or(-1, i32::from),
        dealt[0] as f32 / s,
        dealt[1] as f32 / s,
        f32::from(landed[0]) / s,
        f32::from(landed[1]) / s,
        burst[0],
        burst[1],
    )
}

/// The rulesets by seat, as `KH2-KH3`.
fn pair(game: &Game) -> String {
    let r = &game.ext.online().duel_rules;
    let name = |kh3: bool| if kh3 { "KH3" } else { "KH2" };
    format!("{}-{}", name(r.seat_kh3(0)), name(r.seat_kh3(1)))
}

/// Each seat's HP, by slot.
fn hps(game: &Game) -> [i32; 2] {
    let mut hp = [1; 2];
    if let Some(h) = hp.get_mut(usize::from(game.local_slot)) {
        *h = game.sora.vitals.hp;
    }
    for r in &game.rivals {
        if let Some(h) = hp.get_mut(usize::from(r.slot)) {
            *h = r.fighter.vitals.hp;
        }
    }
    hp
}

/// Runs on the fixed tick after the hits.
pub fn tick(mut game: ResMut<Game>, mut exits: MessageWriter<AppExit>) {
    let game = &mut *game;
    let Some(mut bench) = game.ext.online_mut().bench.take() else {
        return;
    };
    step(game, &mut bench, &mut exits);
    game.ext.online_mut().bench = Some(bench);
}

fn step(game: &mut Game, bench: &mut Bench, exits: &mut MessageWriter<AppExit>) {
    if game.rivals.is_empty() {
        return;
    }
    let tick = game.tick;
    let (dealt, landed) = {
        let o = game.ext.online();
        (o.duel_dealt, o.duel_landed)
    };
    if let Some(ko) = bench.ko {
        if tick < ko + BETWEEN {
            return;
        }
        bench.ko = None;
        cleanup::each_sora(game, duel_match_room::refill);
        duel_arena::place(game);
        bench.start = None;
        if bench.played >= bench.rounds {
            game.event(serde_json::json!({"event":"bench_done","tick":tick,"rounds":bench.played}));
            exits.write(AppExit::Success);
        }
        return;
    }
    if bench.played >= bench.rounds {
        return;
    }
    let Some((start, dealt0, landed0)) = bench.start else {
        bench.start = Some((tick, dealt, landed));
        bench.dealt.clear();
        bench.last = dealt;
        return;
    };
    bench.dealt.push([
        dealt[0].saturating_sub(bench.last[0]),
        dealt[1].saturating_sub(bench.last[1]),
    ]);
    bench.last = dealt;
    let hp = hps(game);
    if hp.iter().all(|&h| h > 0) {
        return;
    }
    let winner = match hp {
        [a, b] if a > 0 && b <= 0 => Some(0),
        [a, b] if b > 0 && a <= 0 => Some(1),
        _ => None,
    };
    bench.played += 1;
    let line = row(
        &pair(game),
        bench.played,
        winner,
        tick - start,
        [dealt[0] - dealt0[0], dealt[1] - dealt0[1]],
        [
            landed[0].wrapping_sub(landed0[0]),
            landed[1].wrapping_sub(landed0[1]),
        ],
        burst(&bench.dealt, BURST),
    );
    println!("bench_round,{line}");
    game.event(serde_json::json!({"event":"bench_round","tick":tick,"row":line}));
    write_row(bench, &line);
    bench.ko = Some(tick);
}

fn write_row(bench: &Bench, line: &str) {
    let path = bench.out.clone().or_else(|| {
        std::env::var_os("LOCALAPPDATA").map(|d| {
            PathBuf::from(d)
                .join("KHC")
                .join("bench")
                .join("matrix.csv")
        })
    });
    let Some(path) = path else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let new = !path.exists();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        if new {
            let _ = writeln!(f, "{HEADER}");
        }
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_burst_is_the_best_window() {
        let dealt = [[1, 0], [5, 2], [0, 2], [5, 0], [1, 9]];
        assert_eq!(burst(&dealt, 2), [6, 9]);
        assert_eq!(burst(&dealt, 10), [12, 13]);
        assert_eq!(burst(&[], 3), [0, 0]);
    }

    #[test]
    fn a_row_has_every_column() {
        let r = row("KH2-KH3", 3, Some(1), 600, [100, 170], [20, 31], [40, 60]);
        assert_eq!(r, "KH2-KH3,3,1,10.00,10.000,17.000,2.000,3.100,40,60");
        assert_eq!(r.split(',').count(), HEADER.split(',').count());
        assert!(row("KH2-KH2", 1, None, 0, [0; 2], [0; 2], [0; 2]).contains(",-1,"));
    }
}
