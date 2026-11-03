//! The title's New Game dialog (modal FUN_1402b7420; construction
//! FUN_1402b8520; pages FUN_1402b8910 / FUN_1402b8d60; page frame
//! FUN_1402b7940; driver FUN_1402b7e40; close FUN_1402b8470; intro skip
//! FUN_1402b8830; apply FUN_1402b88c0). All sprites use title.2ld SEQD 0
//! on image 0, in layout space with x origin 0.
use crate::sprite_player::{Id, LightCur, Sprites, Text};
use data::sprite_sequence::Sequence;

/// sys.bar ids (`& 0x7fff`) the dialog shows.
pub const HEADING: u32 = 0xc32e;
pub const PAGE_TITLES: [u32; 2] = [0xc330, 0xc337];
pub const SUMMARIES: [u32; 2] = [0xc380, 0xc381];
pub const MODES: [u32; 4] = [0xc331, 0xc332, 0xc333, 0xce33];
pub const MODE_HELP: [u32; 4] = [0xc334, 0xc335, 0xc336, 0xce34];
pub const VIBRATION: [u32; 2] = [0xc338, 0xc339];
pub const VIBRATION_HELP: [u32; 2] = [0xc33a, 0xc33b];
pub const NO: u32 = 0xc384;
pub const YES: u32 = 0xc383;
pub const CONFIRM: u32 = 0xc32f;

/// Every id above, in the order `Texts::from_messages` expects.
pub fn message_ids() -> Vec<u32> {
    let mut ids = vec![HEADING];
    ids.extend(PAGE_TITLES);
    ids.extend(SUMMARIES);
    ids.extend(MODES);
    ids.extend(MODE_HELP);
    ids.extend(VIBRATION);
    ids.extend(VIBRATION_HELP);
    ids.extend([NO, YES, CONFIRM]);
    ids
}

#[derive(Clone, Debug, Default)]
pub struct Texts {
    pub heading: Vec<u8>,
    pub titles: [Vec<u8>; 2],
    pub summaries: [Vec<u8>; 2],
    pub items: [Vec<Vec<u8>>; 2],
    pub help: [Vec<Vec<u8>>; 2],
    pub no: Vec<u8>,
    pub yes: Vec<u8>,
    pub confirm: Vec<u8>,
}

impl Texts {
    /// From messages loaded in `message_ids()` order.
    pub fn from_messages(m: Vec<Vec<u8>>) -> Self {
        let mut it = m.into_iter();
        let mut take = |n: usize| {
            (0..n)
                .map(|_| it.next().unwrap_or_default())
                .collect::<Vec<_>>()
        };
        let heading = take(1).remove(0);
        let titles = take(2);
        let summaries = take(2);
        let modes = take(4);
        let mode_help = take(4);
        let vib = take(2);
        let vib_help = take(2);
        let rest = take(3);
        Self {
            heading,
            titles: [titles[0].clone(), titles[1].clone()],
            summaries: [summaries[0].clone(), summaries[1].clone()],
            items: [modes, vib],
            help: [mode_help, vib_help],
            no: rest[0].clone(),
            yes: rest[1].clone(),
            confirm: rest[2].clone(),
        }
    }
}

/// Priorities: heading 0x753b, page sprites 0x753c, pointer and orb 0x754a.
const PRIO_HEADING: u32 = 0x753b;
const PRIO_PAGE: u32 = 0x753c;
const PRIO_POINTER: u32 = 0x754a;

/// SEQD 0 groups (0x1405b86d0 and the build routines).
const HEADING_GROUPS: [usize; 3] = [22, 23, 24];
const DESCRIPTION: [usize; 3] = [28, 29, 30];
const PAGE_TITLE: usize = 25;
const LABEL: usize = 26;
const VALUE: usize = 13;
/// Rows: appear, close, idle, selected.
const PAGE_ROWS: [usize; 4] = [15, 17, 16, 14];
const CONFIRM_ROWS: [usize; 4] = [19, 21, 20, 18];
const POINTER: usize = 8;
const LIGHT: [usize; 2] = [10, 11];
/// Description text: offset (0, 76) from its group, size 17.
const DESCRIPTION_SIZE: f32 = 17.;
const TEXT_SIZE: f32 = 15.;

/// se000 sounds (FUN_1402f6270 with profile 1).
pub const SOUND_MOVE: u32 = 1;
pub const SOUND_DECIDE: u32 = 2;
pub const SOUND_CANCEL: u32 = 4;
pub const SOUND_BUZZER: u32 = 5;

/// Logical bits (pad mode 7).
const UP: u64 = 0x10;
const DOWN: u64 = 0x20;
const LEFT: u64 = 0x40;
const RIGHT: u64 = 0x80;
const DECIDE: u64 = 0x4;
const CANCEL: u64 = 0x8;
const START: u64 = 0x100;

/// A list (0x48 B, FUN_1402f7720) and its input (FUN_1402f9440).
#[derive(Clone, Debug, Default)]
struct List {
    rows: Vec<Id>,
    cursor: usize,
    wrap: bool,
    previous_mask: u64,
    next_mask: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    None,
    Row(usize),
    Cancel,
    Start,
}

impl List {
    fn input(&mut self, trig: u64, repeat: u64) -> (Pick, bool) {
        let n = self.rows.len();
        let before = self.cursor;
        if n == 0 {
            return (Pick::None, false);
        }
        if repeat & self.previous_mask != 0 {
            if self.cursor > 0 {
                self.cursor -= 1;
            } else if self.wrap && trig & self.previous_mask != 0 {
                self.cursor = n - 1;
            }
        } else if repeat & self.next_mask != 0 {
            if self.cursor + 1 < n {
                self.cursor += 1;
            } else if self.wrap && trig & self.next_mask != 0 {
                self.cursor = 0;
            }
        } else if trig & DECIDE != 0 {
            return (Pick::Row(self.cursor), false);
        } else if trig & CANCEL != 0 {
            return (Pick::Cancel, false);
        } else if trig & START != 0 {
            return (Pick::Start, false);
        }
        (Pick::None, self.cursor != before)
    }
}

pub struct NewGame {
    pub sprites: Sprites,
    /// Image index every sprite draws from (title.2ld image 0).
    sheet: usize,
    texts: Texts,
    heading: Option<Id>,
    description: Option<Id>,
    page_sprites: Vec<Id>,
    lists: [List; 2],
    pointer: Option<Id>,
    light: LightCur,
    pub page: usize,
    substate: u8,
    next_page: usize,
    /// Cursor per page: mode, vibration index (0 ON), confirm row.
    pub values: [usize; 2],
    done: bool,
    pub confirmed: bool,
    frames: u32,
    closing: bool,
    pub finished: bool,
}

impl NewGame {
    /// FUN_1402b8520. Defaults: the current difficulty and vibration (after
    /// the title's save clear: Standard, ON).
    pub fn new(texts: Texts, sheet: usize, mode: usize, vibration_on: bool) -> Self {
        Self {
            sprites: Sprites::default(),
            sheet,
            texts,
            heading: None,
            description: None,
            page_sprites: Vec::new(),
            lists: [
                List {
                    wrap: true,
                    previous_mask: UP,
                    next_mask: DOWN,
                    ..Default::default()
                },
                List {
                    wrap: false,
                    previous_mask: RIGHT,
                    next_mask: LEFT,
                    ..Default::default()
                },
            ],
            pointer: None,
            light: LightCur::default(),
            page: 0,
            substate: 0,
            next_page: 0,
            values: [mode.min(3), if vibration_on { 0 } else { 1 }],
            done: false,
            confirmed: false,
            frames: 0,
            closing: false,
            finished: false,
        }
    }

    fn text(&self, bytes: &[u8], size: f32, align: u8, keep_rgb: bool) -> Text {
        let mut t = Text::new(bytes.to_vec(), size);
        t.align = align;
        t.group_position = false;
        t.keep_rgb = keep_rgb;
        t
    }

    fn row_text(&self, seq: &Sequence, bytes: &[u8], group: usize) -> Text {
        let mut t = self.text(bytes, TEXT_SIZE, 2, false);
        t.offset = [seq.groups[group].text_position_x as f32, 0.];
        t
    }

    /// FUN_1402b8910: the heading (first page only), description, page
    /// title and rows of page 0 or 1.
    fn build_page(&mut self, seq: &Sequence) {
        let page = self.page;
        if self.heading.is_none() {
            let t = self.text(&self.texts.heading, TEXT_SIZE, 2, false);
            let h = self.sprites.create(
                seq,
                PRIO_HEADING,
                self.sheet,
                HEADING_GROUPS[0],
                Some(HEADING_GROUPS[1]),
                Some(t),
            );
            self.heading = Some(h);
        }
        let help = self.texts.help[page][self.values[page]].clone();
        let d = self.description_sprite(seq, &help);
        let title_text = self.text(&self.texts.titles[page], TEXT_SIZE, 2, false);
        let title = self.sprites.create(
            seq,
            PRIO_PAGE,
            self.sheet,
            PAGE_TITLE,
            None,
            Some(title_text),
        );
        self.child(title, d);
        let n = self.texts.items[page].len();
        let pitch = seq.groups[PAGE_ROWS[0]].ui_padding as f32;
        let cursor = self.values[page];
        let mut rows = Vec::new();
        for i in 0..n {
            let queued = if i == cursor {
                PAGE_ROWS[3]
            } else {
                PAGE_ROWS[2]
            };
            let text = self.row_text(seq, &self.texts.items[page][i], PAGE_ROWS[0]);
            let r = self.sprites.create(
                seq,
                PRIO_PAGE,
                self.sheet,
                PAGE_ROWS[0],
                Some(queued),
                Some(text),
            );
            self.child(r, d);
            self.sprites.get_mut(r).offset = [0., -(pitch * n as f32) / 2. + pitch * i as f32];
            rows.push(r);
        }
        self.lists[0].rows = rows;
        self.lists[0].cursor = cursor;
        self.lists[1].rows.clear();
        self.page_sprites.push(title);
        self.substate = 0;
    }

    /// FUN_1402b8d60: the confirm page.
    fn build_confirm(&mut self, seq: &Sequence) {
        let confirm = self.texts.confirm.clone();
        let d = self.description_sprite(seq, &confirm);
        let pitch = seq.groups[LABEL].ui_padding as f32;
        for k in 0..2 {
            let t = self.text(&self.texts.summaries[k], TEXT_SIZE, 0, false);
            let label = self
                .sprites
                .create(seq, PRIO_PAGE, self.sheet, LABEL, None, Some(t));
            self.child(label, d);
            self.sprites.get_mut(label).offset = [0., pitch * (k as f32 - 1.)];
            let value = self.texts.items[k][self.values[k]].clone();
            let t = self.text(&value, TEXT_SIZE, 0, true);
            let v = self
                .sprites
                .create(seq, PRIO_PAGE, self.sheet, VALUE, None, Some(t));
            self.child(v, label);
            let g = &seq.groups[LABEL];
            self.sprites.get_mut(v).offset = [g.light_position_x as f32, g.text_position_y as f32];
            self.page_sprites.extend([label, v]);
        }
        let pitch = seq.groups[CONFIRM_ROWS[0]].ui_padding as f32;
        let mut rows = Vec::new();
        for (i, bytes) in [self.texts.no.clone(), self.texts.yes.clone()]
            .iter()
            .enumerate()
        {
            let queued = if i == 1 {
                CONFIRM_ROWS[3]
            } else {
                CONFIRM_ROWS[2]
            };
            let text = self.row_text(seq, bytes, CONFIRM_ROWS[0]);
            let r = self.sprites.create(
                seq,
                PRIO_PAGE,
                self.sheet,
                CONFIRM_ROWS[0],
                Some(queued),
                Some(text),
            );
            self.child(r, d);
            self.sprites.get_mut(r).offset = [pitch * i as f32, 0.];
            rows.push(r);
        }
        self.lists[1].rows = rows;
        self.lists[1].cursor = 1;
        self.lists[0].rows.clear();
        self.substate = 0;
    }

    fn description_sprite(&mut self, seq: &Sequence, text: &[u8]) -> Id {
        let mut t = self.text(text, DESCRIPTION_SIZE, 2, false);
        let g = &seq.groups[DESCRIPTION[0]];
        t.offset = [g.light_position_x as f32, g.text_position_y as f32];
        let d = self.sprites.create(
            seq,
            PRIO_PAGE,
            self.sheet,
            DESCRIPTION[0],
            Some(DESCRIPTION[1]),
            Some(t),
        );
        self.description = Some(d);
        self.page_sprites.clear();
        d
    }

    fn child(&mut self, child: Id, parent: Id) {
        self.sprites.attach(child, parent);
        self.sprites.get_mut(child).die_with_parent = true;
    }

    fn list_index(&self) -> usize {
        if self.page < 2 { 0 } else { 1 }
    }

    fn row_groups(&self) -> [usize; 4] {
        if self.page < 2 {
            PAGE_ROWS
        } else {
            CONFIRM_ROWS
        }
    }

    /// FUN_1402b92c0.
    fn intro_done(&self) -> bool {
        let Some(d) = self.description else {
            return false;
        };
        if self.sprites.get(d).player.group != DESCRIPTION[1] {
            return false;
        }
        let g = self.row_groups();
        self.lists.iter().all(|l| {
            l.rows.first().is_none_or(|&r| {
                let s = self.sprites.get(r);
                s.dead || s.player.group == g[2] || s.player.group == g[3]
            })
        })
    }

    /// FUN_1402b8830: the page skips to idle.
    fn skip_intro(&mut self, seq: &Sequence) {
        if let Some(d) = self.description {
            self.sprites.set(seq, d, DESCRIPTION[1], None);
        }
        let l = &self.lists[self.list_index()];
        let g = self.row_groups();
        for (i, &r) in l.rows.clone().iter().enumerate() {
            let group = if i == l.cursor { g[3] } else { g[2] };
            self.sprites.set(seq, r, group, None);
        }
    }

    /// FUN_1402b8470.
    fn close_page(&mut self, seq: &Sequence) {
        if let Some(d) = self.description {
            self.sprites.close(seq, d, DESCRIPTION[2]);
        }
        let g = self.row_groups();
        for &r in &self.lists[self.list_index()].rows.clone() {
            self.sprites.close(seq, r, g[1]);
        }
    }

    /// FUN_1402b9200.
    fn page_alive(&self) -> bool {
        self.description.is_some_and(|d| self.sprites.alive(d))
            || self.page_sprites.iter().any(|&s| self.sprites.alive(s))
            || self
                .lists
                .iter()
                .any(|l| l.rows.iter().any(|&r| self.sprites.alive(r)))
    }

    fn kill_cursor(&mut self) {
        if let Some(p) = self.pointer.take() {
            self.sprites.kill(p);
        }
        self.light.kill(&mut self.sprites);
    }

    /// The sprites' own tasks (they run before the title task), including
    /// the orb's motion toward the cursor row.
    pub fn update_sprites(&mut self, seq: &Sequence) {
        self.sprites.update(seq);
        if self.light.id.is_some() {
            let l = &self.lists[self.list_index()];
            if let Some(&row) = l.rows.get(l.cursor) {
                let c = self.sprites.context(row);
                let g = &seq.groups[self.sprites.get(row).player.group];
                let target = [
                    c[0] + g.light_position_x as f32,
                    c[1] + g.text_position_y as f32,
                ];
                self.light.place(&mut self.sprites, target);
            }
        }
    }

    /// One title-task frame of the modal (FUN_1402b7420): returns the
    /// sounds; `finished` turns true when every sprite is gone.
    pub fn frame(&mut self, seq: &Sequence, trig: u64, repeat: u64) -> Vec<u32> {
        let mut sounds = Vec::new();
        self.frames += 1;
        match self.frames {
            // Construction, then a yield.
            1 => return sounds,
            2 => self.build_page(seq),
            _ => {}
        }
        if self.closing {
            let alive = self.heading.is_some_and(|h| self.sprites.alive(h))
                || self.page_alive()
                || self.pointer.is_some()
                || self.light.id.is_some();
            if !alive {
                self.finished = true;
            }
            return sounds;
        }
        let moved = self.page_frame(seq, trig, repeat, &mut sounds);
        self.driver(seq, moved);
        if self.done {
            if let Some(h) = self.heading {
                self.sprites.close(seq, h, HEADING_GROUPS[2]);
            }
            self.closing = true;
        }
        sounds
    }

    /// FUN_1402b7940; returns whether the cursor moved.
    fn page_frame(
        &mut self,
        seq: &Sequence,
        trig: u64,
        repeat: u64,
        sounds: &mut Vec<u32>,
    ) -> bool {
        let mut moved = false;
        if self.substate != 2 {
            let li = self.list_index();
            let (r, m) = self.lists[li].input(trig, repeat);
            moved = m;
            if (r != Pick::None || moved) && !self.intro_done() {
                self.skip_intro(seq);
            }
            if self.substate == 0 && self.intro_done() {
                self.substate = 1;
            }
            if moved {
                sounds.push(SOUND_MOVE);
            }
            match r {
                Pick::Start => sounds.push(SOUND_BUZZER),
                Pick::Cancel => {
                    sounds.push(SOUND_CANCEL);
                    self.close_page(seq);
                    self.substate = 2;
                    match self.page {
                        0 => self.done = true,
                        1 => self.next_page = 0,
                        _ => self.next_page = 1,
                    }
                }
                Pick::Row(i) => {
                    sounds.push(SOUND_DECIDE);
                    self.close_page(seq);
                    self.substate = 2;
                    if self.page < 2 {
                        self.next_page = self.page + 1;
                    } else if i == 0 {
                        self.next_page = 0;
                    } else {
                        self.done = true;
                        self.confirmed = true;
                    }
                }
                Pick::None => {}
            }
        } else if !self.page_alive() && !self.done {
            self.page = self.next_page;
            if self.page == 2 {
                self.build_confirm(seq);
            } else {
                self.build_page(seq);
            }
        }
        if self.done {
            self.kill_cursor();
        }
        moved
    }

    /// FUN_1402b7e40.
    fn driver(&mut self, seq: &Sequence, moved: bool) {
        if self.done || self.substate != 1 {
            self.kill_cursor();
            return;
        }
        let li = self.list_index();
        let cursor = self.lists[li].cursor;
        let Some(&row) = self.lists[li].rows.get(cursor) else {
            return;
        };
        if self.pointer.is_none() {
            let p = self
                .sprites
                .create(seq, PRIO_POINTER, self.sheet, POINTER, None, None);
            self.sprites.get_mut(p).parent = Some(row);
            self.pointer = Some(p);
            if self.light.id.is_none() {
                let l = self.sprites.create(
                    seq,
                    PRIO_POINTER,
                    self.sheet,
                    LIGHT[0],
                    Some(LIGHT[1]),
                    None,
                );
                self.light.id = Some(l);
            }
        }
        if moved {
            let g = self.row_groups();
            for (i, &r) in self.lists[li].rows.clone().iter().enumerate() {
                let group = if i == cursor { g[3] } else { g[2] };
                self.sprites.set(seq, r, group, None);
            }
            if let Some(p) = self.pointer {
                self.sprites.get_mut(p).parent = Some(row);
            }
            self.light.retarget();
            if self.page < 2 {
                self.values[self.page] = cursor;
                let help = self.texts.help[self.page][cursor].clone();
                if let Some(d) = self.description
                    && let Some(t) = self.sprites.get_mut(d).text.as_mut()
                {
                    t.bytes = help;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_wraps_on_fresh_presses_or_clamps() {
        let mut l = List {
            rows: vec![Id(0), Id(1), Id(2), Id(3)],
            cursor: 0,
            wrap: true,
            previous_mask: UP,
            next_mask: DOWN,
        };
        assert_eq!(l.input(0, UP), (Pick::None, false));
        assert_eq!(l.input(UP, UP), (Pick::None, true));
        assert_eq!(l.cursor, 3);
        let mut c = List {
            rows: vec![Id(0), Id(1)],
            cursor: 1,
            wrap: false,
            previous_mask: RIGHT,
            next_mask: LEFT,
        };
        assert_eq!(c.input(LEFT, LEFT), (Pick::None, false));
        assert_eq!(c.input(RIGHT, RIGHT), (Pick::None, true));
        assert_eq!(c.cursor, 0);
        assert_eq!(c.input(CANCEL, 0), (Pick::Cancel, false));
    }

    #[test]
    fn message_order_round_trips() {
        let ids = message_ids();
        let t = Texts::from_messages(ids.iter().map(|&i| vec![(i & 0xff) as u8]).collect());
        assert_eq!(t.items[0].len(), 4);
        assert_eq!(t.yes, vec![0x83]);
        assert_eq!(t.help[1][1], vec![0x3b]);
    }
}
