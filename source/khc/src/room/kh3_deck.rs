//! KH3's command deck in the KH3 ruleset [KHC-004]: Attack, Magic and Items, with
//! no Drive row and no second deck. The rules are `kh3_sora::command`'s: the
//! six elements in the order they are learned, one row each at the tier owned;
//! a row short of MP recoloured but still castable unless MP Safety is on; all
//! greyed while MP Charge runs; the pouch's distinct items with their counts;
//! three shortcut pages, L1 with up or down turning them. (The pinned engine
//! draws no shortcut menu, so its rows are not on screen yet.)
//!
//! KH3's own movies draw it, read from the player's install (`kh3_ui`): `HudCommand` the deck
//! and the commands on offer, `HudTarget` the shotlock's scope and marks, `HudStatus` the HP, MP
//! and focus gauges; they ask for KH3's own HUD sounds. KH2's `CommandMenu` stays underneath, hidden:
//! this module takes the pad's presses before the room's tick and writes the rows before the
//! HUD's, so the room's own Magic, Items and shortcut paths never run. A chosen command
//! is KH3's own (`ETresCommandKind`), and goes to KH3's Sora, or on a joiner to
//! the host in the same two `rival::Command`s the KH2 menu uses. Single player
//! and the KH2 ruleset never come here.

use super::*;
use kh3_player::ability::Ability;
use kh3_ui::command::{
    CloseKind, CommandDisplay, CommandModel, CommandStrings, EventButton, MenuEvent, MenuRow,
    MenuState, ShootFlow, Situation,
};
use kh3_ui::hud_command::{BUTTON_TEXT_DOWN, BUTTON_TEXT_LEFT, HudCommand};
use kh3_ui::hud_status::{HudStatus, SlotValues, StatusDriver, StatusValues};
use kh3_ui::hud_target::{self, HudTarget, TargetDriver};
use kh3_ui::render::{Renderer, state_hash};
use kh3_ui::{Canvas, Matrix, Movie};
use player::magic::{self as spells, Mp};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use visual::field_command_menu::CommandMenu;
use visual::texel_upload::{TexelWrite, TexelWrites};

/// The main list's rows, and the sub-list kinds the menu's art knows them by.
const ROWS: usize = 3;
const MAGIC: usize = 1;
const ITEMS: usize = 2;
/// The rows' category icons.
const MAGIC_ICON: u8 = 2;
const ITEM_ICON: u8 = 3;
/// se000: cursor, open, close, refused.
const SE_CURSOR: u32 = 1;
const SE_OPEN: u32 = 3;
const SE_CLOSE: u32 = 4;
const SE_REFUSED: u32 = 5;
/// `rival::Command`'s number from here up is a KH3 command, below a magic type
/// or an item slot (the unattended scripts').
pub const FIRST_SPELL_KIND: u8 = 29;
pub const FIRST_ITEM_KIND: u8 = 58;
/// L1 (logical bit 23), and up and down.
const L1: u64 = 0x80_0000;
const UP: u64 = 0x4000;
const DOWN: u64 = 0x8000;

/// A menu row's colour index: 0 usable, 2 short of MP, 5 greyed; 1 is a group
/// row with nothing usable in it (the menu's art).
const USABLE: usize = 0;
const SHORT: usize = 2;
const GREY: usize = 5;
const GROUP_GREY: usize = 1;

/// The elements in `LEARN_ORDER`: the room's magic type whose level is the tier
/// (Magnet's level is Aero's and Reflect's Water's, the two the room lacks), and
/// the first tier's command.
const ORDER: [(u8, u8); 6] = [
    (spells::FIRE, 29),
    (spells::REFLECT, 41),
    (spells::CURE, 49),
    (spells::BLIZZARD, 33),
    (spells::THUNDER, 37),
    (spells::MAGNET, 45),
];

/// What KH3's Sora knows (`kh3_player::core`'s `magic.known`), by magic type: every
/// element's third tier, whatever the KH2 room's save holds.
const KNOWN: [u8; 6] = [3; 6];

/// A shortcut slot: `loadout`'s pages, at the tier owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shortcut {
    Empty,
    Spell(u8),
    Item(u8),
}

const PAGES: [[Shortcut; 4]; 3] = [
    [
        Shortcut::Spell(spells::FIRE),
        Shortcut::Spell(spells::BLIZZARD),
        Shortcut::Spell(spells::THUNDER),
        Shortcut::Spell(spells::CURE),
    ],
    [
        Shortcut::Spell(spells::REFLECT),
        Shortcut::Spell(spells::MAGNET),
        Shortcut::Item(58),
        Shortcut::Item(61),
    ],
    [Shortcut::Empty; 4],
];

/// One row as it is shown, with the command it runs (0: none).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: u8,
    pub label: String,
    pub colour: usize,
    /// An item's count; 0 for anything else.
    pub quantity: u32,
}

impl Row {
    fn empty() -> Row {
        Row {
            kind: 0,
            label: String::new(),
            colour: GREY,
            quantity: 0,
        }
    }

    fn usable(&self) -> bool {
        self.colour < 4
    }
}

/// The command of a magic type at a level: its tier, up to the third.
pub fn spell_kind(magic_type: u8, level: u8) -> Option<u8> {
    let &(_, first) = ORDER.iter().find(|o| o.0 == magic_type)?;
    (level > 0).then(|| first + level.min(3) - 1)
}

/// A spell's colour: `mp_cost` of its command row (255 is all that is left).
fn spell_colour(cost: i32, mp: &Mp, safety: bool) -> usize {
    if mp.charging() {
        return GREY;
    }
    let short = cost < 255 && cost > 0 && mp.cur < cost;
    match (short, safety) {
        (true, true) => GREY,
        (true, false) => SHORT,
        _ => USABLE,
    }
}

fn spell_row(kind: u8, mp: &Mp, safety: bool, cost: &impl Fn(u8) -> Option<i32>) -> Row {
    Row {
        kind,
        label: kh3_sora::magic::spells::command_name(kind).to_owned(),
        colour: cost(kind).map_or(GREY, |c| spell_colour(c, mp, safety)),
        quantity: 0,
    }
}

/// The Magic list: one row per element whose level is above 0.
pub fn magic_rows(
    levels: &[u8; 6],
    mp: &Mp,
    safety: bool,
    cost: impl Fn(u8) -> Option<i32>,
) -> Vec<Row> {
    ORDER
        .iter()
        .filter_map(|&(t, _)| spell_kind(t, levels[usize::from(t)]))
        .map(|kind| spell_row(kind, mp, safety, &cost))
        .collect()
}

/// The Items list: the pouch's distinct items, in slot order, with their counts.
/// `slots` is the room's list, which `duel_kh3::sync_pouch` keeps the pouch's.
pub fn item_rows(slots: &[u16]) -> Vec<Row> {
    let mut rows: Vec<(u8, u32)> = Vec::new();
    for kind in slots.iter().filter_map(|&s| item_of_room(s)) {
        match rows.iter_mut().find(|r| r.0 == kind) {
            Some(r) => r.1 += 1,
            None => rows.push((kind, 1)),
        }
    }
    rows.into_iter()
        .map(|(kind, n)| Row {
            kind,
            label: kh3_sora::magic::spells::command_name(kind).to_owned(),
            colour: USABLE,
            quantity: n,
        })
        .collect()
}

/// The room's item ids and KH3's item commands: Potion, Hi-Potion, Ether, Elixir,
/// Mega-Potion, Mega-Ether, Megalixir.
const ITEMS_OF_ROOM: [(u16, u8); 7] = [
    (1, 58),
    (2, 59),
    (3, 61),
    (4, 64),
    (5, 60),
    (6, 63),
    (7, 65),
];

/// A room item as KH3's command.
pub fn item_of_room(id: u16) -> Option<u8> {
    ITEMS_OF_ROOM.iter().find(|i| i.0 == id).map(|i| i.1)
}

/// A KH3 item command as the room's item.
pub fn room_of_item(kind: u8) -> Option<u16> {
    ITEMS_OF_ROOM.iter().find(|i| i.1 == kind).map(|i| i.0)
}

/// A group row's colour from its list.
fn group_colour(rows: &[Row]) -> usize {
    if rows.is_empty() {
        GREY
    } else if rows.iter().all(|r| !r.usable()) {
        GROUP_GREY
    } else {
        USABLE
    }
}

/// The buttons pressed this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Presses {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub decide: bool,
    pub back: bool,
}

/// What a tick's presses did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    None,
    Moved,
    /// Decide on Attack: the room's own tick takes it.
    Attack,
    Opened,
    Closed,
    Execute(u8),
    Refused,
}

/// The menu's place: `ui_tick`'s state machine, kept here so that a joiner's menu
/// answers at once and not a round trip later. Cancel keeps the main cursor, as
/// `ui_tick` does; a command puts it back on Attack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub main: usize,
    /// The open list (`MAGIC`, `ITEMS`) and its row.
    pub list: Option<usize>,
    pub row: usize,
}

impl Cursor {
    pub fn press(&mut self, p: Presses, magic: &[Row], items: &[Row]) -> Step {
        if let Some(list) = self.list {
            let rows = if list == MAGIC { magic } else { items };
            if rows.is_empty() {
                self.list = None;
                self.row = 0;
                return Step::Closed;
            }
            let n = rows.len();
            self.row = self.row.min(n - 1);
            let mut step = Step::None;
            if p.up && n > 1 {
                self.row = (self.row + n - 1) % n;
                step = Step::Moved;
            } else if p.down && n > 1 {
                self.row = (self.row + 1) % n;
                step = Step::Moved;
            }
            if p.decide {
                let row = &rows[self.row];
                if !row.usable() {
                    return Step::Refused;
                }
                *self = Cursor::default();
                return Step::Execute(row.kind);
            }
            if p.back || p.left {
                self.list = None;
                self.row = 0;
                return Step::Closed;
            }
            return step;
        }
        let mut step = Step::None;
        if p.up {
            self.main = (self.main + ROWS - 1) % ROWS;
            step = Step::Moved;
        } else if p.down {
            self.main = (self.main + 1) % ROWS;
            step = Step::Moved;
        }
        if self.main == 0 {
            return if p.decide { Step::Attack } else { step };
        }
        if p.decide || p.right {
            let rows = if self.main == MAGIC { magic } else { items };
            if !rows.iter().any(Row::usable) {
                return Step::Refused;
            }
            self.list = Some(self.main);
            self.row = 0;
            return Step::Opened;
        }
        step
    }
}

/// The room's rows while the deck stands in for them.
struct Saved {
    items: Vec<Item>,
    opens: Vec<Option<usize>>,
    shortcuts: [u16; 4],
}

#[derive(Resource, Default)]
pub struct Kh3Deck {
    saved: Option<Saved>,
    cursor: Cursor,
    page: usize,
    magic: Vec<Row>,
    items: Vec<Row>,
    shortcut: Vec<Row>,
    costs: Vec<(u8, i32)>,
    /// [Test] The tick the unattended deck run began: the first the room lets Sora act.
    script_start: Option<u64>,
    /// This tick's one-shot facts for the deck's movie.
    events: Vec<MenuEvent>,
    /// Triangle was pressed for this tick: a command gone from the list was used, not lost.
    flow_pressed: bool,
    shown: Option<Box<Shown>>,
}

/// The deck's region of the 1920 x 1080 stage: the movie's sub lists reach up from the bottom left.
const COMMAND_REGION: (i32, i32, usize, usize) = (0, 300, 900, 780);
/// The size the shotlock's full-screen display is drawn at.
const TARGET_SIZE: (usize, usize) = (1280, 720);
/// The gauges' region: Sora's slot stands at the bottom right, and a long HP bar reaches left.
const STATUS_REGION: (i32, i32, usize, usize) = (900, 700, 1020, 380);

/// A thread drawing one movie: the compositor is software, and the shotlock's full-screen scope
/// takes longer than a frame. A movie that changes while one is being drawn waits its turn.
struct Worker {
    to: Sender<Movie>,
    from: Mutex<Receiver<(Vec<u8>, f32)>>,
    busy: bool,
    size: (usize, usize),
}

impl Worker {
    fn new(
        hud: &kh3_ui::load::HudAssets,
        at: (i32, i32),
        size: (usize, usize),
        scale: f32,
    ) -> Worker {
        let (to, jobs) = channel::<Movie>();
        let (done, from) = channel();
        let mut renderer = Renderer::new(hud.fonts.clone());
        let _ = std::thread::Builder::new()
            .name("kh3 hud".into())
            .spawn(move || {
                let mut canvas = Canvas::region(at.0, at.1, size.0, size.1);
                let root = Matrix::scale(scale, scale);
                while let Ok(movie) = jobs.recv() {
                    let started = std::time::Instant::now();
                    canvas.reset(at.0, at.1, size.0, size.1);
                    renderer.draw(&mut canvas, &movie, &root);
                    let mut rgba = Vec::new();
                    canvas.to_rgba8(&mut rgba);
                    let ms = started.elapsed().as_secs_f32() * 1000.;
                    if done.send((rgba, ms)).is_err() {
                        break;
                    }
                }
            });
        Worker {
            to,
            from: Mutex::new(from),
            busy: false,
            size,
        }
    }

    fn ask(&mut self, movie: Movie) {
        self.busy = self.to.send(movie).is_ok();
    }

    fn take(&mut self) -> Option<(Vec<u8>, f32)> {
        let done = self.from.lock().ok()?.try_recv().ok()?;
        self.busy = false;
        Some(done)
    }
}

/// KH3's movies, playing.
struct Shown {
    command: HudCommand,
    display: CommandDisplay,
    strings: CommandStrings,
    target: HudTarget,
    driver: TargetDriver,
    command_worker: Worker,
    target_worker: Worker,
    status: HudStatus,
    status_driver: StatusDriver,
    status_worker: Worker,
    /// The HP last shown: a fall flashes the portrait.
    hp_seen: i32,
    /// The commands on offer last tick.
    offered: Vec<u32>,
    hashes: [u64; 3],
    draw_worst: f32,
    draw_total: f32,
    draw_frames: u32,
    images: Option<[(Handle<Image>, usize); 3]>,
    /// The scope is open; locks counted; marks on the foe; a shot or an airstep is playing.
    aim_open: bool,
    aim_total: i32,
    aim_marks: i32,
    aim_type: usize,
    aim_fired: bool,
    aim_airstep: bool,
    sounds: Vec<kh3_ui::UiSound>,
}

impl Shown {
    fn new(hud: &kh3_ui::load::HudAssets) -> Shown {
        let (x, y, w, h) = COMMAND_REGION;
        let mut target = HudTarget::new(Arc::clone(&hud.target), hud.fonts.clone());
        // The cockpit shows it: its root waits on `off` otherwise.
        target.native_root("loop");
        let mut status = HudStatus::new(Arc::clone(&hud.status));
        status.set_status(0, 0);
        // The ring's portrait is KH2's Sora, drawn by the room under the movie.
        let status_driver = StatusDriver::default();
        let (sx, sy, sw, sh) = STATUS_REGION;
        Shown {
            command: HudCommand::new(Arc::clone(&hud.command), hud.fonts.clone()),
            display: CommandDisplay::new(),
            strings: CommandStrings::default(),
            target,
            driver: TargetDriver::default(),
            command_worker: Worker::new(hud, (x, y), (w, h), 1.),
            target_worker: Worker::new(hud, (0, 0), TARGET_SIZE, TARGET_SIZE.0 as f32 / 1920.),
            status,
            status_driver,
            status_worker: Worker::new(hud, (sx, sy), (sw, sh), 1.),
            hp_seen: 0,
            offered: Vec::new(),
            hashes: [0; 3],
            draw_worst: 0.,
            draw_total: 0.,
            draw_frames: 0,
            images: None,
            aim_open: false,
            aim_total: 0,
            aim_marks: 0,
            aim_type: hud_target::LOCKON_TYPE_FLOWSHOTATTACK,
            aim_fired: false,
            aim_airstep: false,
            sounds: Vec::new(),
        }
    }
}

const FOE: u64 = 1;

fn menu_row(r: &Row) -> MenuRow {
    MenuRow {
        text: r.label.clone(),
        enabled: r.usable(),
        mp_short: r.colour == SHORT,
        quantity: r.quantity,
        flash: false,
    }
}

/// A command's name on the deck: the spell table's, or the game's enum in words.
fn command_text(kind: i32) -> String {
    // The Kingdom Key's form change (`form::CHANGE_L1`).
    if kind == 148 {
        return "Second Form".to_owned();
    }
    let named = u8::try_from(kind).map_or("", kh3_sora::magic::spells::command_name);
    if !named.is_empty() {
        return named.to_owned();
    }
    let game_name = kh3_sora::enums::ETresCommandKind::from_value(kind)
        .map_or("", |c| c.game_name().trim_start_matches("TRES_CMD_KIND_"));
    let mut text = String::new();
    for word in game_name.split('_').filter(|w| !w.is_empty()) {
        if !text.is_empty() {
            text.push(' ');
        }
        let mut letters = word.chars();
        text.extend(letters.next());
        text.extend(letters.flat_map(char::to_lowercase));
    }
    text
}

/// The deck's icon of a command by its HUD class: grand magic, form and finish, the rest.
fn situation_icon(class: u8) -> i32 {
    match class {
        1 => 3,
        3 | 4 => 1,
        _ => 0,
    }
}

/// A Grand Magic spell of the KH2 path, as the command it stands for.
fn grand_of_room(spell: u8) -> i32 {
    let first = match spell {
        spells::FIRE => 110,
        spells::BLIZZARD => 113,
        _ => 116,
    };
    first + 2
}

/// The commands on offer: the core's own list, and on a joiner or without a core the one the
/// host sends.
fn situations(game: &Game, m: &mut CommandModel) {
    let k = &game.sora.ext.fighter().kh3;
    if let Some(ui) = k.core_ui() {
        let s = ui.situation;
        m.situations = s
            .list
            .iter()
            .map(|e| Situation {
                serial: e.serial,
                name: command_text(e.kind.value()),
                icon_type: situation_icon(e.class),
                in_effect: 0,
                total: e.total.max(0.),
                remaining: e.remaining.max(0.),
                enabled: !e.appearing,
                refreshed: 0,
            })
            .collect();
        m.situation_selected = usize::try_from(s.selected)
            .ok()
            .filter(|&i| i < m.situations.len());
        let button = s.slot_b.map(|e| (e, true)).or(s.slot_a.map(|e| (e, false)));
        if let Some((entry, slot_b)) = button {
            let kind = s.extra.filter(|_| slot_b).unwrap_or(entry.kind);
            m.event_button = Some(EventButton {
                name: command_text(kind.value()),
                has_gauge: entry.total > 0.,
                slot_b,
            });
        }
    } else {
        let kind = k
            .grand_offer
            .map(|(cmd, _)| i32::from(cmd))
            .or(k.offer.map(|(spell, _)| grand_of_room(spell)));
        if let Some(kind) = kind {
            m.situations.push(Situation {
                serial: kind as u32 + 1,
                name: command_text(kind),
                icon_type: if (110..=125).contains(&kind) { 3 } else { 0 },
                in_effect: 0,
                total: 0.,
                remaining: 0.,
                enabled: true,
                refreshed: 0,
            });
            m.situation_selected = Some(0);
        }
    }
    m.situation_show_triangle = !m.situations.is_empty();
}

/// What the gauges show this tick: the core's numbers, or on a joiner the host's.
fn status_values(game: &Game) -> StatusValues {
    let fighter = game.sora.ext.fighter();
    let k = &fighter.kh3;
    let player = &game.sora.player;
    let vitals = &game.sora.vitals;
    let mp = player.mp;
    let in_battle = !game.ext.online().duel_frozen;
    let charge = if mp.charging() && mp.total > 0. {
        (1. - mp.remaining / mp.total).clamp(0., 1.) * mp.max as f32
    } else {
        0.
    };
    let mut slot = SlotValues {
        active: vitals.target.max_hp > 0,
        hp: vitals.hp.max(0),
        hp_max: vitals.target.max_hp,
        mp: mp.cur,
        mp_max: mp.max,
        mp_charging: mp.charging(),
        mp_charge_value: charge,
        in_battle,
    };
    let mut values = StatusValues {
        focus: k.focus.round() as i32,
        focus_max: kh3_player::tuning::FOCUS_MAX as i32,
        focus_charge: k.aim.is_some_and(|a| a.syphon),
        game_in_battle: in_battle,
        mp_gauge_unlocked: true,
        focus_gauge_unlocked: true,
        ..StatusValues::default()
    };
    if let Some(ui) = k.core_ui() {
        let g = ui.gauges;
        slot.mp_charging = g.mp_charging;
        slot.mp_charge_value = g.mp_charge_value;
        values.focus_max = g.focus_max;
    }
    values.slots[0] = slot;
    values
}

/// What the deck's movie shows this tick.
fn model(game: &Game, deck: &mut Kh3Deck) -> CommandModel {
    let aiming = game.sora.ext.fighter().kh3.aim.is_some();
    let state = if aiming {
        MenuState::ShootFlow
    } else if game.pad.held & L1 != 0 {
        MenuState::Shortcut
    } else if deck.cursor.list.is_some() {
        MenuState::SubSel
    } else {
        MenuState::MainSel
    };
    let main_row = |text: &str, rows: &[Row]| MenuRow {
        text: text.to_owned(),
        enabled: rows.iter().any(Row::usable),
        ..MenuRow::default()
    };
    let mut m = CommandModel {
        state,
        simple_hud: true,
        battle_mode: if game.ext.online().duel_frozen { 0 } else { 2 },
        link_none: true,
        main_cursor: deck.cursor.main,
        list: deck.cursor.list.unwrap_or(0),
        row_cursor: deck.cursor.row,
        shortcut_page: deck.page % PAGES.len(),
        events: std::mem::take(&mut deck.events),
        ..CommandModel::default()
    };
    m.main[0] = MenuRow::new("Attack");
    m.main[1] = main_row("Magic", &deck.magic);
    m.main[2] = main_row("Items", &deck.items);
    m.lists[MAGIC] = deck.magic.iter().map(menu_row).collect();
    m.lists[ITEMS] = deck.items.iter().map(menu_row).collect();
    for (to, from) in m.shortcut.iter_mut().zip(&deck.shortcut) {
        *to = menu_row(from);
    }
    if aiming {
        let k = &game.sora.ext.fighter().kh3;
        let syphon = k.aim.is_some_and(|a| a.syphon);
        let core = k.core_aim();
        let is_max = core.as_ref().is_some_and(|a| a.is_max);
        let first = if syphon {
            "Focus Syphon"
        } else if is_max {
            "Ragnarok"
        } else {
            "King of Hearts"
        };
        let total = core
            .as_ref()
            .map_or_else(|| i32::from(k.aim.map_or(0, |a| a.locks)), |a| a.total);
        m.shoot_flow = Some(ShootFlow {
            rows: vec![
                (first.to_owned(), BUTTON_TEXT_DOWN.to_owned()),
                ("Airstep".to_owned(), BUTTON_TEXT_LEFT.to_owned()),
            ],
            enabled: [total > 0, core.as_ref().is_some_and(|a| a.can_airstep)],
            from_shortcut: false,
        });
    }
    situations(game, &mut m);
    m
}

/// The shotlock's scope and marks (`aim` of the reference's HUD).
fn aim(game: &Game, s: &mut Shown) {
    let k = &game.sora.ext.fighter().kh3;
    let core = k.core_aim();
    let (mode, aim_time, total, max) = match &core {
        Some(a) => (a.mode, a.aim_time, a.total, a.max_locks),
        None => match k.aim {
            Some(a) => (
                if a.syphon { 4 } else { 1 },
                5.,
                i32::from(a.locks),
                kh3_player::tuning::MAX_LOCKS as i32,
            ),
            None => (0, 0., 0, 0),
        },
    };
    let open = matches!(mode, 1 | 4);
    let Shown {
        target,
        driver,
        sounds,
        ..
    } = s;
    if s.aim_airstep && mode != 3 {
        target.set_flow_scope(hud_target::SFSS_OUT_ATHLETIC_FLOW1, false, true);
        s.aim_airstep = false;
    }
    if s.aim_fired && mode != 2 {
        driver.remove_cursor(target, FOE, s.aim_type, sounds);
        s.aim_marks = 0;
        target.set_flow_scope(3, false, true);
        s.aim_fired = false;
    }
    if open && !s.aim_open {
        let syphon = mode == 4;
        driver.open_scope(target, aim_time, syphon, sounds);
        (s.aim_open, s.aim_total, s.aim_marks) = (true, 0, 0);
        s.aim_type = if syphon {
            hud_target::LOCKON_TYPE_FLOWSHOTFOCUSCHARGE
        } else {
            hud_target::LOCKON_TYPE_FLOWSHOTATTACK
        };
    }
    if !s.aim_open {
        return;
    }
    if open {
        while s.aim_marks < total {
            s.aim_marks += 1;
            if driver
                .add_cursor(target, FOE, s.aim_type, true, sounds)
                .is_none()
            {
                break;
            }
        }
        if total > s.aim_total {
            s.aim_total = total;
            driver.set_lock_count(target, total, max, sounds);
        }
        return;
    }
    let state = match mode {
        2 => hud_target::SFSS_OUT_SHOOT_FLOW0,
        3 => hud_target::SFSS_OUT_ATHLETIC_FLOW0,
        _ => hud_target::SFSS_OUT,
    };
    driver.close_scope(target, state);
    s.aim_airstep = mode == 3;
    if mode == 2 {
        s.aim_fired = true;
    } else {
        driver.remove_cursor(target, FOE, s.aim_type, sounds);
        s.aim_marks = 0;
    }
    s.aim_open = false;
}

/// The camera on screen, as a clip matrix over the scene.
fn view_of(game: &Game, rig: Option<&CameraRig>) -> Option<hud_target::View> {
    let rig = rig?;
    let fov0 = rig.camera.vertical_fov();
    let (eye, target, fov) = match game
        .overlay_camera
        .or_else(|| (hooks().camera_view)(game, fov0))
        .or(rig.extra_view)
    {
        Some(v) => (v.eye, v.target, v.fov),
        None => (
            from_world(rig.camera.eye),
            from_world(rig.camera.look_at),
            fov0,
        ),
    };
    let clip = bevy::math::Mat4::perspective_rh(fov, 16. / 9., 1., 100_000.)
        * bevy::math::Mat4::look_at_rh(eye, target, Vec3::Y);
    Some(hud_target::View {
        clip_from_world: glam::Mat4::from_cols_array(&clip.to_cols_array()),
        rect: (0., 0., 1920., 1080.),
    })
}

/// The deck stands: the KH3 ruleset, with KH3's data read.
fn active(game: &Game) -> bool {
    duel_kh3::mine(game) && game.ext.online().kh3_book.is_some()
}

/// Every spell's MP cost, as KH3's Sora reads them: its command rows (255 is all).
fn read_costs() -> Vec<(u8, i32)> {
    use kh3_sora::status::MpCost;
    ORDER
        .iter()
        .flat_map(|&(_, first)| first..first + 3)
        .filter_map(|kind| {
            let unit = kh3_sora::magic::spells::unit(kind)?;
            Some((
                kind,
                match unit.mp {
                    MpCost::All => 255,
                    MpCost::Points(cost) => cost,
                },
            ))
        })
        .collect()
}

fn refresh(game: &Game, deck: &mut Kh3Deck) {
    if deck.costs.is_empty() {
        deck.costs = read_costs();
    }
    let costs = std::mem::take(&mut deck.costs);
    let cost = |kind: u8| costs.iter().find(|c| c.0 == kind).map(|c| c.1);
    let mp = game.sora.player.mp;
    let safety = game.sora.ext.fighter().kh3.abilities.has(Ability::MpSafety);
    deck.magic = magic_rows(&KNOWN, &mp, safety, cost);
    deck.items = game
        .items
        .as_ref()
        .map_or_else(Vec::new, |it| item_rows(&it.slots));
    deck.shortcut = PAGES[deck.page % PAGES.len()]
        .iter()
        .map(|slot| match *slot {
            Shortcut::Empty => Row::empty(),
            Shortcut::Spell(t) => spell_kind(t, KNOWN[usize::from(t)])
                .map_or_else(Row::empty, |kind| spell_row(kind, &mp, safety, &cost)),
            Shortcut::Item(kind) => deck
                .items
                .iter()
                .find(|r| r.kind == kind)
                .cloned()
                .unwrap_or_else(Row::empty),
        })
        .collect();
    deck.costs = costs;
}

fn to_items(rows: &[Row], icon: u8) -> Vec<Item> {
    rows.iter()
        .map(|r| Item {
            label: pause_menu::encode_text(&r.label),
            icon,
            colour: r.colour,
        })
        .collect()
}

/// [Test] The spells `KHC_SPELL_TOUR` casts, Grand Magic last.
const SPELL_TOUR: [u16; 34] = [
    29, 30, 31, 33, 34, 35, 37, 38, 39, 41, 42, 43, 45, 46, 47, 49, 50, 51, 110, 111, 112, 113,
    114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125,
];

/// [Test] Where `script`'s numbers start.
const SCRIPT_FROM: u64 = 100;

/// The row under the last one a list has: the menu lays its rows up from a place
/// a level 99 HP gauge covers, so the row there is empty and no command is hidden.
fn under_gauge() -> Item {
    Item {
        label: Vec::new(),
        icon: 0,
        colour: GREY,
    }
}

/// The deck onto the engine's menu.
fn write(menu: &mut CommandMenu, deck: &Kh3Deck) {
    menu.items.truncate(ROWS);
    menu.items.push(under_gauge());
    menu.opens.truncate(ROWS);
    menu.opens.push(None);
    menu.cursor = deck.cursor.main;
    if let Some(r) = menu.items.get_mut(MAGIC) {
        r.colour = group_colour(&deck.magic);
    }
    if let Some(r) = menu.items.get_mut(ITEMS) {
        r.colour = group_colour(&deck.items);
    }
    match deck.cursor.list {
        Some(list) => {
            let (rows, icon) = if list == MAGIC {
                (&deck.magic, MAGIC_ICON)
            } else {
                (&deck.items, ITEM_ICON)
            };
            if menu.sub.as_ref().is_none_or(|s| s.kind != list) {
                menu.open_sub(list, Vec::new());
            }
            if let Some(sub) = menu.sub.as_mut() {
                sub.items = to_items(rows, icon);
                sub.items.push(under_gauge());
                sub.cursor = deck.cursor.row.min(rows.len().saturating_sub(1));
            }
        }
        None => menu.close_sub(),
    }
}

/// A chosen command goes to KH3's Sora, or from a joiner to the host.
fn execute(game: &mut Game, kind: u8, tick: u64) {
    let slot = game.local_slot;
    let mp = game.sora.player.mp.cur;
    game.event(
        serde_json::json!({"event":"kh3_command","tick":tick,"slot":slot,"kind":kind,
        "name":kh3_sora::magic::spells::command_name(kind),"mp":mp}),
    );
    if game.replica {
        game.ext.online_mut().kh3_pending = Some(if kind >= FIRST_ITEM_KIND {
            rival::Command::Item(kind)
        } else {
            rival::Command::Cast(kind)
        });
    } else {
        game.sora
            .ext
            .fighter_mut()
            .kh3
            .request_cast(u16::from(kind));
    }
}

/// [Test] `--kh3-deck-test`: the presses of the unattended deck run, by the ticks
/// since Sora can act. The list rows it names are the learned order: Fire, Water,
/// Cure, Blizzard, Thunder, Aero. The room's own scripted attacks end at tick 630,
/// and it waits past them.
fn script(since: u64) -> Presses {
    let tick = since + SCRIPT_FROM;
    let press = |up, down, decide, back| Presses {
        up,
        down,
        decide,
        back,
        ..Presses::default()
    };
    match tick {
        // Water: Magic, its second row.
        700 | 730 => press(false, true, false, false),
        715 | 745 => press(false, false, true, false),
        // Thunder: its fifth.
        900 | 930 | 945 | 960 | 975 => press(false, true, false, false),
        915 | 990 => press(false, false, true, false),
        // Items, out with Cancel, in again, the first item.
        1100 | 1115 => press(false, true, false, false),
        1130 | 1145 | 1160 => press(false, false, true, false),
        1138 => press(false, false, false, true),
        // The list again, one Potion fewer, and back round to Attack.
        1200 | 1215 | 1260 => press(false, true, false, false),
        1230 => press(false, false, true, false),
        1245 => press(false, false, false, true),
        // Cure, which takes all the MP and starts MP Charge.
        1300 | 1330 | 1338 => press(false, true, false, false),
        1315 | 1345 => press(false, false, true, false),
        // Magic refused during the charge, and back up to Attack.
        1500 => press(false, true, false, false),
        1515 => press(false, false, true, false),
        1560 => press(true, false, false, false),
        _ => Presses::default(),
    }
}

/// Before the room's tick: the deck's own, with its presses taken from the room.
pub fn before(mut game: ResMut<Game>, hud: Option<ResMut<Hud>>, mut deck: ResMut<Kh3Deck>) {
    let Some(mut hud) = hud else { return };
    let game = &mut *game;
    let deck = &mut *deck;
    let Some(m) = hud.menu.as_mut() else { return };
    if !active(game) {
        if let Some(saved) = deck.saved.take() {
            m.menu.items = saved.items;
            m.menu.opens = saved.opens;
            m.menu.cursor = 0;
            m.menu.close_sub();
            game.shortcuts = saved.shortcuts;
            game.hide_command_menu = false;
            game.hide_gauge = false;
            *deck = Kh3Deck::default();
        }
        return;
    }
    if deck.saved.is_none() {
        deck.saved = Some(Saved {
            items: m.menu.items.clone(),
            opens: m.menu.opens.clone(),
            shortcuts: game.shortcuts,
        });
        let tick = game.tick;
        game.event(serde_json::json!({"event":"kh3_deck","tick":tick}));
    }
    let tick = game.tick;
    deck.flow_pressed = game.flow_queued;
    let o = game.ext.online();
    let frozen = o.duel_frozen || o.duel_idle || o.kh3_loading;
    let scripted = o.kh3_deck_test && game.autoplay.is_some();
    let mut p = Presses {
        up: std::mem::take(&mut game.menu_up_queued),
        down: std::mem::take(&mut game.menu_down_queued),
        left: std::mem::take(&mut game.deck_queued),
        right: std::mem::take(&mut game.open_queued),
        back: std::mem::take(&mut game.back_queued),
        // The room's tick keeps a decide on Attack.
        decide: if deck.cursor.main == 0 && deck.cursor.list.is_none() {
            game.attack_queued
        } else {
            std::mem::take(&mut game.attack_queued)
        },
    };
    if scripted && !frozen {
        let start = *deck.script_start.get_or_insert(tick);
        p = script(tick - start);
        // After the list's tour, a full style gauge offers a command, and Triangle runs it; then
        // low HP, for the alarm and the portrait, and back.
        match (tick - start)
            + if std::env::var_os("KHC_SPELL_TOUR").is_some() {
                100000
            } else {
                0
            } {
            1600 => {
                game.sora.ext.fighter_mut().kh3.core_add_style(1000.);
                for rival in &mut game.rivals {
                    rival.fighter.ext.fighter_mut().kh3.core_add_style(1000.);
                }
            }
            1900 => game.flow_queued = true,
            2200 => {
                let hp = game.sora.vitals.target.max_hp / 5;
                game.sora.vitals.hp = hp;
                game.sora.ext.fighter_mut().kh3.core_set_hp(hp);
            }
            2500 => {
                let hp = game.sora.vitals.target.max_hp;
                game.sora.vitals.hp = hp;
                game.sora.ext.fighter_mut().kh3.core_set_hp(hp);
            }
            _ => {}
        }
    }
    // [Test] `KHC_SPELL_TOUR`: every spell in turn, 300 ticks apart from tick 600.
    // The variable's value is the row to start from.
    if let Some(from) = std::env::var("KHC_SPELL_TOUR")
        .ok()
        .filter(|_| scripted && !frozen)
        .and_then(|v| v.parse::<usize>().ok())
    {
        p = Presses::default();
        if let Some(&kind) = tick
            .is_multiple_of(300)
            .then(|| (tick / 300).checked_sub(2))
            .flatten()
            .and_then(|i| SPELL_TOUR.get(i as usize + from))
        {
            let k = &mut game.sora.ext.fighter_mut().kh3;
            k.core_round_start();
            k.request_cast(kind);
            game.event(serde_json::json!({"event":"spell_tour","tick":tick,"kind":kind}));
        }
    }
    // L1 with up or down turns the shortcut pages.
    if game.pad.held & L1 != 0 {
        let pressed = game.pad.pressed;
        if pressed & UP != 0 {
            deck.page = (deck.page + PAGES.len() - 1) % PAGES.len();
        } else if pressed & DOWN != 0 {
            deck.page = (deck.page + 1) % PAGES.len();
        }
    }
    refresh(game, deck);
    // An empty slot leaves its press to jump and guard, as the room's rule says.
    for (k, row) in deck.shortcut.iter().enumerate().take(4) {
        game.shortcuts[k] = u16::from(row.kind);
    }
    let dead = game.sora.player.dead();
    let aiming = game.sora.ext.fighter().kh3.aim.is_some();
    if (aiming || dead) && deck.cursor.list.is_some() {
        deck.cursor.list = None;
        deck.cursor.row = 0;
    }
    let step = if frozen || dead || aiming {
        Step::None
    } else {
        deck.cursor.press(p, &deck.magic, &deck.items)
    };
    match step {
        Step::Execute(_) => deck.events.push(MenuEvent::SubClosed(CloseKind::Decide)),
        Step::Closed => deck.events.push(MenuEvent::SubClosed(CloseKind::Back)),
        Step::Refused => deck.events.push(MenuEvent::Refused),
        _ => {}
    }
    // KH3's movies ask for their own sounds; KH2's stand in without them.
    let kh2_sounds = deck.shown.is_none();
    let menu_sound = |game: &mut Game, se, name| {
        if kh2_sounds {
            game.sound_queue.push((se, name));
        }
    };
    match step {
        Step::Moved => menu_sound(game, SE_CURSOR, "menu_cursor"),
        Step::Opened => {
            menu_sound(game, SE_OPEN, "menu_open");
            let list = deck.cursor.list;
            let rows = if list == Some(MAGIC) {
                &deck.magic
            } else {
                &deck.items
            };
            let shown: Vec<_> = rows.iter().map(|r| (r.label.as_str(), r.colour)).collect();
            game.event(
                serde_json::json!({"event":"kh3_deck_list","tick":tick,"list":list,
                "rows":shown,"mp":game.sora.player.mp.cur,
                "charging":game.sora.player.mp.charging()}),
            );
        }
        Step::Closed => {
            // Backing out is not also a jump.
            game.jump_queued = false;
            menu_sound(game, SE_CLOSE, "menu_close");
        }
        Step::Refused => {
            menu_sound(game, SE_REFUSED, "menu_denied");
            let shown: Vec<_> = deck
                .magic
                .iter()
                .map(|r| (r.label.as_str(), r.colour))
                .collect();
            let (mp, charging) = (game.sora.player.mp.cur, game.sora.player.mp.charging());
            game.event(serde_json::json!({"event":"kh3_deck_refused","tick":tick,
                "main":deck.cursor.main,"magic":shown,"mp":mp,"charging":charging}));
        }
        Step::Execute(kind) => execute(game, kind, tick),
        Step::Attack | Step::None => {}
    }
    // A shortcut's press: its row, if it can run now.
    let slot = std::mem::take(&mut game.shortcut_queued);
    if slot != 0 && !frozen && !dead {
        let row = deck.shortcut.get(slot.trailing_zeros() as usize).cloned();
        match row {
            Some(r) if r.usable() && r.kind != 0 => {
                deck.events.push(MenuEvent::ShortcutExecuted {
                    style_change: false,
                });
                execute(game, r.kind, tick);
            }
            Some(r) if r.colour == GREY && r.kind != 0 => {
                deck.events.push(MenuEvent::Refused);
                menu_sound(game, SE_REFUSED, "menu_denied");
            }
            _ => {}
        }
    }
    write(&mut m.menu, deck);
}

/// After the room's tick, before the HUD's: the room has coloured the rows by its
/// own rules.
pub fn after(
    mut game: ResMut<Game>,
    hud: Option<ResMut<Hud>>,
    rig: Option<Res<CameraRig>>,
    mut fx: ResMut<kh3_fx::Kh3Fx>,
    mut deck: ResMut<Kh3Deck>,
) {
    // [Test] The script's Triangle stood for one tick.
    if game.ext.online().kh3_deck_test && game.autoplay.is_some() {
        game.flow_queued = false;
    }
    let Some(mut hud) = hud else { return };
    if !active(&game) || deck.saved.is_none() {
        return;
    }
    let Some(m) = hud.menu.as_mut() else { return };
    let deck = &mut *deck;
    refresh(&game, deck);
    write(&mut m.menu, deck);
    let assets = game.ext.online().kh3_hud.clone();
    if deck.shown.is_none()
        && let Some(assets) = assets
    {
        deck.shown = Some(Box::new(Shown::new(&assets)));
        game.hide_command_menu = true;
        game.hide_gauge = true;
    }
    if deck.shown.is_none() {
        return;
    }
    let mut model = model(&game, deck);
    let values = status_values(&game);
    let view = view_of(&game, rig.as_deref());
    let foe = duel_kh3::room_foe(&game).map(|f| f.centre);
    let flow_pressed = deck.flow_pressed;
    let Some(s) = deck.shown.as_mut() else { return };
    let dt = 1. / 60.;
    // A command gone in the tick its button ran it was used, not lost.
    let serials: Vec<u32> = model.situations.iter().map(|e| e.serial).collect();
    if flow_pressed {
        for &serial in s.offered.iter().filter(|o| !serials.contains(o)) {
            model.events.push(MenuEvent::SituationUsed {
                serial,
                form_change: false,
            });
        }
    }
    if s.offered != serials {
        let names: Vec<&str> = model.situations.iter().map(|e| e.name.as_str()).collect();
        let tick = game.tick;
        game.event(serde_json::json!({"event":"kh3_situations","tick":tick,"names":names}));
    }
    s.offered = serials;
    let Shown {
        command,
        display,
        strings,
        sounds,
        ..
    } = &mut **s;
    display.update(command, &model, strings, dt, sounds);
    command.tick();
    aim(&game, s);
    let position_of = |_: u64| foe.map(glam::Vec3::from_array);
    let Shown {
        target,
        driver,
        sounds,
        ..
    } = &mut **s;
    driver.update(target, dt, view.as_ref(), &position_of, sounds);
    target.tick();
    let hp = values.slots[0].hp;
    if hp < s.hp_seen {
        s.status_driver.notify_damage(0);
    }
    s.hp_seen = hp;
    let Shown {
        status,
        status_driver,
        sounds,
        ..
    } = &mut **s;
    status_driver.update(status, &values, dt, sounds);
    status.tick(dt);
    for sound in sounds.drain(..) {
        kh3_fx::play_ui(&mut fx, sound);
        let tick = game.tick;
        game.event(serde_json::json!({"event":"kh3_hud_sound","tick":tick,
            "sound":format!("{sound:?}")}));
    }
}

/// The movies into their images, when they changed, and onto the HUD.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut game: ResMut<Game>,
    camp: Option<Res<CampRes>>,
    settings: Res<settings_room::SettingsRes>,
    mut frame: ResMut<Ui2dFrame>,
    mut images: ResMut<Assets<Image>>,
    mut writes: ResMut<TexelWrites>,
    compose: Option<Res<Compose>>,
    mut deck: ResMut<Kh3Deck>,
) {
    let Some(s) = deck.shown.as_mut() else { return };
    if !game.hide_command_menu || camp.is_some_and(|c| c.menu_up()) || !settings.current.show_hud {
        return;
    }
    let (cx, cy, cw, ch) = COMMAND_REGION;
    let (sx, sy, sw, sh) = STATUS_REGION;
    if s.images.is_none() {
        let mut make = |w: usize, h: usize| {
            let mut image = Image::new(
                bevy::render::render_resource::Extent3d {
                    width: w as u32,
                    height: h as u32,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                vec![0; w * h * 4],
                bevy::render::render_resource::TextureFormat::Rgba8Unorm,
                bevy::asset::RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = bevy::image::ImageSampler::linear();
            let handle = images.add(image);
            let sheet = frame.add_image(handle.clone(), [w as u32, h as u32], false);
            (handle, sheet)
        };
        s.images = Some([
            make(cw, ch),
            make(TARGET_SIZE.0, TARGET_SIZE.1),
            make(sw, sh),
        ]);
    }
    let Some(sheets) = s.images.clone() else {
        return;
    };
    let hash = state_hash(&s.command.movie);
    if hash != s.hashes[0] && !s.command_worker.busy {
        s.hashes[0] = hash;
        s.command_worker.ask(s.command.movie.clone());
    }
    let hash = state_hash(&s.target.movie);
    if hash != s.hashes[1] && !s.target_worker.busy {
        s.hashes[1] = hash;
        s.target_worker.ask(s.target.movie.clone());
    }
    let hash = state_hash(&s.status.movie);
    if hash != s.hashes[2] && !s.status_worker.busy {
        s.hashes[2] = hash;
        s.status_worker.ask(s.status.movie.clone());
    }
    let mut cost = None;
    for (worker, (image, _)) in [
        &mut s.command_worker,
        &mut s.target_worker,
        &mut s.status_worker,
    ]
    .into_iter()
    .zip(&sheets)
    {
        if let Some((rgba, ms)) = worker.take() {
            writes.0.push(TexelWrite {
                image: image.id(),
                rect: [0, 0, worker.size.0 as u32, worker.size.1 as u32],
                rgba,
            });
            cost = Some(cost.unwrap_or(0f32).max(ms));
        }
    }
    // What drawing them costs on its thread, for the log.
    if let Some(ms) = cost {
        s.draw_worst = s.draw_worst.max(ms);
        s.draw_total += ms;
        s.draw_frames += 1;
    }
    if s.draw_frames >= 600 {
        let tick = game.tick;
        let (worst, mean) = (s.draw_worst, s.draw_total / s.draw_frames as f32);
        (s.draw_worst, s.draw_total, s.draw_frames) = (0., 0., 0);
        game.event(
            serde_json::json!({"event":"kh3_hud_cost","tick":tick,"worst_ms":worst,"mean_ms":mean}),
        );
    }
    let part = |x: i32, y: i32, w: usize, h: usize| {
        [
            x as f32 / 1920.,
            y as f32 / 1080.,
            (x as f32 + w as f32) / 1920.,
            (y as f32 + h as f32) / 1080.,
        ]
    };
    frame.hud_part(sheets[1].1, [0., 0., 1., 1.]);
    // Under the room's HUD image, which holds KH2's Sora for the ring.
    frame.hud_part(sheets[2].1, part(sx, sy, sw, sh));
    if let Some(status) = frame.draws.pop() {
        let at = frame
            .draws
            .iter()
            .position(|d| Some(d.0) == compose.as_ref().and_then(|c| c.hud_sheet))
            .unwrap_or(frame.draws.len());
        frame.draws.insert(at, status);
    }
    frame.hud_part(sheets[0].1, part(cx, cy, cw, ch));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cost(kind: u8) -> Option<i32> {
        match kind {
            49..=51 => Some(255),
            29..=47 => Some(20),
            _ => None,
        }
    }

    fn row(kind: u8, colour: usize) -> Row {
        Row {
            kind,
            label: String::new(),
            colour,
            quantity: 0,
        }
    }

    fn magic() -> Vec<Row> {
        vec![row(31, 0), row(43, 2), row(51, 5)]
    }

    #[test]
    fn commands_on_offer_have_their_names() {
        assert_eq!(command_text(112), "Firaza");
        assert_eq!(command_text(grand_of_room(spells::FIRE)), "Firaza");
        assert_eq!(command_text(grand_of_room(spells::THUNDER)), "Thundaza");
        let kind = kh3_sora::enums::ETresCommandKind::from_value(148).map(|c| c.game_name());
        assert_eq!(kind, Some("TRES_CMD_KIND_CHANGE_L1"));
        assert_eq!(command_text(148), "Second Form");
        assert_eq!(command_text(-1), "");
        // No name of its own: the game's enum in words.
        let kind = kh3_sora::enums::ETresCommandKind::from_value(149).map(|c| c.game_name());
        assert_eq!(kind, Some("TRES_CMD_KIND_CHANGE_L2"));
        assert_eq!(command_text(149), "Change L2");
    }

    #[test]
    fn the_list_is_the_learned_order_at_the_tiers_owned() {
        let mut levels = [3; 6];
        levels[usize::from(spells::THUNDER)] = 1;
        levels[usize::from(spells::MAGNET)] = 0;
        let rows = magic_rows(&levels, &Mp::new(100), false, cost);
        let kinds: Vec<u8> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [31, 43, 51, 35, 37]);
        assert_eq!(rows[1].label, "Waterga");
        assert!(rows.iter().all(|r| r.colour == USABLE));
    }

    #[test]
    fn short_mp_recolours_unless_mp_safety_greys_it() {
        let mut mp = Mp::new(100);
        mp.cur = 10;
        let levels = [1; 6];
        let rows = magic_rows(&levels, &mp, false, cost);
        assert_eq!(rows[0].colour, SHORT);
        // Cure takes what is left, so it is never short.
        assert_eq!(rows[2].colour, USABLE);
        assert_eq!(magic_rows(&levels, &mp, true, cost)[0].colour, GREY);
    }

    #[test]
    fn mp_charge_greys_every_spell() {
        let mut mp = Mp::new(100);
        mp.start_charge(10.);
        let rows = magic_rows(&[1; 6], &mp, false, cost);
        assert!(rows.iter().all(|r| r.colour == GREY));
        assert_eq!(group_colour(&rows), GROUP_GREY);
    }

    #[test]
    fn a_spell_the_book_lacks_is_grey() {
        let rows = magic_rows(&[1; 6], &Mp::new(100), false, |_| None);
        assert!(rows.iter().all(|r| r.colour == GREY));
    }

    #[test]
    fn items_are_distinct_with_their_counts() {
        let rows = item_rows(&[1, 0, 3, 1, 500]);
        assert_eq!(
            rows.iter()
                .map(|r| (r.kind, r.label.as_str(), r.quantity))
                .collect::<Vec<_>>(),
            [(58, "Potion", 2), (61, "Ether", 1)]
        );
        assert_eq!(group_colour(&item_rows(&[0, 0])), GREY);
    }

    #[test]
    fn room_items_and_kh3_items_map_both_ways() {
        for (id, kind) in ITEMS_OF_ROOM {
            assert_eq!(item_of_room(id), Some(kind));
            assert_eq!(room_of_item(kind), Some(id));
        }
        assert_eq!(item_of_room(0), None);
        assert_eq!(room_of_item(0), None);
    }

    #[test]
    fn the_main_list_wraps_over_three_rows() {
        let mut c = Cursor::default();
        let up = Presses {
            up: true,
            ..Presses::default()
        };
        assert_eq!(c.press(up, &magic(), &[]), Step::Moved);
        assert_eq!(c.main, 2);
        let down = Presses {
            down: true,
            ..Presses::default()
        };
        c.press(down, &magic(), &[]);
        assert_eq!(c.main, 0);
    }

    #[test]
    fn decide_on_attack_is_the_rooms() {
        let mut c = Cursor::default();
        let decide = Presses {
            decide: true,
            ..Presses::default()
        };
        assert_eq!(c.press(decide, &magic(), &[]), Step::Attack);
        assert_eq!(c, Cursor::default());
    }

    #[test]
    fn a_list_opens_runs_its_row_and_goes_back_to_attack() {
        let m = magic();
        let mut c = Cursor {
            main: MAGIC,
            ..Cursor::default()
        };
        let decide = Presses {
            decide: true,
            ..Presses::default()
        };
        assert_eq!(c.press(decide, &m, &[]), Step::Opened);
        assert_eq!(c.list, Some(MAGIC));
        let down = Presses {
            down: true,
            ..Presses::default()
        };
        c.press(down, &m, &[]);
        assert_eq!(c.press(decide, &m, &[]), Step::Execute(43));
        assert_eq!(c, Cursor::default());
    }

    #[test]
    fn a_grey_row_is_refused_and_the_list_stays() {
        let m = magic();
        let mut c = Cursor {
            main: MAGIC,
            list: Some(MAGIC),
            row: 2,
        };
        let decide = Presses {
            decide: true,
            ..Presses::default()
        };
        assert_eq!(c.press(decide, &m, &[]), Step::Refused);
        assert_eq!(c.list, Some(MAGIC));
    }

    #[test]
    fn cancel_and_left_keep_the_main_cursor() {
        let m = magic();
        for p in [
            Presses {
                back: true,
                ..Presses::default()
            },
            Presses {
                left: true,
                ..Presses::default()
            },
        ] {
            let mut c = Cursor {
                main: MAGIC,
                list: Some(MAGIC),
                row: 1,
            };
            assert_eq!(c.press(p, &m, &[]), Step::Closed);
            assert_eq!(c.main, MAGIC);
            assert_eq!(c.list, None);
        }
    }

    #[test]
    fn right_opens_and_an_empty_list_refuses() {
        let right = Presses {
            right: true,
            ..Presses::default()
        };
        let mut c = Cursor {
            main: ITEMS,
            ..Cursor::default()
        };
        assert_eq!(c.press(right, &magic(), &[]), Step::Refused);
        assert_eq!(c.list, None);
        let items = [row(58, 0)];
        assert_eq!(c.press(right, &magic(), &items), Step::Opened);
    }

    #[test]
    fn spell_kinds_follow_the_level_and_stop_at_the_third_tier() {
        assert_eq!(spell_kind(spells::FIRE, 1), Some(29));
        assert_eq!(spell_kind(spells::MAGNET, 2), Some(46));
        assert_eq!(spell_kind(spells::REFLECT, 9), Some(43));
        assert_eq!(spell_kind(spells::CURE, 0), None);
    }
}
