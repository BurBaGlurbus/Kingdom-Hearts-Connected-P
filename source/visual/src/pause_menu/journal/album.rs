//! Album: state 4 lists the world's photos (FUN_14032a8a0 /
//! FUN_1403298f0), state 5 shows one (FUN_14032a320 / FUN_14032a110) with
//! L2 / R2 cycling the unlocked ones (FUN_140329f40). The photo file is
//! `menu/<lang>/jm_photo/<worldcode><nn>.bin`, one 512x256 IMGD.
use super::*;

const LIST_PANEL: usize = 90;
const ROW: usize = 2;
const MARK: [usize; 2] = [92, 93];
const PHOTO_PANEL: usize = 86;
const CAPTION: usize = 87;
const TITLE_GROUP: usize = 88;
/// jmemo sequence 1: the feathered 382x222 photo frame.
const PHOTO_SEQ: usize = 1;
const PRIO_PANEL: u32 = 0x755c;
const PRIO_NEW: u32 = 0x755e;
const HELP: [u16; 2] = [0x48d8, 0x48d9];
const EMPTY: u16 = 0x32dd;
const ROWS: usize = 8;
const PITCH: f32 = 26.;
const SIZE: f32 = 18.;
/// The photo's offset on the panel (G86 LightPositionX, TextPositionY).
const PHOTO_OFFSET: [f32; 2] = [160., -113.];

#[derive(Default)]
pub(super) struct Page {
    /// `albu` rows of the world: listed ones.
    visible: Vec<usize>,
    rows: Vec<Id>,
    marks: Vec<Id>,
    panel: Option<Id>,
    /// The visible index shown on the photo page.
    selected: usize,
    photo_made: bool,
}

impl Journal {
    fn album_unlocked(&self, row: usize) -> bool {
        self.data.albums.get(row).is_some_and(|a| self.flag(a.flag))
    }

    /// FUN_14032a8a0.
    pub(super) fn build_album_list(&mut self, layout_: &Layout) {
        let seq = &layout_.sequences[SEQUENCE];
        self.reset_list();
        self.album = Page::default();
        self.no_light = true;
        self.category_frame(seq);
        let c = self.container;
        let panel = self.page_sprite(seq, PRIO_PANEL, LIST_PANEL, c, None);
        self.album.panel = Some(panel);
        for i in 0..ROWS {
            let mut t = Self::text(Vec::new(), SIZE);
            t.group_position = false;
            t.keep_rgb = false;
            let r = self.page_sprite(seq, BASE, ROW, panel, Some(t));
            self.sprites.get_mut(r).offset = [0., PITCH * i as f32];
            let m = self.page_sprite(seq, PRIO_NEW, ROW, r, None);
            self.album.rows.push(r);
            self.album.marks.push(m);
        }
        let all = self.show_all();
        self.album.visible = (0..self.data.albums.len())
            .filter(|&r| {
                let a = &self.data.albums[r];
                a.world as i32 == self.world && (self.flag(a.flag) || all)
            })
            .take(ROWS)
            .collect();
        let count = self.album.visible.len();
        if count == 0 {
            self.no_cursor = true;
            let text = self.msg(EMPTY);
            self.set_text(self.album.rows.first().copied(), text);
        } else {
            for (i, &r) in self.album.visible.clone().iter().enumerate() {
                let a = self.data.albums[r].clone();
                let unlocked = self.flag(a.flag);
                // FUN_1403008d0: five "?" for a locked row.
                let text = if unlocked {
                    self.msg(a.title)
                } else {
                    vec![0x49; 5]
                };
                self.set_text(Some(self.album.rows[i]), text);
                let new = unlocked && !self.seen_albums_at_open.contains(&(r as u16));
                if new {
                    let seen = self.seen_albums.contains(&(r as u16));
                    self.sprites
                        .set(seq, self.album.marks[i], MARK[seen as usize], None);
                }
            }
        }
        self.list = MenuList::new(
            self.album.rows.clone(),
            self.memory[4].min(count.saturating_sub(1)),
        );
        self.list.total = count.max(1);
        self.list.mask = 0x73;
        if count == 0 {
            self.list.mask &= !0x10;
        }
        self.memory[4] = 0;
        self.show_help(seq, HELP[0]);
        self.album_mark_seen(seq);
    }

    /// FUN_14032b0a0: the row under the cursor (state 4) or on the photo
    /// page counts as seen; its mark goes dim.
    fn album_mark_seen(&mut self, seq: &Sequence) {
        let i = if self.state == 4 {
            self.list.cursor
        } else {
            self.album.selected
        };
        let Some(&r) = self.album.visible.get(i) else {
            return;
        };
        if !self.album_unlocked(r) {
            return;
        }
        if self.seen_albums.insert(r as u16)
            && self.state == 4
            && let Some(&m) = self.album.marks.get(i)
            && !self.seen_albums_at_open.contains(&(r as u16))
        {
            self.sprites.set(seq, m, MARK[1], None);
        }
    }

    /// FUN_14032a320.
    pub(super) fn build_album_photo(&mut self, layout_: &Layout) {
        let seq = &layout_.sequences[SEQUENCE];
        self.reset_list();
        self.no_cursor = true;
        self.category_frame(seq);
        let c = self.container;
        let panel = self.page_sprite(seq, PRIO_PANEL, PHOTO_PANEL, c, None);
        self.album.panel = Some(panel);
        self.album.photo_made = false;
        let row = self.album.visible.get(self.album.selected).copied();
        let a = row.and_then(|r| self.data.albums.get(r)).cloned();
        let mut t = Self::text(
            a.as_ref().map(|a| self.msg(a.title)).unwrap_or_default(),
            SIZE,
        );
        t.group_position = false;
        t.keep_rgb = false;
        self.page_sprite(seq, PRIO_PANEL, TITLE_GROUP, c, Some(t));
        let mut t = Self::text(
            a.as_ref().map(|a| self.msg(a.caption)).unwrap_or_default(),
            SIZE,
        );
        t.group_position = false;
        t.keep_rgb = false;
        let cap = self.page_sprite(seq, BASE, CAPTION, panel, Some(t));
        // The caption row is the only list row; R2 / L2 page the photos.
        self.list = MenuList::new(vec![cap], 0);
        self.list.mask = (0x73 | 0xc000) & !0x10;
        self.show_help(seq, HELP[1]);
        self.album_mark_seen(seq);
    }

    /// The photo appears once its file is loaded (FUN_14032ad90).
    fn album_photo_create(&mut self, layout_: &Layout) {
        if self.album.photo_made {
            return;
        }
        let Some(&r) = self.album.visible.get(self.album.selected) else {
            return;
        };
        if !self.cache.photos.contains(&(r as u16)) {
            self.cache.wanted_photo = Some(r as u16);
            return;
        }
        let Some(panel) = self.album.panel else {
            return;
        };
        self.album.photo_made = true;
        self.pictures.push(Picture {
            prio: BASE,
            sequence: PHOTO_SEQ,
            sheet: Sheet::Photo(r as u16),
            player: Player::new(&layout_.sequences[PHOTO_SEQ], 1),
            queued: Some(2),
            parent: panel,
            offset: PHOTO_OFFSET,
            dead: false,
        });
    }

    /// FUN_1403298f0.
    pub(super) fn album_list(
        &mut self,
        layout_: &Layout,
        pad: &Pad,
        sounds: &mut Vec<u32>,
    ) -> bool {
        let seq = &layout_.sequences[SEQUENCE];
        let (input, moved) = self.input(pad);
        if self.substate == 2 {
            self.leave(layout_, input, moved, true);
            return false;
        }
        self.settle(seq, input, moved, true);
        let valid = match input {
            In::Row(k) => self
                .album
                .visible
                .get(k)
                .is_some_and(|&r| self.album_unlocked(r)),
            _ => true,
        };
        if let Some(s) = sound(input, moved, valid) {
            sounds.push(s);
        }
        if moved && self.substate == 1 {
            self.album_mark_seen(seq);
        }
        if !valid {
            return moved;
        }
        match input {
            In::Start => self.exit = true,
            In::Cancel => {
                self.pill_out(seq);
                self.hide_help(seq);
                self.slide(seq, 1, 1);
                self.pending = Some(NOTES_PAGE);
                self.substate = 2;
            }
            In::Row(k) => {
                self.album.selected = k;
                self.memory[4] = k;
                self.hide_help(seq);
                self.slide(seq, 0, 1);
                self.pending = Some(5);
                self.substate = 2;
            }
            _ => {}
        }
        moved
    }

    /// FUN_14032a110.
    pub(super) fn album_photo(
        &mut self,
        layout_: &Layout,
        pad: &Pad,
        sounds: &mut Vec<u32>,
    ) -> bool {
        let seq = &layout_.sequences[SEQUENCE];
        let (input, moved) = self.input(pad);
        if self.substate == 2 {
            self.leave(layout_, input, moved, true);
            return false;
        }
        self.album_photo_create(layout_);
        self.settle(seq, input, moved, true);
        // FUN_14032a7a0: R2 / L2 need more than one unlocked photo.
        let unlocked = self
            .album
            .visible
            .iter()
            .filter(|&&r| self.album_unlocked(r))
            .count();
        let valid = match input {
            In::R2 | In::L2 => unlocked > 1,
            _ => true,
        };
        if let Some(s) = sound(input, moved, valid) {
            sounds.push(s);
        }
        if !valid {
            return moved;
        }
        match input {
            In::Start => self.exit = true,
            In::Cancel => {
                self.hide_help(seq);
                self.slide(seq, 1, 1);
                self.memory[4] = self.album.selected;
                self.pending = Some(4);
                self.substate = 2;
            }
            In::R2 | In::L2 => {
                let n = self.album.visible.len();
                let i = self.album.selected;
                let un = |s: &Self, j: usize| s.album_unlocked(s.album.visible[j]);
                self.album.selected = if input == In::R2 {
                    (i + 1..n)
                        .find(|&j| un(self, j))
                        .or_else(|| (0..i).find(|&j| un(self, j)))
                } else {
                    (0..i)
                        .rev()
                        .find(|&j| un(self, j))
                        .or_else(|| (i + 1..n).rev().find(|&j| un(self, j)))
                }
                .unwrap_or(i);
                self.slide(seq, (input != In::R2) as usize, 1);
                self.pending = Some(5);
                self.substate = 2;
            }
            _ => {}
        }
        moved
    }
}
