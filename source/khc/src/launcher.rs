//! The launcher window `KHC` opens without arguments, in the look of
//! KINGDOM HEARTS HD 1.5+2.5 ReMIX's own settings menu (its art and sounds
//! read from the player's install, `collection_menu.rs`). The first launch
//! opens on Quick start: the game found and checked, KINGDOM HEARTS III if
//! there is one, preloading and a name. Then the menu: room codes first, the
//! LAN, practice (KH2 or KH3), replays and Options. Nothing loads into the
//! room until a duel starts.
//!
//! Before a folder is known there are no game files, so the launcher draws
//! plain shapes and a Windows font [launcher design, not the original's
//! menus]. The room runs as a child `KHC room ...` process; this window waits
//! hidden and comes back when it closes.
mod collection_menu;
mod install;

use bevy::app::AppExit;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{FileDragAndDrop, PrimaryWindow, WindowResolution};
use bevy::winit::{UpdateMode, WinitSettings};
use collection_menu::{CollectionMenu, Cue};
use engine::launcher::paint::{self, Face, Paint, quad, rgba};
use engine::launcher::{
    FAINT, GOLD, Job, MARGIN, MUTED, Running, TEXT, check_screen, failure_lines, plain, spawn,
    start_check, triangle, wrap,
};
use install::{Bound, Settings};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;
use visual::overlay::{Ui2dFrame, Ui2dPlugin};

pub use install::data_dir;

/// The menu's layout in the 1280x720 design space [launcher layout after
/// the 1.5+2.5 settings menu: title band top left, bars down the left, each
/// row's setting to its right, the cursor's description at the bottom].
const BANNER: [f32; 4] = [0., 34., 560., 40.];
const ROW_X: f32 = 96.;
const ROW_TOP: f32 = 120.;
const ROW_W: f32 = 420.;
const ROW_H: f32 = 40.;
const ROW_GAP: f32 = 8.;
const ROW_BOTTOM: f32 = 600.;
const VALUE_X: f32 = ROW_X + ROW_W + 40.;
const DESCRIPTION_Y: f32 = 622.;
const INK: u32 = 0xe8e2d0;
const INK_DIM: u32 = 0x8a8474;
const BANNER_INK: u32 = 0x2a2412;

#[derive(Clone, Debug, PartialEq)]
enum Action {
    UseFolder(PathBuf),
    /// Browse into a folder (None: the drive list).
    Browse(Option<PathBuf>),
    Pick,
    /// Browse for KINGDOM HEARTS III (None: the drive list).
    BrowseKh3(Option<PathBuf>),
    /// Check this folder and remember it as KINGDOM HEARTS III.
    UseKh3(PathBuf),
    /// Quick start again.
    QuickStart,
    /// Leave the KH3 folder list for where it was opened from.
    Return,
    /// Leave Quick start for the menu (the game checked first).
    QuickDone,
    /// The menu, the local network, Options.
    Main,
    Lan,
    Options,
    /// A row that only informs.
    Nothing,
    /// Host a duel on this network and wait for a player.
    HostDuel,
    /// Practice: pick the ruleset.
    Practice,
    /// [KHC-060] The KH2 ruleset on this PC against a training dummy.
    PracticeKh2,
    /// The KH3 ruleset on this PC against a training dummy.
    TestKh3,
    /// Join a host the LAN search found (index into the found list).
    JoinFound(usize),
    /// Type the host's address, then join it.
    JoinAddress,
    /// Host a duel with a room code: from the relay when one is set, else this
    /// PC's own address (KHC-051).
    HostRoom,
    /// Type a room code, then join it.
    JoinRoom,
    /// Type the relay server's address.
    EditRelay,
    /// [KHC-058] Preload on or off.
    TogglePreload,
    /// Type a rules code to host with.
    EditCode,
    /// Type the name the other player sees.
    EditName,
    /// [KHC-044] Show the profile's recovery code.
    ShowRecovery,
    /// [KHC-044] Type a recovery code to become that player here.
    RestoreCode,
    /// [KHC-044] A new id and secret (pressed twice).
    ResetIdentity,
    /// [KHC-044] Delete profile.json (pressed twice).
    DeleteProfile,
    /// The next devices choice for duels (two windows on one PC).
    CycleInput,
    /// The saved duels.
    Replays,
    /// Play this saved duel.
    PlayReplay(PathBuf),
    Quit,
}

/// A text field being typed into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Name,
    Address,
    Relay,
    Room,
    Code,
    /// [KHC-044] A recovery code from another PC.
    Recovery,
}

/// Longest name, address and room code typed.
const NAME_LIMIT: usize = crate::names::MAX_CHARS;
const ADDRESS_LIMIT: usize = 40;
const ROOM_LIMIT: usize = net::direct::DIRECT_CODE_LENGTH + 2;
/// Longest rules code typed, its dashes included.
const RULES_CODE_LIMIT: usize = 72;

#[derive(Clone)]
struct Item {
    label: String,
    /// The setting shown right of the bar.
    value: Option<String>,
    /// The line at the bottom while the cursor is on it.
    detail: Option<String>,
    /// A gold tag at the bar's end ("STEAM", "KH2FM").
    tag: Option<&'static str>,
    action: Option<Action>,
}

impl Item {
    fn new(label: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            value: None,
            detail: None,
            tag: None,
            action: Some(action),
        }
    }
    fn value(mut self, v: impl Into<String>) -> Self {
        self.value = Some(v.into());
        self
    }
    fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    fn tag(mut self, t: &'static str) -> Self {
        self.tag = Some(t);
        self
    }
    fn disabled(mut self) -> Self {
        self.action = None;
        self
    }
}

/// Where a KH3 folder chosen returns to.
#[derive(Clone)]
enum Back {
    Quick(Option<Bound>),
    Options(Bound),
}

enum Screen {
    Boot,
    /// The game (once checked), KH3, preloading and the name.
    QuickStart(Option<Bound>),
    Pick,
    Browse(Option<PathBuf>),
    BrowseKh3(Option<PathBuf>, Back),
    Check(Job),
    /// Room codes, LAN, practice, replays, Options.
    Main(Bound),
    /// Host on this network, the hosts found, join by address.
    Lan(Bound),
    /// [KHC-060] Practice: KH2 or KH3 against the training dummy.
    Practice(Bound),
    Options(Bound),
    /// The saved duels, newest first.
    Replays(Bound),
    Loading(Running, Bound),
    Problem {
        title: String,
        lines: Vec<String>,
        retry: Option<Bound>,
    },
}

#[derive(Resource)]
struct Launcher {
    dir: PathBuf,
    settings: Settings,
    supported: Option<String>,
    detected: Vec<PathBuf>,
    screen: Screen,
    items: Vec<Item>,
    cursor: usize,
    scroll: usize,
    now: f32,
    fade: f32,
    hover_drop: bool,
    repeat: Option<(i32, f32)>,
    mouse: Option<Vec2>,
    paint: Option<Paint>,
    /// The 1.5+2.5 menu's art and sounds.
    menu: CollectionMenu,
    /// A folder check started from Quick start returns there.
    check_to_quick: bool,
    /// The field being typed into and its text so far.
    editing: Option<(Field, String)>,
    /// The local network screen's search and the hosts it found.
    browser: Option<net::Browser>,
    found: Vec<net::Found>,
    /// This executable's identity (hosts with another are not joinable).
    build: Option<[u8; 32]>,
    /// The checked KH3 install, when one is set; what was wrong otherwise.
    kh3: Result<kh3_data::Install, String>,
    /// How the last duel's link went: Direct or Relay, and its round trip.
    last_link: Option<String>,
    /// The logo (`assets/khc-logo.png`), once it has been looked for.
    logo: Option<Option<(usize, [u32; 2])>>,
    /// [KHC-044] The player's profile (profile.rs), or why it could not be read.
    profile: Result<crate::profile::Profile, String>,
    /// The profile row's last word (a refused name, a shown code), and a
    /// press that waits for a second one to confirm.
    profile_note: Option<String>,
    confirm: Option<Action>,
}

impl Launcher {
    fn go(&mut self, screen: Screen) {
        // The LAN search runs only on the local network screen.
        if matches!(screen, Screen::Lan(_)) {
            if self.browser.is_none() {
                let build = *self
                    .build
                    .get_or_insert_with(|| crate::room::build_identity().unwrap_or([0; 32]));
                self.browser = net::Browser::new(net::DEFAULT_PORT, build)
                    .inspect_err(|e| eprintln!("LAN search unavailable: {e}"))
                    .ok();
            }
        } else {
            self.browser = None;
            self.found.clear();
        }
        self.editing = None;
        self.screen = screen;
        self.items = self.build_items();
        self.cursor = self
            .items
            .iter()
            .position(|i| i.action.is_some())
            .unwrap_or(0);
        self.scroll = 0;
        self.repeat = None;
    }

    /// The checked game, on the screens that have one.
    fn bound(&self) -> Option<Bound> {
        match &self.screen {
            Screen::Main(b)
            | Screen::Lan(b)
            | Screen::Practice(b)
            | Screen::Options(b)
            | Screen::Replays(b)
            | Screen::QuickStart(Some(b)) => Some(b.clone()),
            _ => None,
        }
    }

    /// A field's text with the caret while typed into.
    fn typing(&self, field: Field) -> Option<String> {
        match &self.editing {
            Some((f, text)) if *f == field => Some(format!("{text}_")),
            _ => None,
        }
    }

    fn build_items(&self) -> Vec<Item> {
        let on_off = |on: bool| if on { "On" } else { "Off" };
        let preload = self.settings.preload == Some(true);
        let preload_row = Item::new("Preload", Action::TogglePreload)
            .value(on_off(preload))
            .detail(if preload {
                "A duel loads everything before play, so nothing stutters when it first appears"
            } else {
                "A duel starts at once; things load the first time they are needed"
            });
        let name_row = Item::new("Name", Action::EditName)
            .value(self.typing(Field::Name).unwrap_or_else(|| self.name_tag()))
            .detail(self.name_detail());
        match &self.screen {
            Screen::QuickStart(b) => {
                let folder = b
                    .as_ref()
                    .map(|b| b.folder.clone())
                    .or_else(|| self.settings.game_folder.clone())
                    .or_else(|| self.detected.first().cloned());
                let game = Item::new("KINGDOM HEARTS HD 1.5+2.5 ReMIX", Action::Pick);
                let game = match (b, &folder) {
                    (Some(b), _) => game.value("Ready").detail(plain(&b.folder)),
                    (None, Some(f)) => game.value("Found").detail(plain(f)),
                    (None, None) => game
                        .value("Not found")
                        .detail("Choose the folder it is installed in"),
                };
                let kh3 = Item::new("KINGDOM HEARTS III", Action::BrowseKh3(None));
                let kh3 = match &self.kh3 {
                    Ok(install) => kh3.value("Found").detail(plain(&install.root)),
                    Err(_) => kh3
                        .value("Optional")
                        .detail("For the KH3 ruleset and KH3 practice; both players need it"),
                };
                let start = Item::new("Start", Action::QuickDone)
                    .detail("Check the game once, then open the menu");
                vec![
                    game,
                    kh3,
                    preload_row,
                    name_row,
                    if folder.is_some() {
                        start
                    } else {
                        start.disabled()
                    },
                    Item::new("Quit", Action::Quit),
                ]
            }
            Screen::Pick => {
                let mut items: Vec<Item> = self
                    .detected
                    .iter()
                    .map(|p| {
                        let current = self.settings.game_folder.as_ref() == Some(p);
                        Item::new(
                            if current {
                                "Steam install (current)"
                            } else {
                                "Steam install"
                            },
                            Action::UseFolder(p.clone()),
                        )
                        .detail(plain(p))
                        .tag("STEAM")
                    })
                    .collect();
                items.push(
                    Item::new("Browse for the folder", Action::Browse(None))
                        .detail("Or drop the game folder on this window"),
                );
                if let Some(remembered) = &self.settings.game_folder
                    && !self.detected.contains(remembered)
                {
                    items.push(
                        Item::new("Current folder", Action::UseFolder(remembered.clone()))
                            .detail(plain(remembered)),
                    );
                }
                items.push(Item::new("Quick start", Action::QuickStart));
                items.push(Item::new("Quit", Action::Quit));
                items
            }
            Screen::Browse(path) => browse_items(path.as_deref()),
            Screen::Main(_) => {
                let room = self
                    .typing(Field::Room)
                    .or_else(|| self.settings.last_room.clone());
                let mut join = Item::new("Join a Room", Action::JoinRoom)
                    .detail("Type the code a friend sent you, then Enter");
                if let Some(room) = room {
                    join = join.value(room);
                }
                vec![
                    join,
                    Item::new("Host a Room", Action::HostRoom).detail(
                        if self.settings.relay_server.is_some() {
                            "Get a code to send a friend on another network, through your relay"
                        } else {
                            "Get a code to send a friend on another network; no server needed"
                        },
                    ),
                    Item::new("LAN Host", Action::Lan)
                        .detail("Host or join a player on this network"),
                    Item::new("Practice", Action::Practice)
                        .detail("Try a ruleset's combat against a training dummy"),
                    Item::new("Replays", Action::Replays).detail("Watch a saved duel"),
                    Item::new("Options", Action::Options).detail(
                        "Name, controls, preloading, rules, servers, game folders, profile",
                    ),
                    Item::new("Quit", Action::Quit).detail("Close Kingdom Hearts: Connected"),
                ]
            }
            Screen::Lan(_) => {
                let mut items =
                    vec![
                        Item::new("Host on this network", Action::HostDuel).detail(format!(
                            "Wait for a player on this network. This is version {}",
                            crate::room::duel_net::VERSION
                        )),
                    ];
                for (i, host) in self.found.iter().enumerate() {
                    let item =
                        Item::new(format!("Join {}", host.name), Action::JoinFound(i)).tag("LAN");
                    items.push(if !host.compatible {
                        item.value(format!(
                            "Version {}, and this is {}",
                            host.version,
                            crate::room::duel_net::VERSION
                        ))
                        .disabled()
                    } else if !host.open {
                        item.value("Already in a duel").disabled()
                    } else {
                        item.value(match host.ping_ms {
                            Some(ms) => format!("{ms} ms"),
                            None => String::new(),
                        })
                        .detail(host.addr.ip().to_string())
                    });
                }
                if self.found.is_empty() && self.browser.is_some() {
                    items.push(Item::new("Looking for hosts...", Action::Nothing).disabled());
                }
                let address = self
                    .typing(Field::Address)
                    .or_else(|| self.settings.last_host.clone());
                let mut by_address = Item::new("Join by address", Action::JoinAddress)
                    .detail("Type the host's IP address, then Enter");
                if let Some(a) = address {
                    by_address = by_address.value(a);
                }
                items.push(by_address);
                items.push(Item::new("Back", Action::Main));
                items
            }
            Screen::Options(b) => {
                let code = self
                    .typing(Field::Code)
                    .or_else(|| self.settings.duel_code.clone());
                let relay = self
                    .typing(Field::Relay)
                    .or_else(|| self.settings.relay_server.clone());
                let mut items = vec![
                    name_row,
                    Item::new("Controls", Action::CycleInput)
                        .value(input_label(self.duel_input()))
                        .detail("The devices a duel reads"),
                    preload_row,
                    Item::new("Rules code", Action::EditCode)
                        .value(code.unwrap_or_else(|| "None".into()))
                        .detail("Host with these rules; none keeps the rules from last time"),
                    Item::new("Relay server", Action::EditRelay)
                        .value(relay.unwrap_or_else(|| "None".into()))
                        .detail("Room codes go through it when set; empty for direct codes"),
                    self.kh3_row(),
                    Item::new("Game folder", Action::Pick)
                        .value("KH2FM")
                        .detail(plain(&b.folder)),
                    Item::new("Quick start", Action::QuickStart)
                        .detail("Find the games and choose the settings again"),
                    Item::new("Recovery code", Action::ShowRecovery)
                        .detail("Restores this player on a new PC; keep it to yourself"),
                    Item::new("Restore from a code", Action::RestoreCode).detail(
                        self.typing(Field::Recovery)
                            .unwrap_or_else(|| "Type the recovery code from your other PC".into()),
                    ),
                    Item::new("Reset identity", Action::ResetIdentity).detail(
                        if self.confirm == Some(Action::ResetIdentity) {
                            "Press again: a new player number"
                        } else {
                            "A new player id"
                        },
                    ),
                    Item::new("Delete my profile", Action::DeleteProfile).detail(
                        if self.confirm == Some(Action::DeleteProfile) {
                            "Press again to delete it"
                        } else {
                            "Removes the profile file from this PC"
                        },
                    ),
                ];
                if let Some(link) = &self.last_link {
                    items.push(
                        Item::new("Last duel's link", Action::Nothing)
                            .value(link.clone())
                            .disabled(),
                    );
                }
                items.push(Item::new("Back", Action::Main));
                items
            }
            Screen::Practice(_) => vec![
                Item::new("KINGDOM HEARTS II", Action::PracticeKh2)
                    .detail("KH2 Final Mix combat against a training dummy"),
                match &self.kh3 {
                    Ok(_) => Item::new("KINGDOM HEARTS III", Action::TestKh3)
                        .detail("KH3 combat against a training dummy; START picks KH3 abilities"),
                    Err(why) => Item::new("KINGDOM HEARTS III", Action::TestKh3)
                        .value("Needs KH3")
                        .detail(format!("Needs your KINGDOM HEARTS III: {why}"))
                        .disabled(),
                },
                Item::new("Back", Action::Main),
            ],
            Screen::Replays(b) => replay_items(b),
            Screen::BrowseKh3(path, back) => browse_kh3_items(path.as_deref(), back),
            Screen::Problem { retry, .. } => {
                let mut items = Vec::new();
                if let Some(b) = retry {
                    items.push(Item::new(
                        "Back to the menu",
                        Action::UseFolder(b.folder.clone()),
                    ));
                }
                items.push(Item::new("Choose another folder", Action::Pick));
                items.push(Item::new("Quit", Action::Quit));
                items
            }
            Screen::Boot | Screen::Check(_) | Screen::Loading(..) => Vec::new(),
        }
    }
}

impl Launcher {
    /// The row that sets KINGDOM HEARTS III: the KH3 ruleset needs it.
    fn kh3_row(&self) -> Item {
        let item = Item::new("KINGDOM HEARTS III", Action::BrowseKh3(None));
        match &self.kh3 {
            Ok(install) => item.value("Found").detail(plain(&install.root)),
            Err(why) => item.value("Not set").detail(why.clone()),
        }
    }
}

/// Drives and folders, with the folder holding KINGDOM HEARTS III marked.
fn browse_kh3_items(path: Option<&Path>, _back: &Back) -> Vec<Item> {
    let leave = || Action::Return;
    let Some(path) = path else {
        let mut items: Vec<Item> = ('A'..='Z')
            .map(|d| PathBuf::from(format!("{d}:\\")))
            .filter(|p| p.is_dir())
            .map(|p| Item::new(plain(&p), Action::BrowseKh3(Some(p))))
            .collect();
        items.push(Item::new("Back", leave()));
        return items;
    };
    let mut items = Vec::new();
    let here = Item::new("Use this folder", Action::UseKh3(path.to_path_buf()));
    items.push(if kh3_data::locate(path).is_some() {
        here.detail("KINGDOM HEARTS III is here").tag("KH3")
    } else {
        here.detail("KINGDOM HEARTS III is not in this folder")
            .disabled()
    });
    items.push(Item::new(
        "Up one level",
        Action::BrowseKh3(path.parent().map(Path::to_path_buf)),
    ));
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    for d in dirs {
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        items.push(Item::new(name, Action::BrowseKh3(Some(d))));
    }
    items
}

fn browse_items(path: Option<&Path>) -> Vec<Item> {
    let Some(path) = path else {
        let mut items: Vec<Item> = ('A'..='Z')
            .map(|d| PathBuf::from(format!("{d}:\\")))
            .filter(|p| p.is_dir())
            .map(|p| Item::new(plain(&p), Action::Browse(Some(p))))
            .collect();
        items.push(Item::new("Back", Action::Pick));
        return items;
    };
    let mut items = Vec::new();
    let here = Item::new("Use this folder", Action::UseFolder(path.to_path_buf()));
    items.push(if install::has_executable(path) {
        here.detail("The game is here").tag("KH2FM")
    } else {
        here.detail("The game is not in this folder").disabled()
    });
    items.push(Item::new(
        "Up one level",
        Action::Browse(path.parent().map(Path::to_path_buf)),
    ));
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    for d in dirs {
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let item = Item::new(name, Action::Browse(Some(d.clone())));
        items.push(if install::has_executable(&d) {
            item.tag("KH2FM")
        } else {
            item
        });
    }
    items
}

#[derive(Default)]
struct Pressed {
    up: bool,
    down: bool,
    page: i32,
    accept: bool,
    back: bool,
    change: bool,
}

fn read_input(
    l: &mut Launcher,
    keys: &ButtonInput<KeyCode>,
    pads: &Query<&Gamepad>,
    wheel: f32,
) -> Pressed {
    let mut p = Pressed {
        accept: keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]),
        back: keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]),
        change: keys.just_pressed(KeyCode::F2),
        page: keys.just_pressed(KeyCode::PageDown) as i32
            - keys.just_pressed(KeyCode::PageUp) as i32,
        ..default()
    };
    let mut held = keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) as i32
        - keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) as i32;
    for g in pads.iter() {
        p.accept |= g.just_pressed(GamepadButton::South);
        p.back |= g.just_pressed(GamepadButton::East);
        p.change |= g.just_pressed(GamepadButton::North);
        let stick = g.left_stick().y;
        let dir = g.pressed(GamepadButton::DPadDown) as i32
            - g.pressed(GamepadButton::DPadUp) as i32
            + (stick < -0.5) as i32
            - (stick > 0.5) as i32;
        if held == 0 {
            held = dir.signum();
        }
        p.page += g.just_pressed(GamepadButton::RightTrigger) as i32
            - g.just_pressed(GamepadButton::LeftTrigger) as i32;
    }
    // Held directions repeat after a pause.
    let step = match (held, l.repeat) {
        (0, _) => {
            l.repeat = None;
            0
        }
        (d, Some((was, at))) if was == d => {
            if l.now >= at {
                l.repeat = Some((d, l.now + 0.075));
                d
            } else {
                0
            }
        }
        (d, _) => {
            l.repeat = Some((d, l.now + 0.38));
            d
        }
    };
    // A tap shorter than a frame still moves once.
    let tap = keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) as i32
        - keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) as i32;
    let step = if step == 0 { tap } else { step };
    let wheel_step = if wheel > 0. {
        -1
    } else if wheel < 0. {
        1
    } else {
        0
    };
    p.up = step < 0 || wheel_step < 0;
    p.down = step > 0 || wheel_step > 0;
    p
}

fn visible_rows(l: &Launcher) -> usize {
    (((ROW_BOTTOM - list_top(l) + ROW_GAP) / (ROW_H + ROW_GAP)) as usize).max(1)
}

fn row_rect(l: &Launcher, i: usize) -> [f32; 4] {
    let k = i as f32 - l.scroll as f32;
    [ROW_X, list_top(l) + k * (ROW_H + ROW_GAP), ROW_W, ROW_H]
}

fn move_cursor(l: &mut Launcher, by: i32) {
    let n = l.items.len() as i32;
    if n == 0 {
        return;
    }
    let mut c = l.cursor as i32;
    for _ in 0..n {
        c = (c + by.signum()).rem_euclid(n);
        if l.items[c as usize].action.is_some() {
            break;
        }
    }
    l.cursor = c as usize;
    keep_visible(l);
}

fn keep_visible(l: &mut Launcher) {
    let rows = visible_rows(l);
    if l.cursor < l.scroll {
        l.scroll = l.cursor;
    } else if l.cursor >= l.scroll + rows {
        l.scroll = l.cursor + 1 - rows;
    }
}

fn act(l: &mut Launcher, action: Action, exit: &mut MessageWriter<AppExit>) {
    if !matches!(action, Action::ResetIdentity | Action::DeleteProfile) {
        l.confirm = None;
    }
    match action {
        Action::Quit => {
            exit.write(AppExit::Success);
        }
        Action::Pick => {
            if matches!(l.screen, Screen::QuickStart(_)) {
                l.check_to_quick = true;
            } else if !matches!(l.screen, Screen::Browse(_)) {
                l.check_to_quick = false;
            }
            l.go(Screen::Pick);
        }
        Action::QuickStart => {
            let b = l.bound();
            l.go(Screen::QuickStart(b));
        }
        Action::QuickDone => {
            l.settings.guide_seen = true;
            save_settings(l);
            let Screen::QuickStart(b) = &l.screen else {
                return;
            };
            if let Some(b) = b.clone() {
                return l.go(Screen::Main(b));
            }
            let folder = l
                .settings
                .game_folder
                .clone()
                .or_else(|| l.detected.first().cloned());
            if let Some(folder) = folder {
                l.check_to_quick = false;
                act(l, Action::UseFolder(folder), exit);
            }
        }
        Action::Main => {
            if let Some(b) = l.bound() {
                l.go(Screen::Main(b));
            }
        }
        Action::Lan => {
            if let Some(b) = l.bound() {
                l.go(Screen::Lan(b));
            }
        }
        Action::Options => {
            if let Some(b) = l.bound() {
                l.go(Screen::Options(b));
            }
        }
        Action::Browse(path) => {
            let from = match &l.screen {
                Screen::Browse(Some(p)) => Some(p.clone()),
                _ => None,
            };
            l.go(Screen::Browse(path));
            // Going up lands on the folder just left.
            if let Some(from) = from
                && let Some(i) = l
                    .items
                    .iter()
                    .position(|it| it.action == Some(Action::Browse(Some(from.clone()))))
            {
                l.cursor = i;
                keep_visible(l);
            }
        }
        Action::BrowseKh3(path) => {
            let back = match &l.screen {
                Screen::Options(b) => Back::Options(b.clone()),
                Screen::QuickStart(b) => Back::Quick(b.clone()),
                Screen::BrowseKh3(_, back) => back.clone(),
                _ => Back::Quick(None),
            };
            l.go(Screen::BrowseKh3(path, back));
        }
        Action::Return => {
            if let Screen::BrowseKh3(_, back) = &l.screen {
                let back = back.clone();
                go_back(l, back);
            }
        }
        Action::UseKh3(path) => {
            let Screen::BrowseKh3(_, back) = &l.screen else {
                return;
            };
            let back = back.clone();
            match kh3_data::admit(&path) {
                Ok(install) => {
                    l.settings.kh3_folder = Some(install.root.clone());
                    l.settings.kh3_key_offset = None;
                    save_settings(l);
                    l.kh3 = Ok(install);
                    go_back(l, back);
                }
                Err(e) => l.go(Screen::Problem {
                    title: "That KINGDOM HEARTS III folder can't be used".into(),
                    lines: vec![e.to_string()],
                    retry: match back {
                        Back::Options(b) => Some(b),
                        Back::Quick(b) => b,
                    },
                }),
            }
        }
        Action::UseFolder(folder) => {
            // A library or `common` folder picked by mistake: use the game
            // folder inside it.
            let folder = if install::has_executable(&folder) {
                folder
            } else {
                install::find_game_below(&folder).unwrap_or(folder)
            };
            let hint = (l.settings.game_folder.as_deref() == Some(folder.as_path()))
                .then(|| l.settings.manifest.clone())
                .flatten();
            l.menu.load(&folder);
            let job = start_check(folder, l.dir.clone(), hint);
            l.go(Screen::Check(job));
        }
        Action::Nothing => {}
        Action::HostDuel => {
            let name = l.player_name();
            duel(l, vec!["--host".into(), "--name".into(), name]);
        }
        Action::Practice => {
            if let Some(b) = l.bound() {
                l.go(Screen::Practice(b));
            }
        }
        Action::PracticeKh2 => duel(l, vec!["--kh2-test".into()]),
        Action::TestKh3 => {
            if l.kh3.is_ok() {
                duel(l, vec!["--kh3-test".into()]);
            }
        }
        Action::JoinFound(i) => {
            if let Some(host) = l.found.get(i) {
                let address = host.addr.to_string();
                let name = l.player_name();
                duel(l, vec!["--join".into(), address, "--name".into(), name]);
            }
        }
        Action::JoinAddress => {
            let text = l.settings.last_host.clone().unwrap_or_default();
            l.edit(Field::Address, text);
        }
        Action::EditName => {
            l.profile_note = None;
            let text = l.player_name();
            l.edit(Field::Name, text);
        }
        Action::RestoreCode => {
            l.profile_note = None;
            l.edit(Field::Recovery, String::new());
        }
        Action::ShowRecovery => {
            l.profile_note = l
                .profile
                .as_ref()
                .ok()
                .and_then(|p| p.recovery_code())
                .map(|c| format!("Recovery code {c}"));
            l.refresh();
        }
        Action::ResetIdentity | Action::DeleteProfile if l.confirm != Some(action.clone()) => {
            l.confirm = Some(action);
            l.refresh();
        }
        Action::ResetIdentity => {
            l.confirm = None;
            match crate::profile::new_secret() {
                Ok(secret) => {
                    if let Ok(p) = &mut l.profile {
                        p.reset_identity(secret);
                    }
                    l.save_profile();
                    l.profile_note = Some(format!("Now {}", l.name_tag()));
                }
                Err(e) => l.profile_note = Some(e),
            }
            l.refresh();
        }
        Action::DeleteProfile => {
            l.confirm = None;
            l.profile_note = Some(match crate::profile::delete(&l.dir) {
                // A new one is made at the next start.
                Ok(()) => "Deleted. A new profile starts next time".into(),
                Err(e) => format!("Not deleted: {e}"),
            });
            l.refresh();
        }
        Action::HostRoom => {
            let name = l.player_name();
            // [KHC-051] With no relay, the code is this PC's own address.
            let relay = match l.settings.relay_server.clone() {
                Some(relay) => vec!["--relay".into(), relay],
                None => Vec::new(),
            };
            let mut args = vec!["--host".into()];
            args.extend(relay);
            args.extend(["--room".into(), "--name".into(), name]);
            duel(l, args);
        }
        Action::JoinRoom => {
            let text = l.settings.last_room.clone().unwrap_or_default();
            l.edit(Field::Room, text);
        }
        Action::EditRelay => {
            let text = l.settings.relay_server.clone().unwrap_or_default();
            l.edit(Field::Relay, text);
        }
        Action::TogglePreload => {
            l.settings.preload = Some(l.settings.preload != Some(true));
            save_settings(l);
            l.refresh();
        }
        Action::EditCode => {
            let text = l.settings.duel_code.clone().unwrap_or_default();
            l.edit(Field::Code, text);
        }
        Action::Replays => {
            if let Some(b) = l.bound() {
                l.go(Screen::Replays(b));
            }
        }
        Action::PlayReplay(path) => {
            let Screen::Replays(b) = &l.screen else {
                return;
            };
            let b = b.clone();
            let own = l.kh3.as_ref().ok().map(|i| i.identity);
            let verdict = crate::room::duel_replay::Replay::read(&path).and_then(|r| {
                crate::room::duel_replay::check(&r.header, net::wire::PROTOCOL, own, None)
            });
            match verdict {
                Ok(_) => duel(l, vec!["--replay".into(), path.display().to_string()]),
                Err(why) => l.go(Screen::Problem {
                    title: "That replay can't be played".into(),
                    lines: vec![why],
                    retry: Some(b),
                }),
            }
        }
        Action::CycleInput => {
            let at = INPUT_CHOICES
                .iter()
                .position(|&c| c == l.duel_input())
                .unwrap_or(0);
            l.settings.duel_input = Some(INPUT_CHOICES[(at + 1) % INPUT_CHOICES.len()].into());
            save_settings(l);
            l.refresh();
        }
    }
}

/// Back from the KH3 folder list to where it was opened.
fn go_back(l: &mut Launcher, back: Back) {
    match back {
        Back::Options(b) => l.go(Screen::Options(b)),
        Back::Quick(b) => l.go(Screen::QuickStart(b)),
    }
}

fn save_settings(l: &Launcher) {
    if let Err(e) = install::save_settings(&l.dir, &l.settings) {
        eprintln!("Launcher settings not saved: {e}");
    }
}

/// The replays folder's files, newest first, by who fought.
fn replay_items(b: &Bound) -> Vec<Item> {
    use crate::room::duel_replay::{EXTENSION, Replay, folder};
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(folder())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == EXTENSION))
        .map(|e| {
            let at = e.metadata().and_then(|m| m.modified());
            (at.unwrap_or(std::time::UNIX_EPOCH), e.path())
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut items: Vec<Item> = files
        .into_iter()
        .take(REPLAY_ROWS)
        .map(|(_, path)| {
            let name = plain(&path);
            let label = match Replay::read(&path) {
                Ok(r) => format!("{} vs {}", r.header.host_name, r.header.join_name),
                Err(_) => "Unreadable replay".to_owned(),
            };
            Item::new(label, Action::PlayReplay(path)).detail(name)
        })
        .collect();
    if items.is_empty() {
        items.push(Item::new("No duels saved yet", Action::Nothing).disabled());
    }
    let _ = b;
    items.push(Item::new("Back", Action::Main));
    items
}

/// The most replays listed.
const REPLAY_ROWS: usize = 30;

/// The duel's device choices (the room's `--input`), in the order Controls
/// cycles them.
const INPUT_CHOICES: [&str; 4] = ["auto", "kbm", "pad", "both"];

fn input_label(choice: &str) -> &'static str {
    match choice {
        "kbm" => "Keyboard and mouse",
        "pad" => "Controller",
        "both" => "Keyboard, mouse and controller",
        _ => "Auto: on one PC, the host keyboard and the joiner a controller",
    }
}

impl Launcher {
    /// The name the other player sees: the saved one, else the Windows
    /// user name.
    /// The duel's devices as saved (auto by default).
    fn duel_input(&self) -> &str {
        self.settings
            .duel_input
            .as_deref()
            .filter(|c| INPUT_CHOICES.contains(c))
            .unwrap_or("auto")
    }

    /// [KHC-044] The profile's name; never the Windows user name.
    fn player_name(&self) -> String {
        match &self.profile {
            Ok(p) => p.name.clone(),
            Err(_) => "Player".into(),
        }
    }

    /// The Name row: the name and its number, the rules, and the last word.
    fn name_detail(&self) -> String {
        if let Some(note) = &self.profile_note {
            return note.clone();
        }
        match &self.profile {
            Ok(p) if !p.name_confirmed => {
                format!("{}: a default name; press to choose your own", p.tagged())
            }
            Ok(p) => format!("{}  3 to 16 letters, digits, space, _ or -", p.tagged()),
            Err(e) => format!("The profile could not be read: {e}"),
        }
    }

    fn name_tag(&self) -> String {
        self.profile
            .as_ref()
            .map(|p| p.tagged())
            .unwrap_or_default()
    }

    fn save_profile(&mut self) {
        if let Ok(p) = &self.profile
            && let Err(e) = crate::profile::save(&self.dir, p)
        {
            eprintln!("Profile not saved: {e}");
        }
    }

    fn edit(&mut self, field: Field, text: String) {
        self.editing = Some((field, text));
        self.refresh();
    }

    /// Rebuilds the rows in place (the cursor stays).
    fn refresh(&mut self) {
        self.items = self.build_items();
        self.cursor = self.cursor.min(self.items.len().saturating_sub(1));
    }
}

/// Typing into a field: characters, Backspace, Enter to keep, Esc to drop.
/// Returns true while the field takes the keys.
fn type_text(
    l: &mut Launcher,
    keys: &ButtonInput<KeyCode>,
    typed: &mut MessageReader<KeyboardInput>,
) -> bool {
    let Some((field, mut text)) = l.editing.take() else {
        typed.clear();
        return false;
    };
    let limit = match field {
        Field::Name => NAME_LIMIT,
        Field::Address | Field::Relay => ADDRESS_LIMIT,
        Field::Room => ROOM_LIMIT,
        Field::Code => RULES_CODE_LIMIT,
        Field::Recovery => 80,
    };
    for event in typed.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match &event.logical_key {
            Key::Character(chars) => {
                for c in chars.chars().filter(|c| !c.is_control()) {
                    let allowed = match field {
                        Field::Name => crate::names::allowed(c),
                        Field::Address | Field::Relay => {
                            c.is_ascii_alphanumeric() || ".:-".contains(c)
                        }
                        Field::Room => c.is_ascii_alphanumeric() || c == '-',
                        Field::Code | Field::Recovery => c.is_ascii_alphanumeric() || c == '-',
                    };
                    if allowed && text.chars().count() < limit {
                        text.push(c);
                    }
                }
            }
            Key::Space if field == Field::Name && text.chars().count() < limit => {
                text.push(' ');
            }
            Key::Backspace => {
                text.pop();
            }
            _ => {}
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        l.refresh();
        return true;
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        let text = text.trim().to_owned();
        match field {
            // [KHC-044] The name rules decide; a refused name says why.
            Field::Name => {
                if let Ok(p) = &mut l.profile {
                    l.profile_note = p.rename(&text).err();
                }
                l.save_profile();
                l.refresh();
            }
            Field::Address if !text.is_empty() => {
                l.settings.last_host = Some(text.clone());
                if let Err(e) = install::save_settings(&l.dir, &l.settings) {
                    eprintln!("Launcher settings not saved: {e}");
                }
                let name = l.player_name();
                duel(l, vec!["--join".into(), text, "--name".into(), name]);
            }
            Field::Address => l.refresh(),
            Field::Recovery => {
                if !text.is_empty()
                    && let Ok(p) = &mut l.profile
                {
                    l.profile_note = Some(match p.restore(&text) {
                        Ok(()) => format!("Restored: now {}", p.tagged()),
                        Err(e) => e,
                    });
                    l.save_profile();
                }
                l.refresh();
            }
            // [KHC-016] Kept only when it reads; empty clears it.
            Field::Code => {
                if text.is_empty() {
                    l.settings.duel_code = None;
                } else if crate::room::duel_wire::decode_code(&text).is_ok() {
                    l.settings.duel_code = Some(text.to_ascii_uppercase());
                }
                if let Err(e) = install::save_settings(&l.dir, &l.settings) {
                    eprintln!("Launcher settings not saved: {e}");
                }
                l.refresh();
            }
            Field::Relay => {
                l.settings.relay_server = (!text.is_empty()).then_some(text);
                if let Err(e) = install::save_settings(&l.dir, &l.settings) {
                    eprintln!("Launcher settings not saved: {e}");
                }
                l.refresh();
            }
            Field::Room => {
                let code = net::wire::parse_code(&text);
                if let Some(addr) = net::direct::parse_code(&text) {
                    // [KHC-051] A direct code: no relay needed.
                    let code = net::direct::encode_code(addr);
                    l.settings.last_room = Some(code.clone());
                    if let Err(e) = install::save_settings(&l.dir, &l.settings) {
                        eprintln!("Launcher settings not saved: {e}");
                    }
                    let name = l.player_name();
                    duel(l, vec!["--room".into(), code, "--name".into(), name]);
                } else if let (Some(code), Some(relay)) = (code, l.settings.relay_server.clone()) {
                    let code = String::from_utf8_lossy(&code).into_owned();
                    l.settings.last_room = Some(code.clone());
                    if let Err(e) = install::save_settings(&l.dir, &l.settings) {
                        eprintln!("Launcher settings not saved: {e}");
                    }
                    let name = l.player_name();
                    duel(
                        l,
                        vec![
                            "--relay".into(),
                            relay,
                            "--room".into(),
                            code,
                            "--name".into(),
                            name,
                        ],
                    );
                } else {
                    l.refresh();
                }
            }
        }
        return true;
    }
    l.editing = Some((field, text));
    l.refresh();
    true
}

/// Starts the test room as a duel host or joiner with `session` options.
fn duel(l: &mut Launcher, session: Vec<String>) {
    let Some(b) = l.bound() else {
        return;
    };
    let mut session = session;
    session.extend(["--input".into(), l.duel_input().to_owned()]);
    // [KHC-058] What loads on first use loads before play.
    if l.settings.preload == Some(true) {
        session.push("--preload".into());
    }
    // [KHC-016] A host takes the rules code set on the Multiplayer screen.
    if session.iter().any(|a| a == "--host")
        && let Some(code) = &l.settings.duel_code
    {
        session.extend(["--duel-code".into(), code.clone()]);
    }
    let started = install::duel_args(&l.dir, &b, &session).and_then(|args| {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        spawn("the duel".into(), exe, args)
    });
    match started {
        Ok(running) => l.go(Screen::Loading(running, b)),
        Err(why) => l.go(Screen::Problem {
            title: "The duel could not start".into(),
            lines: vec![why],
            retry: Some(b),
        }),
    }
}

fn back(l: &mut Launcher, exit: &mut MessageWriter<AppExit>) {
    let to = match &l.screen {
        Screen::Browse(Some(p)) => Some(Action::Browse(p.parent().map(Path::to_path_buf))),
        Screen::Browse(None) => Some(Action::Pick),
        Screen::BrowseKh3(Some(p), _) => Some(Action::BrowseKh3(p.parent().map(Path::to_path_buf))),
        Screen::BrowseKh3(None, _) => Some(Action::Return),
        Screen::Pick if l.check_to_quick => Some(Action::QuickStart),
        Screen::Pick => l.settings.game_folder.clone().map(Action::UseFolder),
        Screen::Problem { retry: Some(b), .. } => Some(Action::UseFolder(b.folder.clone())),
        Screen::Lan(_) | Screen::Practice(_) | Screen::Options(_) | Screen::Replays(_) => {
            Some(Action::Main)
        }
        Screen::QuickStart(Some(_)) => Some(Action::Main),
        _ => None,
    };
    if let Some(a) = to {
        l.menu.play(Cue::Back);
        act(l, a, exit);
    }
}

/// A finished check: keep the folder if it is the supported build.
fn checked(l: &mut Launcher, result: Result<Bound, String>, folder: &Path) {
    let b = match result {
        Ok(b) => b,
        Err(why) => {
            return l.go(Screen::Problem {
                title: "This folder can't be used".into(),
                lines: vec![plain(folder), why],
                retry: None,
            });
        }
    };
    if let Some(want) = &l.supported
        && *want != b.identity
    {
        return l.go(Screen::Problem {
            title: "Unsupported game version".into(),
            lines: vec![
                plain(&b.folder),
                format!(
                    "Kingdom Hearts: Connected only runs on the supported Steam version ({}…). This folder has {}….",
                    &want[..12.min(want.len())],
                    &b.identity[..12.min(b.identity.len())]
                ),
            ],
            retry: None,
        });
    }
    l.settings.game_folder = Some(b.folder.clone());
    l.settings.manifest = Some(b.manifest.clone());
    save_settings(l);
    if std::mem::take(&mut l.check_to_quick) {
        l.go(Screen::QuickStart(Some(b)));
    } else {
        l.go(Screen::Main(b));
    }
}

/// The KH3 install set, or the one found in Steam's libraries or Epic's
/// folder (remembered then); why there is none otherwise.
fn find_kh3(settings: &mut Settings, dir: &Path) -> Result<kh3_data::Install, String> {
    if let Some(folder) = settings.kh3_folder.clone() {
        return kh3_data::admit(&folder).map_err(|e| e.to_string());
    }
    let found = kh3_data::detect(&install::steam_libraries());
    let Some(install) = found.iter().find_map(|f| kh3_data::admit(f).ok()) else {
        return Err("Not set: needed for the KH3 mode".into());
    };
    settings.kh3_folder = Some(install.root.clone());
    if let Err(e) = install::save_settings(dir, settings) {
        eprintln!("Launcher settings not saved: {e}");
    }
    Ok(install)
}

/// Developer check: with `KHC_LAUNCHER_SHOT` naming a PNG, F12 saves a
/// picture of the launcher there (a window capture of the GPU surface comes
/// out black).
/// `KHC_LAUNCHER_SHOT_AT` (seconds) takes one by itself, with no key.
fn screenshot(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut taken: Local<bool>,
) {
    let timed = !*taken
        && std::env::var("KHC_LAUNCHER_SHOT_AT")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .is_some_and(|at| time.elapsed_secs() >= at);
    if (keys.just_pressed(KeyCode::F12) || timed)
        && let Some(path) = std::env::var_os("KHC_LAUNCHER_SHOT")
    {
        *taken |= timed;
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(PathBuf::from(
                path,
            )));
    }
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut l: ResMut<Launcher>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<&Gamepad>,
    mut wheel: MessageReader<MouseWheel>,
    mut drops: MessageReader<FileDragAndDrop>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut winit: ResMut<WinitSettings>,
    mut typed: MessageReader<KeyboardInput>,
    mut exit: MessageWriter<AppExit>,
) {
    let l = &mut *l;
    let dt = time.delta_secs().min(0.1);
    l.now += dt;
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    if matches!(l.screen, Screen::Boot) {
        l.supported = install::supported_identity(&l.dir);
        l.detected = install::detect_installs();
        l.kh3 = find_kh3(&mut l.settings, &l.dir);
        // The menu's look comes from the game folder known or found.
        if let Some(folder) = l
            .settings
            .game_folder
            .clone()
            .or_else(|| l.detected.first().cloned())
        {
            l.menu.load(&folder);
        }
        match l.settings.game_folder.clone() {
            Some(folder) if l.settings.guide_seen => act(l, Action::UseFolder(folder), &mut exit),
            _ => l.go(Screen::QuickStart(None)),
        }
    }
    let mut lines = 0.;
    for w in wheel.read() {
        lines += match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 40.,
        };
    }
    for d in drops.read() {
        match d {
            FileDragAndDrop::HoveredFile { .. } => l.hover_drop = true,
            FileDragAndDrop::HoveredFileCanceled { .. } => l.hover_drop = false,
            FileDragAndDrop::DroppedFile { path_buf, .. } => {
                l.hover_drop = false;
                let folder = if path_buf.is_dir() {
                    path_buf.clone()
                } else {
                    path_buf.parent().map(Path::to_path_buf).unwrap_or_default()
                };
                if matches!(
                    l.screen,
                    Screen::Pick
                        | Screen::Browse(_)
                        | Screen::Problem { .. }
                        | Screen::Main(_)
                        | Screen::QuickStart(_)
                ) {
                    act(l, Action::UseFolder(folder), &mut exit);
                }
            }
        }
    }

    // Background work.
    let (mut finished, mut ended) = (None, None);
    let mut link = None;
    match &mut l.screen {
        Screen::Check(job) => {
            if let Some(result) = job.done.lock().ok().and_then(|mut d| d.take()) {
                finished = Some((result, job.folder.clone()));
            }
        }
        Screen::Loading(run, b) => {
            if run.window_open.load(Ordering::Relaxed) && run.since_window.is_none() {
                run.since_window = Some(l.now);
            }
            // Hide once the test's window has had a moment to draw.
            if !run.hidden && run.since_window.is_some_and(|t| l.now - t > 0.6) {
                run.hidden = true;
                window.visible = false;
                winit.unfocused_mode = UpdateMode::reactive_low_power(Duration::from_millis(100));
            }
            if let Ok(Some(status)) = run.child.try_wait() {
                let mut lines = run
                    .output
                    .lock()
                    .map(|o| failure_lines(&o))
                    .unwrap_or_default();
                lines.retain(|line| !line.starts_with("KHC "));
                link = run.output.lock().ok().and_then(|o| last_link(&o));
                ended = Some((status, b.clone(), run.name.clone(), lines));
            }
        }
        _ => {}
    }
    if let Some((status, b, name, lines)) = ended {
        l.last_link = link.or(l.last_link.take());
        if status.success() {
            // A duel left from its result screen (or closed) comes back to
            // the menu.
            l.go(Screen::Main(b));
        } else {
            l.go(Screen::Problem {
                title: format!("{name} stopped with an error"),
                lines: if lines.is_empty() {
                    vec![format!("Exit status: {status}")]
                } else {
                    lines
                },
                retry: Some(b),
            });
        }
        if !matches!(l.screen, Screen::Loading(..)) {
            window.visible = true;
            window.focused = true;
            winit.unfocused_mode =
                UpdateMode::reactive_low_power(Duration::from_secs_f64(1. / 60.));
            l.fade = 1.;
        }
    }
    if let Some((result, folder)) = finished {
        checked(l, result, &folder);
    }

    // The LAN search: new or vanished hosts rebuild the list.
    if let Some(browser) = l.browser.as_mut() {
        browser.update();
        // (When each host was last heard is not part of the rows; its ping
        // is, to 5 ms.)
        let rows = |f: &[net::Found]| -> Vec<_> {
            f.iter()
                .map(|h| {
                    let ping = h.ping_ms.map(|ms| ms.div_ceil(5));
                    (h.name.clone(), h.addr, h.compatible, h.open, ping)
                })
                .collect()
        };
        if rows(browser.found()) != rows(&l.found) {
            l.found = browser.found().to_vec();
            l.refresh();
        }
    }
    // A field being typed into takes the keyboard.
    if type_text(l, &keys, &mut typed) {
        return;
    }
    // Input on lists.
    if l.items.is_empty() {
        return;
    }
    let p = read_input(l, &keys, &pads, lines);
    let was = l.cursor;
    if p.up {
        move_cursor(l, -1);
    }
    if p.down {
        move_cursor(l, 1);
    }
    if p.page != 0 {
        let rows = visible_rows(l) as i32;
        let n = l.items.len() as i32;
        l.cursor = (l.cursor as i32 + p.page * rows).clamp(0, n - 1) as usize;
        keep_visible(l);
    }
    // Mouse: hover picks (only while it moves, so keys win otherwise),
    // click decides.
    let mut clicked = false;
    let pos = window.cursor_position();
    let moved = l.mouse.is_some() && pos.is_some() && pos != l.mouse;
    l.mouse = pos;
    if let Some(pos) = pos {
        let (x, y) = (
            pos.x * paint::W / window.width(),
            pos.y * paint::H / window.height(),
        );
        let rows = visible_rows(l);
        for i in l.scroll..(l.scroll + rows).min(l.items.len()) {
            let [rx, ry, rw, rh] = row_rect(l, i);
            let inside = x >= rx && x <= rx + rw + 20. && y >= ry && y <= ry + rh;
            if inside && l.items[i].action.is_some() {
                if moved {
                    l.cursor = i;
                }
                if mouse.just_pressed(MouseButton::Left) {
                    l.cursor = i;
                    clicked = true;
                }
            }
        }
    }
    if mouse.just_pressed(MouseButton::Right) {
        back(l, &mut exit);
        return;
    }
    if l.cursor != was && !clicked {
        l.menu.play(Cue::Cursor);
    }
    if p.accept || clicked {
        match l.items.get(l.cursor).and_then(|i| i.action.clone()) {
            Some(a) => {
                l.menu.play(Cue::Decide);
                act(l, a, &mut exit);
            }
            None => l.menu.play(Cue::Refused),
        }
    } else if p.back {
        back(l, &mut exit);
    } else if p.change && matches!(l.screen, Screen::Main(_) | Screen::Options(_)) {
        act(l, Action::Pick, &mut exit);
    }
}

/// A picture read from a PNG file: its sheet and pixel size. None when the file
/// is missing or unreadable.
fn load_picture(
    images: &mut Assets<Image>,
    frame: &mut Ui2dFrame,
    path: &Path,
) -> Option<(usize, [u32; 2])> {
    let picture = image::open(path).ok()?.into_rgba8();
    let size = [picture.width(), picture.height()];
    Some((frame.add_sheet(images, picture.into_raw(), size, 1.), size))
}

/// A whole picture in the box `r`.
fn picture(frame: &mut Ui2dFrame, sheet: usize, size: [u32; 2], r: [f32; 4]) {
    let uv = [0, 0, size[0] as i32, size[1] as i32];
    frame
        .draws
        .push((sheet, vec![quad(r, uv, [rgba(0xffffff, 1.); 4], 0)]));
}

/// `assets/khc-logo.png` beside the executable or in a folder above it.
fn logo_file() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .skip(1)
        .take(4)
        .map(|up| up.join("assets/khc-logo.png"))
        .find(|path| path.is_file())
}

/// A picture of `size` fitted inside `area` [x, y, width, height], centred.
fn logo_box(size: [u32; 2], area: [f32; 4]) -> [f32; 4] {
    let scale = (area[2] / size[0] as f32).min(area[3] / size[1] as f32);
    let (w, h) = (size[0] as f32 * scale, size[1] as f32 * scale);
    [
        area[0] + (area[2] - w) / 2.,
        area[1] + (area[3] - h) / 2.,
        w,
        h,
    ]
}

fn draw(
    mut l: ResMut<Launcher>,
    mut frame: ResMut<Ui2dFrame>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
) {
    let l = &mut *l;
    if l.paint.is_none() {
        match Paint::new(&mut images, &mut frame) {
            Ok(p) => l.paint = Some(p),
            Err(e) => {
                eprintln!("Launcher drawing unavailable: {e}");
                return;
            }
        }
    }
    let dt = time.delta_secs().min(0.1);
    l.fade = (l.fade - dt * 4.).max(0.);
    let mut paint = l.paint.take().expect("paint");
    let (p, f) = (&mut paint, &mut *frame);
    l.menu.ready(&mut images, f);
    let logo = *l
        .logo
        .get_or_insert_with(|| logo_file().and_then(|path| load_picture(&mut images, f, &path)));
    l.menu.background(p, f);
    if let Some((sheet, size)) = logo {
        picture(f, sheet, size, logo_box(size, [1100., 8., 148., 96.]));
    }
    let heading = match &l.screen {
        Screen::Boot => String::new(),
        Screen::QuickStart(_) => "QUICK START".into(),
        Screen::Pick | Screen::Browse(_) => "GAME FOLDER".into(),
        Screen::BrowseKh3(..) => "KINGDOM HEARTS III".into(),
        Screen::Check(_) => "CHECKING THE GAME".into(),
        Screen::Main(_) => "MULTIPLAYER".into(),
        Screen::Lan(_) => "LAN HOST".into(),
        Screen::Practice(_) => "PRACTICE".into(),
        Screen::Options(_) => "OPTIONS".into(),
        Screen::Replays(_) => "REPLAYS".into(),
        Screen::Loading(..) => "LOADING".into(),
        Screen::Problem { title, .. } => title.to_uppercase(),
    };
    if !heading.is_empty() {
        let heading = p.fit(Face::Heading, 24., &heading, 900.);
        let [x, y, w, h] = BANNER;
        let w = w.max(p.measure(Face::Heading, 24., &heading) + 150.);
        l.menu.banner(p, f, [x, y, w, h]);
        p.text(
            f,
            Face::Heading,
            24.,
            [x + 96., y + 4.],
            0.,
            &heading,
            [rgba(BANNER_INK, 1.); 2],
            0.,
        );
    }
    // The player, top right, once there is a menu.
    if l.bound().is_some() {
        p.text(
            f,
            Face::Body,
            18.,
            [1060., 44.],
            1.,
            &l.name_tag(),
            [rgba(INK, 1.); 2],
            0.,
        );
    }
    match &l.screen {
        Screen::Check(job) => check_screen(job, p, f),
        Screen::Loading(..) => {
            let dots = ".".repeat(1 + (l.now * 3.) as usize % 3);
            p.text(
                f,
                Face::Body,
                20.,
                [ROW_X, ROW_TOP],
                0.,
                &dots,
                [rgba(INK, 1.); 2],
                0.,
            );
        }
        Screen::Boot => {}
        _ => {
            let subtitle = match &l.screen {
                Screen::Browse(Some(path)) | Screen::BrowseKh3(Some(path), _) => Some(plain(path)),
                _ => None,
            };
            if let Some(s) = subtitle {
                let s = p.fit(Face::Body, 16., &s, paint::W - 2. * ROW_X);
                p.text(
                    f,
                    Face::Body,
                    16.,
                    [ROW_X, 86.],
                    0.,
                    &s,
                    [rgba(MUTED, 1.); 2],
                    0.,
                );
            }
            if let Screen::Problem { lines, .. } = &l.screen {
                let mut y = ROW_TOP - 10.;
                for para in lines {
                    for line in wrap(p, Face::Body, 18., para, paint::W - 2. * MARGIN) {
                        p.text(
                            f,
                            Face::Body,
                            18.,
                            [ROW_X, y],
                            0.,
                            &line,
                            [rgba(INK, 1.); 2],
                            0.,
                        );
                        y += 26.;
                    }
                }
            }
            if let Screen::QuickStart(_) = &l.screen {
                controls(p, f);
            }
            list(l, p, f);
            footer(l, p, f);
        }
    }
    if l.hover_drop {
        p.rect(
            f,
            [0., 0., paint::W, paint::H],
            [rgba(0x000000, 0.7); 4],
            false,
        );
        p.text(
            f,
            Face::Heading,
            30.,
            [paint::W / 2., 340.],
            0.5,
            "Drop to use this game folder",
            [rgba(TEXT, 1.); 2],
            0.,
        );
    }
    if l.fade > 0. {
        p.rect(
            f,
            [0., 0., paint::W, paint::H],
            [rgba(0x000000, l.fade); 4],
            false,
        );
    }
    p.flush(&mut images, f);
    l.paint = Some(paint);
}

fn list_top(l: &Launcher) -> f32 {
    match &l.screen {
        Screen::Problem { lines, .. } => ROW_TOP + 26. * lines.len() as f32 + 24.,
        _ => ROW_TOP,
    }
}

/// How wide a row's setting may be: narrow on Quick start, where the
/// controls stand to the right.
fn value_width(l: &Launcher) -> f32 {
    if matches!(l.screen, Screen::QuickStart(_)) {
        200.
    } else {
        paint::W - VALUE_X - 60.
    }
}

/// The bars, their labels and settings, the pointer at the cursor's.
fn list(l: &Launcher, p: &mut Paint, f: &mut Ui2dFrame) {
    let rows = visible_rows(l);
    let value_w = value_width(l);
    for i in l.scroll..(l.scroll + rows).min(l.items.len()) {
        let item = &l.items[i];
        let on = i == l.cursor;
        let enabled = item.action.is_some();
        let [x, y, w, h] = row_rect(l, i);
        l.menu
            .bar(p, f, [x, y, w, h], on, if enabled { 1. } else { 0.55 });
        let ink = match (enabled, on) {
            (false, _) => INK_DIM,
            (true, true) => 0xffffff,
            (true, false) => INK,
        };
        let tag_w = item.tag.map_or(0., |t| p.measure(Face::Body, 15., t) + 20.);
        let label = p.fit(Face::Body, 20., &item.label, w - 56. - tag_w);
        p.text(
            f,
            Face::Body,
            20.,
            [x + 30., y + (h - 20. * 1.33) / 2.],
            0.,
            &label,
            [rgba(ink, 1.); 2],
            0.,
        );
        if let Some(tag) = item.tag {
            p.text(
                f,
                Face::Body,
                15.,
                [x + w - 26., y + (h - 20.) / 2.],
                1.,
                tag,
                [rgba(GOLD, 1.); 2],
                0.,
            );
        }
        if let Some(value) = &item.value {
            let value = p.fit(Face::Body, 20., value, value_w);
            p.text(
                f,
                Face::Body,
                20.,
                [VALUE_X, y + (h - 20. * 1.33) / 2.],
                0.,
                &value,
                [rgba(if on { GOLD } else { INK }, 1.); 2],
                0.,
            );
        }
        if on {
            l.menu.pointer(p, f, [x + 8., y + h / 2.]);
        }
    }
    let c = rgba(INK, 0.8);
    let mid = ROW_X + ROW_W / 2.;
    if l.scroll > 0 {
        let y = list_top(l) - 12.;
        triangle(
            f,
            p,
            [[mid - 9., y + 4.], [mid + 9., y + 4.], [mid, y - 5.]],
            c,
        );
    }
    if l.scroll + rows < l.items.len() {
        let y = ROW_BOTTOM + 8.;
        triangle(
            f,
            p,
            [[mid - 9., y - 4.], [mid + 9., y - 4.], [mid, y + 5.]],
            c,
        );
    }
}

/// How the room's output last said its link went, as the launcher shows it.
fn last_link(output: &std::collections::VecDeque<String>) -> Option<String> {
    output
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("KHC link: "))
        .map(|text| text.trim().to_owned())
}

/// The cursor's description over a gold rule, and the keys.
fn footer(l: &Launcher, p: &mut Paint, f: &mut Ui2dFrame) {
    let rule = rgba(0xb09858, 0.8);
    let clear = rgba(0xb09858, 0.);
    p.rect(
        f,
        [ROW_X, DESCRIPTION_Y - 10., paint::W - 2. * ROW_X, 2.],
        [rule, clear, rule, clear],
        false,
    );
    let description = l
        .items
        .get(l.cursor)
        .and_then(|i| i.detail.clone())
        .unwrap_or_default();
    let mut y = DESCRIPTION_Y;
    for line in wrap(p, Face::Body, 18., &description, paint::W - 2. * ROW_X)
        .into_iter()
        .take(2)
    {
        p.text(
            f,
            Face::Body,
            18.,
            [ROW_X, y],
            0.,
            &line,
            [rgba(INK, 1.); 2],
            0.,
        );
        y += 24.;
    }
    let hints = if l.editing.is_some() {
        "Type, then Enter to keep      Esc  cancel"
    } else {
        "Enter / A  decide      Esc / B  back"
    };
    p.text(
        f,
        Face::Body,
        15.,
        [paint::W - ROW_X, 684.],
        1.,
        hints,
        [rgba(FAINT, 1.); 2],
        0.,
    );
}

/// Action, keyboard, controller (the room's bindings, `input::read_devices`).
const GUIDE_CONTROLS: [[&str; 3]; 9] = [
    ["Move", "WASD", "Left stick"],
    ["Walk", "Hold Left Alt", "Tilt slightly"],
    ["Camera", "Mouse", "Right stick"],
    ["Attack", "Enter, left click", "A"],
    ["Jump", "Space", "B"],
    ["Guard", "Right Shift, right click", "X"],
    ["Talk", "F", "Y"],
    ["Shortcuts", "Hold Left Shift", "LB"],
    ["Leave the room", "Esc", ""],
];

/// The controls, right of Quick start's rows.
fn controls(p: &mut Paint, f: &mut Ui2dFrame) {
    let columns = [780., 930., 1100.];
    for (x, head) in columns.iter().zip(["Controls", "Keyboard", "Controller"]) {
        p.text(
            f,
            Face::Body,
            16.,
            [*x, ROW_TOP],
            0.,
            head,
            [rgba(GOLD, 1.); 2],
            0.,
        );
    }
    for (row, cells) in GUIDE_CONTROLS.iter().enumerate() {
        let y = ROW_TOP + 32. + row as f32 * 30.;
        for ((x, cell), ink) in columns.iter().zip(cells).zip([INK, MUTED, MUTED]) {
            let cell = p.fit(Face::Body, 16., cell, 160.);
            p.text(
                f,
                Face::Body,
                16.,
                [*x, y],
                0.,
                &cell,
                [rgba(ink, 1.); 2],
                0.,
            );
        }
    }
}

/// The KH3 folder the launcher remembers, and the pak key's place in its
/// executable, for the duel's KH3 mode.
pub fn kh3_folder() -> (Option<std::path::PathBuf>, Option<u64>) {
    let settings = install::load_settings(&install::data_dir());
    (settings.kh3_folder, settings.kh3_key_offset)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<[u8; 32]> {
    let mut out = [0; 32];
    if text.len() != 64 {
        return None;
    }
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// What identifies a BBS package for what is remembered of it: its size and time.
fn bbs_key(root: &Path) -> Option<String> {
    let meta = std::fs::metadata(root.join(bbs_data::bbs_source::PACKAGE)).ok()?;
    let time = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!("{}:{}", meta.len(), time.as_nanos()))
}

fn keep_settings(change: impl FnOnce(&mut install::Settings)) {
    let dir = install::data_dir();
    let mut settings = install::load_settings(&dir);
    let before = settings.clone();
    change(&mut settings);
    if settings != before
        && let Err(e) = install::save_settings(&dir, &settings)
    {
        eprintln!("Launcher settings not saved: {e}");
    }
}

pub fn remember_kh3_digest(install: &kh3_data::Install, digest: [u8; 32]) {
    let kept = (install.identity_hex(), hex(&digest));
    keep_settings(|s| s.kh3_digest = Some(kept));
}

pub fn remember_bbs_digest(root: &Path, digest: [u8; 32]) {
    if let Some(key) = bbs_key(root) {
        keep_settings(|s| s.bbs_digest = Some((key, hex(&digest))));
    }
}

/// These installs as last read, if nothing about them has changed since.
pub fn remembered_capabilities(
    kh3: Option<&kh3_data::Install>,
    root: Option<&Path>,
) -> net::wire::Capabilities {
    use net::wire::Cap;
    let settings = install::load_settings(&install::data_dir());
    let of = |kept: &Option<(String, String)>, key: Option<String>| match (kept, key) {
        (_, None) => Cap::Absent,
        (Some((k, digest)), Some(key)) if *k == key => unhex(digest).map_or(Cap::Unread, Cap::Hash),
        _ => Cap::Unread,
    };
    net::wire::Capabilities {
        kh3: of(
            &settings.kh3_digest,
            kh3.map(kh3_data::Install::identity_hex),
        ),
        bbs: of(&settings.bbs_digest, root.and_then(bbs_key)),
    }
}

/// Remembers where the pak key was found (it saves a scan of the executable).
pub fn remember_kh3_key_offset(offset: u64) {
    let dir = install::data_dir();
    let mut settings = install::load_settings(&dir);
    if settings.kh3_key_offset != Some(offset) {
        settings.kh3_key_offset = Some(offset);
        if let Err(e) = install::save_settings(&dir, &settings) {
            eprintln!("Launcher settings not saved: {e}");
        }
    }
}

pub fn main() -> std::process::ExitCode {
    let dir = install::data_dir();
    let settings = install::load_settings(&dir);
    // [KHC-044] Made on first launch, from the old name when it passes the rules.
    let profile = crate::profile::load_or_create(&dir, settings.player_name.as_deref());
    if let Err(e) = &profile {
        eprintln!("Profile: {e}");
    }
    App::new()
        .insert_resource(Launcher {
            dir,
            settings,
            supported: None,
            detected: Vec::new(),
            screen: Screen::Boot,
            items: Vec::new(),
            cursor: 0,
            scroll: 0,
            now: 0.,
            fade: 1.,
            hover_drop: false,
            repeat: None,
            mouse: None,
            paint: None,
            menu: CollectionMenu::default(),
            check_to_quick: false,
            editing: None,
            browser: None,
            found: Vec::new(),
            build: None,
            kh3: Err(String::new()),
            last_link: None,
            logo: None,
            profile,
            profile_note: None,
            confirm: None,
        })
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(WinitSettings::game())
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("Kingdom Hearts: Connected {}", env!("CARGO_PKG_VERSION")),
                        resolution: WindowResolution::new(1280, 720),
                        ..default()
                    }),
                    ..default()
                })
                .set(visual::low_cost::task_pools()),
        )
        .insert_resource(visual::frame_limit::FrameLimit::new(
            visual::frame_limit::ORIGINAL_FPS,
        ))
        .add_plugins(visual::frame_limit::FrameLimitPlugin)
        .add_plugins(visual::low_cost::LowCostPlugin)
        .add_plugins(Ui2dPlugin {
            order: 0,
            overlay: false,
        })
        .add_systems(Update, ((update, draw).chain(), screenshot))
        .run();
    std::process::ExitCode::SUCCESS
}
