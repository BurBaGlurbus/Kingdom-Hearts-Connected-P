//! The camp's tutorial notices (state 0x25): FUN_140367300 builds the page
//! list, FUN_140367150 / FUN_1403662c0 load and drive each page,
//! FUN_140365dc0 builds the picture, strip and callouts, FUN_1403660c0 the
//! counter and heading, FUN_140366850 closes a page, FUN_140366aa0 names the
//! file (`menu/<lang>/<name>_<v>.bar`) and FUN_140366d80 reads the callout
//! table `THelpText.bin`. The picture and strip are camp.2ld sequence 0;
//! the heading and counter are sequence 1 sprites.
use super::*;
use data::sprite_sequence::{Player, Quad};
use std::collections::HashSet;

/// Table 0x1405bc3e0: file, story flag, category, variant mask.
pub const ROWS: [(&str, u16, u8, u8); 9] = [
    ("at_010", 0x084a, 0, 7),
    ("at_020", 0x084a, 0, 5),
    ("mp_010", 0x1006, 1, 4),
    ("mp_020", 0x1006, 1, 4),
    ("ab_010", 0x0848, 2, 7),
    ("ab_020", 0x0848, 2, 5),
    ("ab_040", 0x08aa, 2, 4),
    ("sw_010", 0x08aa, 3, 4),
    ("sw_020", 0x08aa, 3, 4),
];

/// Heading and page-count messages (0x1405bc480 and 0xcbd8).
pub const HEADING_IDS: [u32; 4] = [0x4bd9, 0x4bda, 0x4bdb, 0x4bdc];
pub const PAGE_ID: u32 = 0x4bd8;

/// camp.2ld sequence 0 (picture and strip) and sequence 1 (heading,
/// counter) groups.
const PICTURE: [usize; 3] = [1, 2, 3];
const STRIP: [usize; 3] = [4, 5, 6];
const HEADING: [usize; 3] = [15, 16, 17];
const COUNTER: [usize; 3] = [18, 19, 20];
const PRIO: u32 = 0x7546;
const CALLOUT_SIZE: f32 = 16.;
/// The callouts' language column (US = 1).
const LANGUAGE: usize = 1;

/// One callout: message and layout position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Callout {
    pub text: Vec<u8>,
    pub at: [i32; 2],
}

/// Everything the notices show: the story flags, the heading texts, the
/// "PAGE " text and the callouts per file name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoticeData {
    pub flags: Vec<u16>,
    pub headings: [Vec<u8>; 4],
    pub page: Vec<u8>,
    pub callouts: Vec<(String, Vec<Callout>)>,
}

/// THelpText.bin: a 0x10 header (count u16 at +6), then 0x180-byte records
/// of a 0x20-byte name, with five callouts per language at +0x90 + 0x30 k +
/// 8 L as `i32 message, i16 x, i16 y`. `text` maps a message id to its
/// bytes.
pub fn parse_callouts(bytes: &[u8], text: impl Fn(u32) -> Vec<u8>) -> Vec<(String, Vec<Callout>)> {
    let count = bytes
        .get(6..8)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize);
    (0..count)
        .filter_map(|r| {
            let rec = bytes.get(0x10 + r * 0x180..0x10 + (r + 1) * 0x180)?;
            let name: String = rec[..0x20]
                .iter()
                .take_while(|&&b| b != 0)
                .map(|&b| b as char)
                .collect();
            let callouts = (0..5)
                .filter_map(|k| {
                    let at = 0x90 + 0x30 * k + 8 * LANGUAGE;
                    let id = i32::from_le_bytes(rec[at..at + 4].try_into().ok()?);
                    let x = i16::from_le_bytes([rec[at + 4], rec[at + 5]]);
                    let y = i16::from_le_bytes([rec[at + 6], rec[at + 7]]);
                    (id != 0).then(|| Callout {
                        text: text(id as u32 & 0x7fff),
                        at: [x as i32, y as i32],
                    })
                })
                .collect();
            Some((name, callouts))
        })
        .collect()
}

/// The notice files the caller has loaded, and the one to load next
/// (FUN_140367150).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoticeCache {
    pub loaded: HashSet<String>,
    pub missing: HashSet<String>,
    pub wanted: Option<String>,
}

/// FUN_140366aa0: the variant letter from the story flags (r / o / s) and
/// the row's mask.
pub fn variant(flag: impl Fn(u16) -> bool, mask: u8) -> char {
    let v0 = if flag(0x8db) && !flag(0x8d0) {
        0
    } else if flag(0x8d0) {
        if flag(0x8d1) { 2 } else { 1 }
    } else {
        2
    };
    let v = if mask & !4 == 0 || (v0 == 1 && mask & 2 == 0) {
        2
    } else {
        v0
    };
    ['r', 'o', 's'][v]
}

/// A sequence 0 sprite: a player with a queued group and a close.
#[derive(Clone, Debug)]
struct Anim {
    player: Player,
    queued: Option<usize>,
    closing: bool,
    dead: bool,
}

impl Anim {
    fn new(seq: &Sequence, group: usize, queued: Option<usize>) -> Self {
        Self {
            player: Player::new(seq, group),
            queued,
            closing: false,
            dead: false,
        }
    }

    fn close(&mut self, seq: &Sequence, group: usize) {
        if self.dead || self.closing {
            return;
        }
        self.player = Player::new(seq, group);
        self.queued = None;
        self.closing = true;
    }

    fn update(&mut self, seq: &Sequence) {
        if self.dead {
            return;
        }
        self.player.update(seq, 1.);
        if seq.groups[self.player.group].play_count != 0 && self.player.finished() {
            if let Some(next) = self.queued.take() {
                self.player = Player::new(seq, next);
            } else if self.closing {
                self.dead = true;
            }
        }
    }
}

/// The page being shown (DAT_140bf01c8 and the sprite block).
#[derive(Debug, Default)]
pub struct Ui {
    /// State to return to once the last page is done.
    ret: Option<State>,
    /// Row indices of ROWS.
    pages: Vec<usize>,
    index: usize,
    /// File shown (or being waited for).
    file: Option<String>,
    picture: Option<Anim>,
    strip: Option<Anim>,
    callouts: Vec<Callout>,
    heading: Option<Id>,
    counter: Option<Id>,
}

impl Ui {
    /// FUN_140366e50: the page goes at once.
    pub fn kill(&mut self) {
        for a in [self.picture.as_mut(), self.strip.as_mut()]
            .into_iter()
            .flatten()
        {
            a.dead = true;
        }
        self.callouts.clear();
    }
}

impl Camp {
    fn flag_set(&self, f: u16) -> bool {
        self.data.notice.flags.contains(&f)
    }

    /// FUN_140367490: a notice of `category` has its flag.
    pub(super) fn notice_available(&self, category: u8) -> bool {
        ROWS.iter()
            .any(|&(_, f, c, _)| c == category && self.flag_set(f))
    }

    /// FUN_140367300(category, return state): the pages are every row of
    /// the category with its flag (all unseen rows when `category` is
    /// None). Returns whether a page was started.
    pub(super) fn start_notices(
        &mut self,
        seq: &Sequence,
        category: Option<u8>,
        ret: State,
    ) -> bool {
        let pages: Vec<usize> = (0..ROWS.len())
            .filter(|&i| {
                let (_, f, c, _) = ROWS[i];
                category.is_none_or(|k| k == c)
                    && self.flag_set(f)
                    && (category.is_some() || self.memory.notices_seen >> i & 1 == 0)
            })
            .collect();
        if pages.is_empty() {
            return false;
        }
        self.notice.ret = Some(ret);
        self.notice.pages = pages;
        self.notice.index = 0;
        self.notice.file = None;
        // FUN_140306000(1): no hand while the notice is up.
        self.item_ui.no_pointer = true;
        self.state = State::Notice;
        self.substate = 0;
        self.page_text(seq);
        true
    }

    fn page_file(&self, row: usize) -> String {
        let (name, _, _, mask) = ROWS[row];
        let v = variant(|f| self.flag_set(f), mask);
        format!("{name}_{v}.bar")
    }

    /// FUN_1403660c0: the "PAGE n/m" counter and the category heading.
    fn page_text(&mut self, seq: &Sequence) {
        let page = self.notice.index;
        let row = self.notice.pages[page.min(self.notice.pages.len() - 1)];
        let mut counter = self.data.notice.page.clone();
        counter.extend(encode_number(&format!(
            "{}/{}",
            page + 1,
            self.notice.pages.len()
        )));
        let alive = |s: &Self, id: Option<Id>| id.is_some_and(|i| s.sprites.alive(i));
        if !alive(self, self.notice.counter) {
            let mut t = Text::new(Vec::new(), Self::group_size(seq, COUNTER[0]));
            t.align = 2;
            let c = self
                .sprites
                .create(seq, PRIO, ART, COUNTER[0], Some(COUNTER[1]), Some(t));
            self.notice.counter = Some(c);
        }
        if let Some(c) = self.notice.counter
            && let Some(t) = self.sprites.get_mut(c).text.as_mut()
        {
            t.bytes = counter;
        }
        if !alive(self, self.notice.heading) {
            let mut t = Text::new(Vec::new(), Self::group_size(seq, HEADING[0]));
            t.align = 2;
            let h = self
                .sprites
                .create(seq, PRIO, ART, HEADING[0], Some(HEADING[1]), Some(t));
            self.notice.heading = Some(h);
        }
        let category = ROWS[row].2 as usize;
        if let Some(h) = self.notice.heading
            && let Some(t) = self.sprites.get_mut(h).text.as_mut()
        {
            t.bytes = self.data.notice.headings[category].clone();
        }
    }

    fn group_size(seq: &Sequence, group: usize) -> f32 {
        match seq.groups[group].text_scale {
            0 => TEXT_SIZE,
            s => s as f32,
        }
    }

    fn picture_dead(&self) -> bool {
        self.notice.picture.as_ref().is_none_or(|p| p.dead)
            && self.notice.strip.as_ref().is_none_or(|p| p.dead)
    }

    /// FUN_140366850.
    fn close_page(&mut self, layout: &Layout, heading_too: bool) {
        let seq0 = &layout.sequences[0];
        if heading_too
            && let Some(h) = self.notice.heading
            && self.sprites.alive(h)
        {
            self.sprites
                .close(&layout.sequences[SEQUENCE], h, HEADING[2]);
        }
        if let Some(p) = self.notice.picture.as_mut() {
            p.close(seq0, PICTURE[2]);
        }
        if let Some(s) = self.notice.strip.as_mut() {
            s.close(seq0, STRIP[2]);
        }
        self.notice.callouts.clear();
    }

    /// FUN_1403662c0.
    pub(super) fn notice(&mut self, layout: &Layout, pad: &Pad, sounds: &mut Vec<u32>) -> bool {
        let seq = &layout.sequences[SEQUENCE];
        let seq0 = &layout.sequences[0];
        let n = self.notice.pages.len();
        let i = self.notice.index;
        let has_next = i + 1 < n;
        let differs = has_next && ROWS[self.notice.pages[i]].2 != ROWS[self.notice.pages[i + 1]].2;
        let alive = |s: &Self, id: Option<Id>| id.is_some_and(|x| s.sprites.alive(x));
        if self.substate == 2 {
            let busy = if has_next {
                (differs && alive(self, self.notice.heading)) || !self.picture_dead()
            } else {
                alive(self, self.notice.counter)
                    || alive(self, self.notice.heading)
                    || !self.picture_dead()
            };
            if busy {
                return false;
            }
            self.notice.index += 1;
            self.notice.file = None;
            self.notice.picture = None;
            self.notice.strip = None;
            if self.notice.index < n {
                self.page_text(seq);
                self.substate = 0;
            } else {
                // Every flagged row is marked seen (save+0x4270).
                for (r, &(_, f, _, _)) in ROWS.iter().enumerate() {
                    if self.flag_set(f) {
                        self.memory.notices_seen |= 1 << r;
                    }
                }
                self.item_ui.no_pointer = false;
                self.state = self.notice.ret.take().unwrap_or(State::Top);
                self.substate = 0;
            }
            return false;
        }
        // Load and build the page once the last one is gone.
        if self.notice.file.is_none() {
            let file = self.page_file(self.notice.pages[i]);
            if self.notice_files.missing.contains(&file) {
                // No such picture in this install: skip the page.
                self.notice.pages.remove(i);
                if self.notice.pages.is_empty() || i >= self.notice.pages.len() {
                    self.item_ui.no_pointer = false;
                    self.state = self.notice.ret.take().unwrap_or(State::Top);
                    self.substate = 0;
                } else {
                    self.page_text(seq);
                }
                return false;
            }
            if !self.notice_files.loaded.contains(&file) {
                self.notice_files.wanted = Some(file);
                return false;
            }
            self.notice.picture = Some(Anim::new(seq0, PICTURE[0], Some(PICTURE[1])));
            self.notice.strip =
                (has_next && !differs).then(|| Anim::new(seq0, STRIP[0], Some(STRIP[1])));
            self.notice.callouts = self
                .data
                .notice
                .callouts
                .iter()
                .find(|(name, _)| *name == file)
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            self.notice.file = Some(file);
        }
        // Intros done: the notice takes input.
        if self.substate == 0 {
            let idle = |a: &Option<Anim>, g: usize| {
                a.as_ref().is_none_or(|a| a.dead || a.player.group == g)
            };
            let sprite_idle = |s: &Self, id: Option<Id>, g: usize| {
                id.is_none_or(|x| !s.sprites.alive(x) || s.sprites.get(x).player.group == g)
            };
            if !(idle(&self.notice.picture, PICTURE[1])
                && idle(&self.notice.strip, STRIP[1])
                && sprite_idle(self, self.notice.heading, HEADING[1])
                && sprite_idle(self, self.notice.counter, COUNTER[1]))
            {
                return false;
            }
            self.substate = 1;
        }
        if pad.trig & bits::DECIDE == 0 {
            return false;
        }
        sounds.push(SOUND_DECIDE);
        let heading_too = if has_next {
            differs
        } else {
            if let Some(c) = self.notice.counter
                && self.sprites.alive(c)
            {
                self.sprites.close(seq, c, COUNTER[2]);
            }
            true
        };
        self.close_page(layout, heading_too);
        self.substate = 2;
        false
    }

    /// The notice's sequence 0 players advance with the frame.
    pub(super) fn notice_update(&mut self, layout: &Layout) {
        let seq0 = &layout.sequences[0];
        for a in [self.notice.picture.as_mut(), self.notice.strip.as_mut()]
            .into_iter()
            .flatten()
        {
            a.update(seq0);
        }
    }

    /// The picture and strip quads (sheet = the notice's file) and the
    /// callouts' text, drawn at 0x7546.
    pub(super) fn notice_draw(&self, layout: &Layout, font: &Font) -> Vec<(Sheet, Vec<Quad>)> {
        let mut out = Vec::new();
        let (Some(file), Some(p)) = (self.notice.file.clone(), self.notice.picture.as_ref()) else {
            return out;
        };
        let seq0 = &layout.sequences[0];
        let m = Modifiers::default();
        if !p.dead {
            out.push((
                Sheet::Notice(file.clone()),
                seq0.quads(&p.player, [0., 0.], &m),
            ));
        }
        if let Some(s) = self.notice.strip.as_ref().filter(|s| !s.dead) {
            out.push((Sheet::Notice(file), seq0.quads(&s.player, [0., 0.], &m)));
        }
        if p.dead || self.notice.callouts.is_empty() {
            return out;
        }
        // The callouts ride the picture's animation (group 1 → 2).
        // (the picture quad starts at the group's origin).
        let first = seq0.quads(&p.player, [0., 0.], &m);
        let ctx = first.first().map_or([0.; 2], |q| q.corners[0]);
        let alpha = first.first().map_or(1., |q| q.colors[0][3]);
        let rgba = [
            0xe5 as f32 / 128.,
            0xe5 as f32 / 128.,
            0xe5 as f32 / 128.,
            alpha,
        ];
        for c in &self.notice.callouts {
            let at = [ctx[0] + c.at[0] as f32, ctx[1] + c.at[1] as f32];
            let q = sprite_player::text_quads(&c.text, at, CALLOUT_SIZE, 0, 0., rgba, true, font);
            out.push((Sheet::Font, q.glyphs));
            if !q.icons.is_empty() {
                out.push((Sheet::Icon, q.icons));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_follow_the_story_flags_and_mask() {
        let none = |_: u16| false;
        // No story flags: Sora's set for every row.
        assert_eq!(variant(none, 7), 's');
        // The prologue set when 0x8db is set and 0x8d0 is not.
        assert_eq!(variant(|f| f == 0x8db, 7), 'r');
        // The transition set; rows without bit 1 fall back to Sora's.
        assert_eq!(variant(|f| f == 0x8d0, 7), 'o');
        assert_eq!(variant(|f| f == 0x8d0, 5), 's');
        // A row with only the Sora picture is always 's'.
        assert_eq!(variant(|f| f == 0x8db, 4), 's');
    }

    #[test]
    fn callouts_read_the_us_column() {
        let mut b = vec![0u8; 0x10 + 0x180];
        b[6] = 1;
        b[0x10..0x10 + 12].copy_from_slice(b"mp_020_s.bar");
        // Callout 0, language 1: message 22046 at (216, 308).
        let at = 0x10 + 0x90 + 8;
        b[at..at + 4].copy_from_slice(&22046i32.to_le_bytes());
        b[at + 4..at + 6].copy_from_slice(&216i16.to_le_bytes());
        b[at + 6..at + 8].copy_from_slice(&308i16.to_le_bytes());
        let t = parse_callouts(&b, |id| vec![(id & 0xff) as u8]);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].0, "mp_020_s.bar");
        assert_eq!(t[0].1.len(), 1);
        assert_eq!(t[0].1[0].at, [216, 308]);
        assert_eq!(t[0].1[0].text, vec![0x1e]);
    }
}
