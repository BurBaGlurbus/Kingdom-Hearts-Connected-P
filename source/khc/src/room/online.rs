//! Multiplayer and the KH3 ruleset on the room: their state, their options,
//! their hooks and their place in the schedule.

use super::extension::{
    CameraFrame, DebugOutcome, DebugRow, Extension, ExtraButtons, RoomExtension, RoomHooks,
    RoomSet, SoraDrive,
};
use super::*;
use std::error::Error;

/// What the room keeps for multiplayer, in `Game::ext`.
pub struct Online {
    /// `--rival-script`: a scripted rival from the first tick.
    pub rival_script: bool,
    /// [Test] `--duel-bench`: both duel Soras close in and swing inside each
    /// combo window (the combo measurement), the joiner's through its
    /// buttons, the host's own Sora through its input.
    pub duel_bench: bool,
    /// [Test] `--duel-magic`: both duel Soras cast every spell in turn at
    /// the other.
    pub duel_magic: bool,
    /// [Test] `--duel-idle`: this player's Sora stands still (the other
    /// side of a bench).
    pub duel_idle: bool,
    /// [Test] `--bench-matrix N`: rounds to a knock-out, measured (duel_bench.rs).
    pub bench: Option<duel_bench::Bench>,
    /// The duel's arena (a row of `duel_arena::ARENAS`).
    pub duel_arena: usize,
    /// The duel's rules (`--duel-damage`, F3 on the host; the host's reach
    /// the joiner).
    pub duel_rules: duel_rules::MatchRules,
    pub net_pad: rival::RivalPad,
    /// What the host's tick did that the joiner shows itself: sounds,
    /// sparks, knock-outs (sent and cleared by duel_sync).
    pub duel_events: Vec<duel_wire::DuelEvent>,
    /// The network overlay (F3 > Sora > Display, `--net-overlay`) and the
    /// link simulator preset the debug menu chose (duel_net::SIMULATED).
    pub net_overlay: bool,
    pub net_simulated: Option<usize>,
    /// The duel's lobby (or countdown) holds both Soras still.
    pub duel_frozen: bool,
    /// F3 asked the host for a test vote.
    pub duel_test_vote: bool,
    /// A vote is open: the other duel screens give it the camp.
    pub duel_vote_up: bool,
    /// Whether this install can show the duel's Mirage Arena, and a hash of the
    /// bytes read for it.
    pub mirage: mirage_arena::Avail,
    pub bbs_digest: Option<[u8; 32]>,
    /// [Test] `--duel-shots`: the duel's screens add captures of
    /// themselves, once per moment.
    pub duel_shots: bool,
    /// [Test] `--duel-forfeit`: this player forfeits two seconds into the
    /// first fight, through the menu.
    pub duel_forfeit: bool,
    /// [Test] `--duel-quick`: the unattended result picks Quick rematch.
    pub duel_quick: bool,
    /// [Test] `--duel-wisdom` (joiner): Wisdom Form now and then, and the
    /// bench's swings (its shots).
    pub duel_wisdom: bool,
    pub duel_shot_keys: std::collections::HashSet<(u8, u64, u64)>,
    /// [KH3 ruleset] The room collision's poles, kept by its triangle
    /// counts (duel_kh3.rs).
    pub kh3_poles: Option<duel_kh3::PoleCache>,
    /// KH3's Sora's world, with the poles' key; the hits the Soras landed this tick: (seat, hit, where and
    /// how he faced, in KH3's space).
    pub kh3_geometry: Option<duel_kh3::GeometryCache>,
    pub kh3_core_hits: Vec<duel_kh3::CoreHit>,
    /// [Test] `--kh3-preview`: the looks or elements shown one by one.
    pub kh3_preview: Option<duel_kh3::Preview>,
    /// [Test] `--duel-pick`: this player's own ruleset where each player picks.
    pub duel_pick: Option<bool>,
    /// [Test] `--duel-pick-flip`: this player changes that pick for each later match.
    pub duel_pick_flip: bool,
    /// [KHC-016] `--duel-code`: the host's rules and bans from a rules code, over the kept ones.
    pub duel_code: Option<(duel_rules::MatchRules, Vec<u16>)>,
    /// [KHC-030] `--duel-victory`: the row of duel_match_room::VICTORY_MOTIONS the winner plays.
    pub duel_victory: usize,
    /// The player's KH3 install, checked this session; the KH3 ruleset reads
    /// it and is not offered without it.
    pub kh3_install: Option<kh3_data::Install>,
    /// KH3's data for the simulation, once it is read (kh3_book.rs); a hash of
    /// every byte read for it; and that the reading failed.
    pub kh3_book: Option<Arc<kh3_player::attack::Book>>,
    pub kh3_digest: Option<[u8; 32]>,
    pub kh3_failed: bool,
    /// KH3's own deck and shotlock display, once read (kh3_deck.rs).
    pub kh3_hud: Option<Arc<kh3_ui::load::HudAssets>>,
    /// [KH3 mode] Effects and sounds from the host, by name (a joiner's).
    pub kh3_fx_remote: Vec<kh3_player::spell::FxRequest>,
    /// [KH3 mode] The player's KH3 files are being read: the duel waits.
    pub kh3_loading: bool,
    /// The launcher's Test KH3 mode (`--kh3-test` without
    /// `--rival-script`): the KH3 ruleset against the training dummy (the
    /// room's Shadow, standing), with the KH3 ability menu on START.
    pub kh3_dummy: bool,
    /// The dummy's hits already counted for focus (duel_kh3.rs).
    pub kh3_dummy_seen: u32,
    /// [KH3 ruleset] Test KH3 mode's ability menu is open (kh3_menu.rs).
    pub kh3_menu: Option<u32>,
    /// [KH3 ruleset] A joiner's command from the deck, for its next input (kh3_deck.rs).
    pub kh3_pending: Option<rival::Command>,
    /// [Test] `--kh3-deck-test`: Sora stands while the deck's script runs.
    pub kh3_deck_test: bool,
    /// A match is running: a knocked-out Sora stays down for the round.
    pub duel_match: bool,
    /// It has not been decided yet: a drop holds the seat (KHC-027).
    pub match_live: bool,
    /// Damage dealt and hits landed this match, by the attacker's slot.
    pub duel_dealt: [u32; 2],
    pub duel_landed: [u16; 2],
    /// The duel match's shots (the intro, the result), when no mission
    /// overlay holds the view.
    pub duel_camera: Option<boss::OverlayView>,
    /// [KH3 ruleset] The camera last shown, for the shotlock's reticle.
    pub kh3_aim_view: Option<kh3_sora::magic::View>,
    /// KH3's lock-on and aiming cameras (kh3_camera.rs).
    pub kh3_camera: kh3_camera::Kh3Camera,
    /// The census line's part.
    pub census: serde_json::Value,
    /// [Test] The scripted runs' clock (duel_script.rs).
    pub script: duel_script::ScriptClock,
    /// [Test] `--duel-seed`: the Drive rolls' start, instead of the clock.
    pub duel_seed: Option<u32>,
    /// Host: where the other player's Sora's sounds of this tick sit in the
    /// sound queue (duel_fx.rs).
    pub rival_sounds: (usize, usize),
    /// Host: the joiner's swings judged by what the joiner saw (duel_rewind.rs).
    pub rewind: duel_rewind::Rewind,
    /// [Test] `--duel-strafe`: the host's Sora steps side to side and guards.
    pub strafe: bool,
    /// This player's own Sora, predicted (duel_predict.rs).
    pub predict: duel_predict::Predict,
    /// The host's own buttons on their way to his Sora (duel_delay.rs).
    pub own_delay: duel_delay::OwnDelay,
    /// [Test] `--duel-press-test`: both Soras stand and jump now and then, to
    /// time a press (duel_script.rs).
    pub press_test: bool,
    /// [Test] `--duel-move-test`: a joiner's Sora only walks and jumps.
    pub move_test: bool,
    /// [Test] The action each Sora last showed, for that timing.
    pub press_seen: [Option<Action>; 2],
    /// [KHC-058] `--preload` is reading what would load on first use: Sora stands.
    pub preloading: bool,
}

/// What a Sora has beyond the room's, in `Fighter::ext`.
#[derive(Default)]
pub struct FighterOnline {
    /// [KH3 ruleset] The KH3 layer's state (duel_kh3.rs), and a grand
    /// magic's window: ticks left and the MP it keeps.
    pub kh3: kh3_player::Kh3,
    pub kh3_grand: Option<(u32, i32)>,
    /// KH2's max HP and MP, while KH3's Sora has put his own on the gauges.
    pub kh2_gauges: Option<(i32, i32)>,
}

/// The state in the room's extension slots.
pub trait Slot {
    fn online(&self) -> &Online;
    fn online_mut(&mut self) -> &mut Online;
    fn fighter(&self) -> &FighterOnline;
    fn fighter_mut(&mut self) -> &mut FighterOnline;
}

impl Slot for Extension {
    fn online(&self) -> &Online {
        self.downcast_ref().expect("the room runs with Online")
    }
    fn online_mut(&mut self) -> &mut Online {
        self.downcast_mut().expect("the room runs with Online")
    }
    fn fighter(&self) -> &FighterOnline {
        self.downcast_ref().expect("the Sora runs with Online")
    }
    fn fighter_mut(&mut self) -> &mut FighterOnline {
        self.downcast_mut().expect("the Sora runs with Online")
    }
}

/// The room's options for multiplayer, and what starting it leaves for the
/// app.
#[derive(Default)]
pub struct OnlineRoom {
    options: Option<Options>,
    loadout: duel_loadout::DuelLoadout,
    session: Option<duel_net::DuelNet>,
    unattended: bool,
    state_hash: Option<duel_hash::HashFile>,
    replay: Option<duel_replay::ReplayPlayer>,
    preload: bool,
}

struct Options {
    rival_script: bool,
    kh3_test: bool,
    kh3_deck_test: bool,
    kh3_folder: Option<PathBuf>,
    duel_bench: bool,
    duel_magic: bool,
    duel_shots: bool,
    duel_forfeit: bool,
    duel_quick: bool,
    duel_wisdom: bool,
    duel_no_mirage: bool,
    duel_idle: bool,
    bench: Option<duel_bench::Bench>,
    kh3_preview: Option<duel_kh3::Preview>,
    net_host: bool,
    net_join: Option<String>,
    net_port: u16,
    net_bind: std::net::Ipv4Addr,
    net_name: Option<String>,
    net_sim: net::link::Conditions,
    net_overlay: bool,
    snapshot_every: u32,
    net_cut: Option<(u64, std::time::Duration)>,
    relay: Option<std::net::SocketAddr>,
    room: Option<[u8; net::wire::CODE_LENGTH]>,
    relay_only: bool,
    /// [KHC-051] `--room` with no code (a host's ask for one), and the address
    /// in a direct code.
    room_asked: bool,
    direct_host: Option<std::net::SocketAddrV4>,
    duel_rules: duel_rules::MatchRules,
    duel_pick: Option<bool>,
    duel_pick_flip: bool,
    duel_code: Option<(duel_rules::MatchRules, Vec<u16>)>,
    duel_victory: usize,
    script: duel_script::ScriptClock,
    duel_seed: Option<u32>,
    net_log: Option<PathBuf>,
    state_hash: Option<PathBuf>,
    state_dump: Option<PathBuf>,
    replay_out: Option<PathBuf>,
    replay: Option<PathBuf>,
    press_test: bool,
    move_test: bool,
    predict: bool,
    no_rewind: bool,
    strafe: bool,
}

impl RoomExtension for OnlineRoom {
    fn hooks(&self) -> RoomHooks {
        HOOKS
    }

    fn window_title(&self) -> Option<String> {
        Some(format!(
            "Kingdom Hearts: Connected {}",
            env!("CARGO_PKG_VERSION")
        ))
    }

    fn take_options(&mut self, options: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        let mut rival_script = false;
        let mut kh3_test = false;
        let mut kh3_deck_test = false;
        let mut kh3_folder: Option<PathBuf> = None;
        let mut duel_bench = false;
        let mut duel_magic = false;
        let mut duel_shots = false;
        let mut duel_forfeit = false;
        let mut duel_quick = false;
        let mut duel_wisdom = false;
        let mut duel_no_mirage = false;
        let mut duel_idle = false;
        let mut bench: Option<duel_bench::Bench> = None;
        let mut kh3_preview: Option<duel_kh3::Preview> = None;
        // The LAN duel (duel_net.rs): host or join, the port, the name shown
        // to the other player, the link simulator and the overlay.
        let mut net_host = false;
        let mut net_join: Option<String> = None;
        let mut net_port = net::DEFAULT_PORT;
        let mut net_bind = std::net::Ipv4Addr::UNSPECIFIED;
        let mut net_name: Option<String> = None;
        let mut net_sim = net::link::Conditions::default();
        let mut net_overlay = false;
        let mut snapshot_every = 1;
        let mut net_cut = None;
        let mut relay = None;
        let mut room = None;
        let mut relay_only = false;
        let mut room_asked = false;
        let mut direct_host = None;
        let mut duel_rules = duel_rules::MatchRules::default();
        let mut duel_pick = None;
        let mut duel_pick_flip = false;
        let mut duel_code = None;
        let mut duel_victory = 0;
        let mut script = duel_script::ScriptClock::default();
        let mut duel_seed = None;
        let mut net_log = None;
        let mut state_hash = None;
        let mut state_dump = None;
        let mut replay_out = None;
        let mut replay = None;
        let mut press_test = false;
        let mut move_test = false;
        let mut no_rewind = false;
        let mut strafe = false;
        let mut predict = false;
        let mut kept = Vec::new();
        let mut options = options.iter();
        while let Some(option) = options.next() {
            match option.to_str() {
                Some("--rival-script") => rival_script = true,
                // The launcher's Test KH3 mode: the KH3 ruleset against the
                // training dummy; with `--rival-script`, against the scripted
                // rival instead (the unattended tests' way; duel_kh3.rs).
                // [Test] The KH3 folder, instead of the launcher's.
                Some("--kh3-folder") => {
                    kh3_folder = Some(PathBuf::from(options.next().ok_or("KH3 folder absent")?));
                }
                Some("--kh3-test") => {
                    kh3_test = true;
                    duel_rules.ruleset = duel_rules::Ruleset::Kh3;
                }
                // [KHC-060] The launcher's KH2 practice: the same Sora alone
                // against the standing dummy, under the KH2 ruleset.
                Some("--kh2-test") => {
                    kh3_test = true;
                    duel_rules.ruleset = duel_rules::Ruleset::Kh2;
                }
                // [Test] The KH3 test standing still while the deck's script runs.
                Some("--kh3-deck-test") => kh3_deck_test = true,
                // [Test] The KH3 test standing still while the looks show.
                Some("--kh3-preview") => {
                    rival_script = true;
                    duel_rules.ruleset = duel_rules::Ruleset::Kh3;
                    let spec = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("preview list absent")?;
                    kh3_preview = Some(duel_kh3::Preview::parse(spec)?);
                }
                Some("--duel-bench") => duel_bench = true,
                Some("--duel-magic") => duel_magic = true,
                Some("--duel-shots") => duel_shots = true,
                Some("--duel-forfeit") => duel_forfeit = true,
                Some("--duel-quick") => duel_quick = true,
                Some("--preload") => self.preload = true,
                Some("--duel-wisdom") => duel_wisdom = true,
                Some("--duel-no-mirage") => duel_no_mirage = true,
                Some("--duel-idle") => duel_idle = true,
                Some("--bench-matrix") => {
                    let rounds = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("bench rounds absent")?
                        .parse::<u32>()?;
                    let mut b = bench.take().unwrap_or_default();
                    b.rounds = rounds;
                    bench = Some(b);
                }
                Some("--bench-idle") => {
                    let mut b = bench.take().unwrap_or_default();
                    b.idle = true;
                    bench = Some(b);
                }
                Some("--bench-out") => {
                    let out = PathBuf::from(options.next().ok_or("bench file absent")?);
                    let mut b = bench.take().unwrap_or_default();
                    b.out = Some(out);
                    bench = Some(b);
                }
                // [KHC-015] The per-ruleset scale values (duel_rules::PARITY) under the host's multipliers.
                Some("--duel-parity") => duel_rules.parity = true,
                Some("--host") => net_host = true,
                Some("--join") => {
                    net_join = Some(
                        options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("host address absent")?
                            .to_owned(),
                    );
                }
                Some("--port") => {
                    net_port = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("port absent")?
                        .parse::<u16>()?;
                }
                Some("--name") => {
                    net_name = Some(
                        options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("name absent")?
                            .to_owned(),
                    );
                }
                Some("--net-sim") => {
                    net_sim = duel_net::parse_conditions(
                        options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("link conditions absent")?,
                    )?;
                }
                Some("--net-overlay") => net_overlay = true,
                // Internet play: the rendezvous and relay server, and with a code
                // the room to join there. A host's own code comes from it.
                Some("--relay") => {
                    relay = Some(duel_net::parse_address(
                        options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("relay address absent")?,
                    )?);
                }
                Some("--room") => {
                    let code = options
                        .clone()
                        .next()
                        .and_then(|v| v.to_str())
                        .filter(|v| !v.starts_with("--"));
                    // Six characters: a relay's room; twelve: a direct code (KHC-051).
                    if let Some(code) = code {
                        options.next();
                        if let Some(addr) = net::direct::parse_code(code) {
                            direct_host = Some(addr);
                        } else {
                            room = Some(net::wire::parse_code(code).ok_or(
                                "room code: six (relay) or twelve (direct) letters and digits, \
                                 with no 0, O, 1, I or L; a direct code's check failed",
                            )?);
                        }
                    } else {
                        room_asked = true;
                    }
                }
                // [Test] Everything through the relay, with no direct try.
                Some("--relay-only") => relay_only = true,
                // [Test] This far into the script the link goes for this long.
                Some("--net-cut") => {
                    let text = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("net cut absent")?;
                    let (at, seconds) = text.split_once(',').ok_or("net cut: TICKS,SECONDS")?;
                    net_cut = Some((
                        at.trim().parse::<u64>()?,
                        std::time::Duration::from_secs_f32(seconds.trim().parse::<f32>()?),
                    ));
                }
                // Host: snapshots a second, for a slow upload link.
                Some("--snapshot-rate") => {
                    snapshot_every = match options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("snapshot rate absent")?
                    {
                        "60" => 1,
                        "30" => 2,
                        _ => return Err("snapshot rate: 60 or 30".into()),
                    };
                }
                Some("--duel-damage") => {
                    duel_rules.damage = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel damage absent")?
                        .parse::<f32>()?
                        .clamp(duel_rules::DAMAGE_RANGE.0, duel_rules::DAMAGE_RANGE.1);
                }
                Some("--duel-magic-damage") => {
                    duel_rules.magic = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel magic damage absent")?
                        .parse::<f32>()?
                        .clamp(duel_rules::DAMAGE_RANGE.0, duel_rules::DAMAGE_RANGE.1);
                }
                Some("--duel-hit-cap") => {
                    let cap = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel hit cap absent")?
                        .parse::<u8>()?;
                    if !duel_rules::HIT_CAPS.contains(&cap) {
                        return Err(format!(
                            "duel hit cap: one of {:?} percent (100: none)",
                            duel_rules::HIT_CAPS
                        )
                        .into());
                    }
                    duel_rules.hit_cap = cap;
                }
                Some("--duel-ruleset") => {
                    let name = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel ruleset absent")?;
                    duel_rules.ruleset =
                        duel_rules::ruleset_named(name).ok_or("duel ruleset: kh2, kh3 or each")?;
                }
                Some("--duel-pick-flip") => duel_pick_flip = true,
                // [KHC-016] Host: the rules (and bans) of a rules code.
                Some("--duel-code") => {
                    let code = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("rules code absent")?;
                    duel_code = Some(duel_wire::decode_code(code)?);
                }
                // [KHC-030] Host: the winner's motion at the result.
                Some("--duel-victory") => {
                    let name = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel victory absent")?;
                    duel_victory = duel_match_room::VICTORY_MOTIONS
                        .iter()
                        .position(|v| v.0 == name)
                        .ok_or("duel victory: none, stance or summon")?;
                }
                // The host's buttons wait as long as the joiner's.
                Some("--latency-match") => duel_rules.latency_match = true,
                Some("--no-latency-match") => duel_rules.latency_match = false,
                // [Test] Both Soras stand and jump now and then, to time a press.
                Some("--duel-press-test") => press_test = true,
                // [Test] A joiner's Sora only walks and jumps.
                Some("--duel-move-test") => move_test = true,
                // [Test] Swings are judged against the present.
                Some("--no-rewind") => no_rewind = true,
                // [Test] The host's Sora steps side to side and guards.
                Some("--duel-strafe") => strafe = true,
                // Who wins a dodge begun after what the joiner saw.
                Some("--duel-dodge") => {
                    let name = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel dodge absent")?;
                    duel_rules.dodge = duel_rewind::DodgePolicy::named(name)
                        .ok_or("duel dodge: attacker or defender")?;
                }
                // This player's own Sora moves at once (duel_predict.rs).
                Some("--predict") => predict = true,
                // [Test] The scripts start when loading is over (duel_script.rs).
                Some("--script-after-load") => script.on = true,
                Some("--script-locked") => {
                    script.on = true;
                    script.locked = true;
                }
                Some("--script-stop") => {
                    script.stop = Some(
                        options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("script stop absent")?
                            .parse()?,
                    );
                }
                // [Test] This game panics at this tick (crash.rs).
                Some("--duel-crash-at") => {
                    script.crash_at = Some(
                        options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("crash tick absent")?
                            .parse()?,
                    );
                }
                Some("--duel-seed") => {
                    duel_seed = Some(
                        options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("duel seed absent")?
                            .parse()?,
                    );
                }
                Some("--net-log") => {
                    net_log = Some(PathBuf::from(options.next().ok_or("net log path absent")?));
                }
                Some("--state-hash") => {
                    state_hash = Some(PathBuf::from(
                        options.next().ok_or("state hash path absent")?,
                    ));
                }
                Some("--state-dump") => {
                    state_dump = Some(PathBuf::from(
                        options.next().ok_or("state dump path absent")?,
                    ));
                }
                Some("--replay-out") => {
                    replay_out = Some(PathBuf::from(options.next().ok_or("replay path absent")?));
                }
                Some("--replay") => {
                    replay = Some(PathBuf::from(options.next().ok_or("replay file absent")?));
                }
                // [Test] Alone: the scripted rival's ruleset where each player
                // picks.
                Some("--duel-rival-pick") => {
                    duel_rules.picks[1] = match options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("rival pick absent")?
                    {
                        "kh2" => false,
                        "kh3" => true,
                        _ => return Err("rival pick: kh2 or kh3".into()),
                    };
                }
                // [Test] This player's own ruleset where each player picks.
                Some("--duel-pick") => {
                    duel_pick = Some(
                        match options
                            .next()
                            .and_then(|v| v.to_str())
                            .ok_or("duel pick absent")?
                        {
                            "kh2" => false,
                            "kh3" => true,
                            _ => return Err("duel pick: kh2 or kh3".into()),
                        },
                    );
                }
                Some("--duel-arena") => {
                    let name = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel arena absent")?;
                    duel_rules.arena =
                        duel_arena::named(name).ok_or("duel arena: mirage or room")?;
                }
                Some("--duel-mode") => {
                    let name = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel mode absent")?;
                    duel_rules.mode =
                        duel_rules::mode_named(name).ok_or("duel mode: stock, best3 or time")?;
                }
                Some("--duel-time") => {
                    let seconds = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("duel time absent")?
                        .parse::<u16>()?;
                    if !duel_rules::TIME_LIMITS.contains(&seconds) {
                        return Err(format!(
                            "duel time: one of {:?} seconds",
                            duel_rules::TIME_LIMITS
                        )
                        .into());
                    }
                    duel_rules.time_limit = seconds;
                }
                Some("--bind") => {
                    net_bind = options
                        .next()
                        .and_then(|v| v.to_str())
                        .ok_or("bind address absent")?
                        .parse()?;
                }
                _ => kept.push(option.clone()),
            }
        }
        self.options = Some(Options {
            rival_script,
            kh3_test,
            kh3_deck_test,
            kh3_folder,
            duel_bench,
            duel_magic,
            duel_shots,
            duel_forfeit,
            duel_quick,
            duel_wisdom,
            duel_no_mirage,
            duel_idle,
            bench,
            kh3_preview,
            net_host,
            net_join,
            net_port,
            net_bind,
            net_name,
            net_sim,
            net_overlay,
            snapshot_every,
            net_cut,
            relay,
            room,
            relay_only,
            room_asked,
            direct_host,
            duel_rules,
            duel_pick,
            duel_pick_flip,
            duel_code,
            duel_victory,
            script,
            duel_seed,
            net_log,
            state_hash,
            state_dump,
            replay_out,
            replay,
            press_test,
            move_test,
            predict,
            no_rewind,
            strafe,
        });
        Ok(kept)
    }

    fn schedule(&self) -> Option<(Vec<u64>, u64)> {
        let p = self.options.as_ref()?.kh3_preview.as_ref()?;
        Some((p.captures(), p.end()))
    }

    fn setup_game(&mut self, game: &mut Game, args: &[PathBuf]) -> Result<(), Box<dyn Error>> {
        let Options {
            rival_script,
            kh3_test,
            kh3_deck_test,
            kh3_folder,
            duel_bench,
            duel_magic,
            duel_shots,
            duel_forfeit,
            duel_quick,
            duel_wisdom,
            duel_no_mirage,
            duel_idle,
            bench,
            kh3_preview,
            net_host,
            net_join,
            net_port,
            net_bind,
            net_name,
            net_sim,
            net_overlay,
            snapshot_every,
            net_cut,
            relay,
            room,
            relay_only,
            room_asked,
            direct_host,
            mut duel_rules,
            duel_pick,
            duel_pick_flip,
            duel_code,
            duel_victory,
            mut script,
            duel_seed,
            net_log,
            state_hash,
            state_dump,
            replay_out,
            replay,
            press_test,
            move_test,
            predict,
            no_rewind,
            strafe,
        } = self.options.take().ok_or("options come first")?;
        // Alone, this player's pick is the host's seat's at once (a session's
        // goes through the lobby).
        if let (Some(pick), false, None) = (duel_pick, net_host, &net_join) {
            duel_rules.picks[0] = pick;
        }
        let replay = replay
            .map(|path| duel_replay::Replay::read(&path))
            .transpose()?;
        if let Some(r) = &replay {
            duel_rules = duel_wire::decode_rules(&r.header.rules)
                .ok_or("this replay's rules cannot be read")?;
            // The log marks the fight's start, for comparing it with the
            // original's.
            script.on = true;
        }
        self.state_hash = state_hash
            .map(|p| duel_hash::HashFile::create(&p, state_dump.as_deref()))
            .transpose()?;
        game.versus = rival_script || kh3_test;
        // [KHC-061] Connected is for players: the room's debug menu, status
        // line and F-key aids are off.
        game.no_debug = true;
        game.ext = Box::new(Online {
            rival_script,
            duel_bench,
            duel_magic,
            duel_idle,
            bench,
            duel_arena: duel_arena::TEST_ROOM,
            duel_rules,
            net_pad: rival::RivalPad::default(),
            duel_events: Vec::new(),
            net_overlay: false,
            net_simulated: None,
            duel_frozen: false,
            duel_test_vote: false,
            duel_vote_up: false,
            // [Test] `--duel-no-mirage`: this install acts as one without
            // Birth by Sleep.
            mirage: if duel_no_mirage {
                mirage_arena::Avail::Missing("left out for a test (--duel-no-mirage)".into())
            } else {
                mirage_arena::Avail::Unknown
            },
            bbs_digest: None,
            duel_shots,
            duel_forfeit,
            duel_quick,
            duel_wisdom,
            duel_shot_keys: std::collections::HashSet::new(),
            duel_pick,
            duel_pick_flip,
            duel_code,
            duel_victory,
            kh3_poles: None,
            kh3_geometry: None,
            kh3_core_hits: Vec::new(),
            kh3_preview,
            kh3_install: kh3_folder
                .or_else(|| crate::launcher::kh3_folder().0)
                .and_then(|folder| {
                    kh3_data::admit(&folder)
                        .inspect_err(|e| eprintln!("KH3 install refused: {e}"))
                        .ok()
                }),
            kh3_book: None,
            kh3_digest: None,
            kh3_failed: false,
            kh3_hud: None,
            kh3_fx_remote: Vec::new(),
            kh3_loading: false,
            kh3_dummy: kh3_test && !rival_script,
            kh3_dummy_seen: 0,
            kh3_menu: None,
            kh3_pending: None,
            kh3_deck_test,
            duel_match: false,
            match_live: false,
            duel_dealt: [0; 2],
            duel_landed: [0; 2],
            duel_camera: None,
            kh3_aim_view: None,
            kh3_camera: kh3_camera::Kh3Camera::default(),
            census: serde_json::Value::Null,
            script,
            duel_seed,
            rival_sounds: (0, 0),
            rewind: duel_rewind::Rewind {
                off: no_rewind,
                ..duel_rewind::Rewind::default()
            },
            strafe,
            predict: duel_predict::Predict::new(predict && replay.is_none()),
            own_delay: duel_delay::OwnDelay::default(),
            press_test,
            move_test,
            press_seen: [None; 2],
            preloading: false,
        });
        let remembered = crate::launcher::remembered_capabilities(
            game.ext.online().kh3_install.as_ref(),
            args.first().map(PathBuf::as_path),
        );
        // Each match's replay: not an unattended run's unless it says where.
        let unattended = game.autoplay.is_some();
        let replay_out = match replay_out {
            Some(path) => Some(Some(path)),
            None => (!unattended).then_some(None),
        };
        // A LAN duel: the two Soras and nothing else (duel_net.rs).
        let net_setup = match (net_host, net_join) {
            _ if replay.is_some() => replay.as_ref().map(|r| duel_net::Setup {
                role: net::Role::Join,
                bind: net_bind,
                port: net_port,
                host: None,
                name: r.header.join_name.clone(),
                conditions: net::link::Conditions::default(),
                overlay: true,
                net_log: None,
                replay_out: None,
                remembered,
                snapshot_every: 1,
                cut: None,
                relay: None,
                room: None,
                relay_only: false,
                direct: false,
            }),
            (true, Some(_)) => return Err("--host and --join exclude each other".into()),
            (true, None) => Some(duel_net::Setup {
                role: net::Role::Host,
                bind: net_bind,
                port: net_port,
                host: None,
                name: net_name.clone().unwrap_or_else(duel_net::default_name),
                conditions: net_sim,
                overlay: net_overlay,
                net_log: net_log.clone(),
                replay_out: replay_out.clone(),
                remembered,
                snapshot_every,
                cut: net_cut,
                // A host's code comes from the relay when there is one, else
                // from its own address.
                direct: room_asked && relay.is_none(),
                relay,
                room: None,
                relay_only,
            }),
            (false, None) if room.is_some() => Some(duel_net::Setup {
                role: net::Role::Join,
                bind: net_bind,
                port: net_port,
                host: None,
                name: net_name.clone().unwrap_or_else(duel_net::default_name),
                conditions: net_sim,
                overlay: net_overlay,
                net_log,
                replay_out,
                remembered,
                snapshot_every: 1,
                cut: net_cut,
                relay: Some(relay.ok_or("--room needs --relay")?),
                room,
                relay_only,
                direct: false,
            }),
            (false, None) if direct_host.is_some() => Some(duel_net::Setup {
                role: net::Role::Join,
                bind: net_bind,
                port: net_port,
                host: direct_host.map(std::net::SocketAddr::V4),
                name: net_name.clone().unwrap_or_else(duel_net::default_name),
                conditions: net_sim,
                overlay: net_overlay,
                net_log,
                replay_out,
                remembered,
                snapshot_every: 1,
                cut: net_cut,
                relay: None,
                room: None,
                relay_only: false,
                direct: true,
            }),
            (false, Some(address)) => Some(duel_net::Setup {
                role: net::Role::Join,
                bind: net_bind,
                port: net_port,
                host: Some(duel_net::parse_address(&address)?),
                name: net_name.clone().unwrap_or_else(duel_net::default_name),
                conditions: net_sim,
                overlay: net_overlay,
                net_log,
                replay_out,
                remembered,
                snapshot_every: 1,
                cut: net_cut,
                relay: None,
                room: None,
                relay_only: false,
                direct: false,
            }),
            (false, None) => None,
        };
        // The duel's baseline (duel_loadout.rs), for a session or the scripted
        // rival; the room's own build comes back when the session ends.
        let mut duel_loadout = duel_loadout::DuelLoadout::default();
        if net_setup.is_some() || rival_script || game.ext.online().kh3_dummy {
            let record = duel_loadout::apply(game, &mut duel_loadout)?;
            game.event(record);
        }
        // Unattended runs keep no duel prefs (duel_prefs.rs).
        let duel_unattended = game.autoplay.is_some();
        let duel_session = match &net_setup {
            Some(setup) => {
                let session = match &replay {
                    Some(r) => duel_net::DuelNet::playback(setup, &r.header.host_name)?,
                    None => duel_net::DuelNet::start(setup)?,
                };
                game.versus = true;
                game.local_slot = duel_net::local_slot(setup.role);
                game.replica = setup.role == net::Role::Join;
                game.ext.online_mut().net_overlay = setup.overlay;
                // [Duel rule] Sora alone: magic, items and Drive forms are the
                // host's to run for both (duel_sync); the second deck's limits,
                // the summons, the party and the Shadow are off.
                game.second_deck = None;
                game.summons = None;
                game.party.clear();
                if let Some(rt) = game.enemy.as_mut() {
                    rt.enabled = false;
                }
                game.event(serde_json::json!({"event":"duel_session",
                "role":format!("{:?}", setup.role),"port":setup.port,
                "host":setup.host.map(|h| h.to_string()),"name":setup.name,
                "conditions":[setup.conditions.latency_ms, setup.conditions.jitter_ms],
                "loss":setup.conditions.loss_percent}));
                Some(session)
            }
            None => None,
        };
        // The launcher's practice is a duel as a session's is: Sora alone
        // (duel_kh3.rs), under either ruleset.
        let dummy = game.ext.online().kh3_dummy;
        if duel_session.is_none()
            && (rival_script || dummy)
            && (dummy || duel_rules.kh3() || game.ext.online().script.on)
        {
            game.second_deck = None;
            game.summons = None;
            game.party.clear();
            // The dummy is the room's Shadow, standing (duel_kh3::dummy_setup
            // puts it in front of Sora).
            let dummy = game.ext.online().kh3_dummy;
            if let Some(rt) = game.enemy.as_mut() {
                rt.enabled = dummy;
                rt.enemy.dummy = dummy;
            }
        }
        // Another protocol or install is refused, with the reason.
        self.replay = match replay {
            Some(r) => {
                let own = game.ext.online().kh3_install.as_ref().map(|i| i.identity);
                let exe = duel_net::build_identity().ok();
                let warnings = duel_replay::check(&r.header, net::wire::PROTOCOL, own, exe)?;
                Some(duel_replay::ReplayPlayer::new(r, warnings))
            }
            None => None,
        };
        self.loadout = duel_loadout;
        self.session = duel_session;
        // Unattended runs and replays keep no duel prefs (duel_prefs.rs).
        self.unattended = duel_unattended || self.replay.is_some();
        Ok(())
    }

    fn build_render(&mut self, app: &mut App) {
        app.add_plugins(kh3_shaded::ShadedPlugin);
    }

    fn build_app(&mut self, app: &mut App, args: &[PathBuf]) {
        let duel_unattended = self.unattended;
        if let Some(session) = self.session.take() {
            app.insert_resource(session);
            app.insert_resource(duel_prefs::DuelPrefs::load(&args[1], duel_unattended));
            app.init_resource::<duel_lobby_room::LobbyRoom>();
            app.init_resource::<duel_vote::VoteRoom>();
            app.init_resource::<duel_match_room::MatchRoom>();
            app.init_resource::<duel_music::DuelMusic>();
            // The Birth by Sleep arena reads the install's BBS package, bound
            // by a manifest kept beside the KH2 one.
            app.insert_resource(mirage_arena::MirageArena::new(
                args[0].clone(),
                bbs_data::bbs_source::manifest_beside(&args[1]),
            ));
            app.add_systems(
                Update,
                (
                    mirage_arena::tick,
                    mirage_arena::show,
                    mirage_arena::follow_sky,
                    mirage_arena::scroll,
                )
                    .chain(),
            );
            app.add_systems(Update, duel_match_room::warm);
        }
        // [KHC-058] `--preload`.
        app.insert_resource(preload::Preload::new(self.preload))
            .init_resource::<preload::PreloadSheets>()
            .add_systems(Update, preload::tick)
            .add_systems(
                Update,
                preload::present
                    .after(RoomSet::PerfPresent)
                    .before(RoomSet::Finish),
            );
        if let Some(file) = self.state_hash.take() {
            app.insert_resource(file);
            app.add_systems(
                FixedUpdate,
                duel_hash::tick
                    .after(duel_sync::host_send)
                    .before(duel_predict::host_show_again)
                    .run_if(field_running),
            );
        }
        if let Some(player) = self.replay.take() {
            app.insert_resource(player);
            app.add_systems(Update, duel_replay::keys);
            app.add_systems(
                FixedUpdate,
                duel_replay::feed
                    .before(duel_net::begin)
                    .before(RoomSet::ReadPad),
            );
        }
        app.insert_resource(std::mem::take(&mut self.loadout));
        app.init_resource::<duel_kh3::Kh3Hud>()
            .init_resource::<kh3_deck::Kh3Deck>()
            .init_resource::<kh3_model::Kh3Model>()
            .init_resource::<kh3_fx::Kh3Fx>()
            .init_resource::<kh3_post::Kh3Post>()
            .add_systems(Startup, (rival_play::script_setup, duel_kh3::dummy_setup))
            .add_systems(
                PostUpdate,
                kh3_model::update
                    .before(bevy::transform::TransformSystems::Propagate)
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            )
            .add_systems(
                Last,
                (duel_net::close_on_exit, census_note.before(cleanup::census)),
            )
            .add_systems(
                FixedUpdate,
                (
                    (duel_net::begin, hold_menu)
                        .chain()
                        .before(RoomSet::ReadPad),
                    (
                        duel_lobby_room::tick,
                        duel_vote::tick,
                        duel_match_room::tick,
                        kh3_menu::tick,
                        duel_music::tick.run_if(resource_exists::<duel_music::DuelMusic>),
                    )
                        .chain()
                        .after(RoomSet::ReadPad)
                        .before(RoomSet::CampTick),
                    duel_script::tick
                        .after(duel_match_room::tick)
                        .before(RoomSet::CampTick),
                    kh3_deck::before
                        .after(RoomSet::CampTick)
                        .before(RoomSet::MissionTick),
                    kh3_deck::after
                        .after(RoomSet::Simulate)
                        .before(RoomSet::HudTick),
                    duel_sync::join_apply
                        .after(RoomSet::MissionTick)
                        .before(RoomSet::ReactionCommand)
                        .run_if(field_running),
                    (
                        duel_sync::join_send,
                        duel_sync::host_take_inputs,
                        rival_play::tick,
                        duel_hits::tick,
                        kh3_hits::tick,
                        duel_bench::tick,
                    )
                        .chain()
                        .after(RoomSet::Simulate)
                        .before(RoomSet::SummonTick)
                        .run_if(field_running),
                    duel_fx::host_effects
                        .after(RoomSet::EffectsTick)
                        .before(RoomSet::DriveFxTick)
                        .run_if(field_running),
                    duel_fx::host_sounds
                        .after(RoomSet::HudTick)
                        .before(RoomSet::PlaySounds)
                        .run_if(field_running),
                    duel_sync::host_send
                        .after(RoomSet::MusicTick)
                        .before(RoomSet::KeepWeaponPose)
                        .run_if(field_running),
                    duel_script::press_seen
                        .after(duel_sync::host_send)
                        .before(duel_predict::host_show_again),
                    // The host's own Sora ahead of his simulation, for all but
                    // what goes out (duel_predict.rs).
                    duel_predict::host_show
                        .after(kh3_hits::tick)
                        .before(RoomSet::SummonTick),
                    duel_predict::host_hide
                        .after(RoomSet::MusicTick)
                        .before(duel_sync::host_send),
                    duel_predict::host_show_again
                        .after(duel_sync::host_send)
                        .before(RoomSet::KeepWeaponPose),
                    duel_net::end.after(RoomSet::KeepWeaponPose),
                ),
            )
            .add_systems(
                Update,
                (
                    cap_queues.before(cleanup::run),
                    kh3_deck::draw.after(compose_2d).before(camp_present),
                    kh3_post::draw.after(compose_2d).before(camp_present),
                    (rival_play::spawn_drawing, rival_play::present)
                        .chain()
                        .after(RoomSet::Present)
                        .before(RoomSet::ThrownKeyblade),
                    (duel_net::sample, duel_net::present, duel_kh3::present)
                        .chain()
                        .after(RoomSet::PerfPresent)
                        .before(RoomSet::Finish),
                ),
            );
    }
}

/// A screen outside the camp has the menu keys this tick.
fn hold_menu(
    lobby: Option<Res<duel_lobby_room::LobbyRoom>>,
    vote: Option<Res<duel_vote::VoteRoom>>,
    duel_match: Option<Res<duel_match_room::MatchRoom>>,
    mut game: ResMut<Game>,
) {
    game.menu_held = lobby.is_some_and(|l| l.menu_up())
        || vote.is_some_and(|v| v.menu_up(usize::from(game.replica)))
        || duel_match.is_some_and(|m| m.menu_up());
}

/// The session's queues for the census line.
fn census_note(net: Option<Res<duel_net::DuelNet>>, mut game: ResMut<Game>) {
    let events = game.ext.online().duel_events.len();
    let note = net.as_deref().map(|n| {
        serde_json::json!({"snapshots":n.snapshots.len(),"events":n.events.len(),
            "inputs":n.inputs.len(),"inbox":n.inbox.len(),"warnings":n.warnings.len(),
            "duel_events":events})
    });
    game.ext.online_mut().census = note.unwrap_or(serde_json::Value::Null);
}

/// The queues' ceilings.
fn cap_queues(mut net: Option<ResMut<duel_net::DuelNet>>, mut game: ResMut<Game>) {
    cleanup::cap(&mut game.ext.online_mut().duel_events, cleanup::MAX_QUEUE);
    if let Some(net) = net.as_deref_mut() {
        while net.events.len() > cleanup::MAX_QUEUE {
            net.events.pop_front();
        }
        while net.warnings.len() > cleanup::MAX_QUEUE {
            net.warnings.pop_front();
        }
        while net.inputs.len() > cleanup::MAX_QUEUE {
            net.inputs.pop_first();
        }
    }
}

const HOOKS: RoomHooks = RoomHooks {
    new_fighter: || Box::new(FighterOnline::default()),
    frozen: |game| {
        // Between matches (the lobby, the countdown) neither Sora moves.
        // [Test] `--duel-idle`: nor does this player's; nor a Sora whose script
        // waits for loading to end.
        let o = game.ext.online();
        o.duel_frozen || o.duel_idle || o.kh3_loading || o.script.waiting() || o.preloading
    },
    drive,
    own_pad: duel_delay::own_pad,
    layer_on: |game| duel_kh3::on(game),
    cast,
    replica_input,
    replica_sounds: duel_predict::own_sounds,
    sora_step,
    after_magic: |game| {
        // [KH3 ruleset] The shotlock bolts onto their bullets.
        if duel_kh3::mine(game) {
            let slot = game.local_slot;
            duel_kh3::bolts(game, slot);
        }
    },
    camera_sora: duel_predict::host_predict,
    camera_tick,
    camera_view: |game, fov| {
        // The match's shots, else the wall camera (KH3's lock-on and aiming
        // cameras are the fixed tick's).
        game.ext
            .online()
            .duel_camera
            .or_else(|| duel_kh3::flow_view(game, fov))
    },
    draws_soras: kh3_model::shows_kh3,
    sora_turn: |f| duel_kh3::turn(f.player.yaw, &f.ext.fighter().kh3),
    steers_bullet: duel_kh3::is_bolt,
    bullet_style: duel_kh3::bullet_style,
    keeps_loop: |key| key == duel_music::KEY,
    census: |game| game.ext.online().census.clone(),
    debug_rows,
    debug_action,
    note_event: crate::crash::note_event,
};

/// Who drives the room Sora: the benches on the host, the KH3 script of an
/// unattended test, and KH3's two buttons.
fn drive(game: &mut Game, tick: u64) -> SoraDrive {
    let (bench, magic) = {
        let o = game.ext.online();
        (o.duel_bench, o.duel_magic)
    };
    // [Test] The scripts' own clock (duel_script.rs): while it waits nobody
    // drives this Sora; while it runs, only the bench or script does.
    let clocked = game.ext.online().script.on;
    let Some(tick) = duel_script::now(game, tick) else {
        return SoraDrive {
            commands_only: true,
            ..SoraDrive::default()
        };
    };
    // [Test] `--duel-bench` on the host: its Sora closes in and swings as
    // the bench pad says.
    let bench_pad = ((bench || magic) && !game.replica && game.autoplay.is_some())
        .then(|| {
            let foe = game.rivals.first()?.fighter.player.position;
            let me = game.sora.player.position;
            Some(if magic {
                rival_play::magic_pad(tick, 0, me, foe)
            } else {
                rival_play::bench_pad(tick, me, foe)
            })
        })
        .flatten();
    // [Test] `--duel-strafe` on the host.
    if game.ext.online().strafe && !game.replica {
        let foe = game.rivals.first().map(|r| r.fighter.player.position);
        if let Some(foe) = foe {
            return SoraDrive {
                commands_only: true,
                script: Some(duel_script::strafe_pad(
                    tick,
                    game.sora.player.position,
                    foe,
                )),
                ..SoraDrive::default()
            };
        }
    }
    // [Test] `--duel-press-test` on the host.
    if game.ext.online().press_test && !game.replica {
        let pad = duel_script::press_pad(tick, game.local_slot);
        duel_script::note_press(game, &pad, tick);
        return SoraDrive {
            commands_only: true,
            script: Some(pad),
            ..SoraDrive::default()
        };
    }
    // [Test] The unattended KH3 test: the room Sora runs the KH3 script
    // (duel_kh3.rs); otherwise R2 and Triangle are the KH3 buttons.
    let script = duel_kh3::scripted_room_pad(game, tick);
    let pad = match script {
        Some((_, k)) => k,
        None => duel_kh3::room_pad(game),
    };
    SoraDrive {
        bench: bench_pad,
        // [Test] `--duel-magic` on the host.
        commands_only: magic || clocked,
        script: script.map(|(p, _)| p),
        buttons: ExtraButtons {
            shotlock: pad.shotlock,
            flow: pad.situation,
        },
    }
}

fn cast(game: &mut Game, t: u8, tick: u64) {
    let mp = game.sora.player.mp.cur;
    let ok = if duel_kh3::mine(game) && game.ext.online().kh3_book.is_some() {
        duel_kh3::kh3_cast(game, t)
    } else {
        magic_room::request(game, t, tick)
    };
    if ok && duel_kh3::mine(game) && !game.sora.ext.fighter().kh3.core_on() {
        game.sora.ext.fighter_mut().kh3.spell_released(t);
    }
    if game.versus {
        let (slot, after) = (game.local_slot, game.sora.player.mp.cur);
        let levels = game.magic.as_ref().map(|m| m.levels);
        game.event(serde_json::json!({"event":"duel_cast","tick":tick,
            "slot":slot,"magic":t,"ok":ok,"mp":[mp, after],"levels":levels}));
    }
}

/// The joiner's Sora is the host's to move: his buttons go to the host
/// (duel_sync), and the host's snapshots place him; with `--predict` they move
/// him here too, until a snapshot says otherwise.
fn replica_input(
    game: &mut Game,
    input: &Input,
    command: rival::Command,
    buttons: ExtraButtons,
    frozen: bool,
    tick: u64,
) -> player::Step {
    let mut pad = rival::RivalPad::of(input);
    pad.command = command;
    pad.shotlock_held = buttons.shotlock;
    pad.flow = buttons.flow;
    // [Test] `--rival-script` on a joiner: the rival's test script
    // presses his buttons, against the host's Sora.
    let o = game.ext.online();
    let (script, bench, magic, wisdom, kh3) = (
        o.rival_script,
        o.duel_bench,
        o.duel_magic,
        o.duel_wisdom,
        o.duel_rules.seat_kh3(game.local_slot),
    );
    let tick = duel_script::now(game, tick);
    let scripted = script || bench || magic || wisdom;
    if scripted && tick.is_none() {
        pad = rival::RivalPad::default();
    }
    // [Test] `--script-locked`: the host decides who stands, on its own tick.
    if scripted
        && (!frozen || game.ext.online().script.locked)
        && let Some(tick) = tick
        && let Some(foe) = game.rivals.first().map(|r| r.fighter.player.position)
    {
        let me = game.sora.player.position;
        pad = if kh3 {
            rival_play::kh3_pad(
                tick,
                rival_play::KH3_SECOND,
                me,
                foe,
                rival_play::kh3_view(&game.sora, game.rivals.first().map(|r| &r.fighter)),
            )
        } else if magic {
            rival_play::magic_pad(tick, 100, me, foe)
        } else if wisdom {
            let mut p = rival_play::bench_pad(tick, me, foe);
            if tick.is_multiple_of(600) {
                p.command = rival::Command::Drive(player::forms::WISDOM);
            }
            p
        } else if bench {
            rival_play::bench_pad(tick, me, foe)
        } else {
            rival_play::script_pad(tick, me, foe)
        };
    }
    // [Test] `--duel-press-test` on a joiner.
    if game.ext.online().press_test {
        pad = rival::RivalPad::default();
        if !frozen && let Some(tick) = tick {
            pad = duel_script::press_pad(tick, game.local_slot);
            duel_script::note_press(game, &pad, tick);
        }
    }
    // [Test] `--duel-move-test` on a joiner.
    if game.ext.online().move_test {
        pad = rival::RivalPad::default();
        if !frozen && let Some(tick) = tick {
            pad = duel_script::move_pad(tick);
        }
    }
    // The KH3 deck has its own commands (kh3_deck.rs).
    if let Some(c) = game.ext.online_mut().kh3_pending.take() {
        pad.command = c;
    }
    game.ext.online_mut().net_pad = pad;
    duel_predict::step_own(game, &pad)
}

/// [KH3 ruleset] The layer over the KH2 controller.
fn sora_step(
    game: &mut Game,
    input: &Input,
    buttons: ExtraButtons,
    tick: u64,
) -> Option<player::Step> {
    if !duel_kh3::mine(game) {
        return None;
    }
    let poles = duel_kh3::poles(game);
    let foe = duel_kh3::room_foe(game);
    let pad = kh3_player::Pad {
        shotlock: buttons.shotlock,
        situation: buttons.flow,
        ..kh3_player::Pad::default()
    };
    let book = game.ext.online().kh3_book.clone();
    let geometry = duel_kh3::geometry(game);
    let foe_body = geometry
        .as_ref()
        .and_then(|_| duel_kh3::room_foe_body(game));
    let view = game.ext.online().kh3_aim_view.or_else(|| {
        duel_kh3::aim_view(
            game.sora.player.position,
            foe.map(|f: kh3_player::Foe| f.position),
        )
    });
    let out = {
        let Game {
            sora,
            params,
            clips,
            world,
            ..
        } = &mut *game;
        let scene = kh3_player::Scene {
            foe,
            poles: &poles,
            book: book.as_deref(),
            geometry: geometry.as_deref(),
            foe_body,
            view,
        };
        duel_kh3::step(sora, input, &pad, params, clips, &*world, &scene)
    };
    let slot = game.local_slot;
    duel_kh3::after(game, &out, slot);
    duel_kh3::preview_tick(game, tick);
    Some(out.step)
}

/// [KH3 ruleset] KH3's lock-on and aiming cameras over the follow camera.
fn camera_tick(game: &mut Game, f: &CameraFrame) -> Option<boss::OverlayView> {
    if !duel_kh3::mine(game) {
        let o = game.ext.online_mut();
        o.kh3_camera = kh3_camera::Kh3Camera::default();
        o.kh3_aim_view = None;
        return None;
    }
    let frame = kh3_camera::Frame {
        feet: Vec3::from_array(game.sora.player.position),
        yaw: game.sora.player.yaw,
        locked: game
            .target_locked
            .then_some(game.selected_target)
            .flatten()
            .map(Vec3::from_array),
        aim: duel_kh3::aim_point(game),
        turn: f.turn,
        height: f.height,
        eye: f.eye,
        look: f.look,
        fov: f.fov,
    };
    let o = game.ext.online_mut();
    let view = o.kh3_camera.tick(&frame);
    // The camera last shown, for the shotlock's reticle.
    o.kh3_aim_view = match &view {
        Some(v) => duel_kh3::camera_view(v.eye, v.target, v.fov),
        None => duel_kh3::camera_view(f.eye, f.look, f.fov),
    };
    view
}

const ROW_NET_OVERLAY: u8 = 0;
const ROW_NET_SIMULATOR: u8 = 1;
const ROW_DAMAGE: u8 = 2;
const ROW_TEST_VOTE: u8 = 3;
const ROW_SPAWN_RIVAL: u8 = 4;
const ROW_REMOVE_RIVAL: u8 = 5;
const ROW_PANIC: u8 = 6;

fn debug_rows(game: &Game) -> Vec<DebugRow> {
    let o = game.ext.online();
    let row = |label, value, id, enabled, steps, help| DebugRow::Row {
        label,
        value,
        id,
        enabled,
        steps,
        help,
    };
    let mut rows = vec![
        row(
            "Network overlay",
            Some(if o.net_overlay { "On" } else { "Off" }.to_owned()),
            ROW_NET_OVERLAY,
            game.versus,
            false,
            "Ping, loss, data rates and warnings at the top right.",
        ),
        row(
            "Simulated network",
            Some(
                o.net_simulated
                    .and_then(|p| duel_net::SIMULATED.get(p))
                    .map_or("Off", |s| s.0)
                    .to_owned(),
            ),
            ROW_NET_SIMULATOR,
            game.versus,
            false,
            "Steps the link simulator through its presets.",
        ),
        DebugRow::Section("Duel"),
        row(
            "Damage",
            Some(format!("x{}", o.duel_rules.damage)),
            ROW_DAMAGE,
            (game.versus || !game.rivals.is_empty()) && !game.replica,
            true,
            "Left/Right: what every Sora-against-Sora hit's damage is multiplied by.",
        ),
        row(
            "Test vote",
            None,
            ROW_TEST_VOTE,
            game.versus && !game.replica && !game.rivals.is_empty(),
            false,
            "Both players vote; different answers go to a coin flip.",
        ),
        row(
            "Scripted rival Sora",
            None,
            ROW_SPAWN_RIVAL,
            game.rivals.is_empty() && game.rival_kit.is_some(),
            false,
            "A second Sora, run by a test script, to try the duel alone.",
        ),
        row(
            "Remove the rival",
            None,
            ROW_REMOVE_RIVAL,
            game.rivals
                .iter()
                .any(|r| r.control == rival::Control::Script),
            false,
            "Remove the scripted second Sora.",
        ),
    ];
    // A debug build can stop on purpose, to try the crash report (crash.rs).
    if cfg!(debug_assertions) {
        rows.push(row(
            "Crash on purpose",
            None,
            ROW_PANIC,
            true,
            false,
            "Stops this game with an error: the report, and the other player's message.",
        ));
    }
    rows
}

fn debug_action(game: &mut Game, id: u8, step: i32) -> DebugOutcome {
    let mut out = DebugOutcome::default();
    match id {
        ROW_NET_OVERLAY => {
            let o = game.ext.online_mut();
            o.net_overlay = !o.net_overlay;
        }
        ROW_NET_SIMULATOR => {
            let o = game.ext.online_mut();
            let next = o
                .net_simulated
                .map_or(1, |p| (p + 1) % duel_net::SIMULATED.len());
            o.net_simulated = Some(next);
            out.message = Some(format!(
                "Simulated network: {}",
                duel_net::SIMULATED[next].0
            ));
        }
        ROW_DAMAGE => {
            let (lo, hi) = duel_rules::DAMAGE_RANGE;
            let d = &mut game.ext.online_mut().duel_rules.damage;
            *d = (*d + step as f32 * duel_rules::DAMAGE_STEP).clamp(lo, hi);
            out.message = Some(format!("Duel damage x{}", *d));
        }
        ROW_TEST_VOTE => {
            game.ext.online_mut().duel_test_vote = true;
            out.close = true;
        }
        ROW_SPAWN_RIVAL => {
            rival_play::spawn_scripted(game);
            out.close = true;
            out.message = Some("A scripted rival Sora joined".into());
        }
        ROW_REMOVE_RIVAL => {
            game.rivals.retain(|r| r.control != rival::Control::Script);
            out.message = Some("The rival left".into());
        }
        ROW_PANIC => panic!("F3 > Crash on purpose"),
        _ => {}
    }
    out
}
