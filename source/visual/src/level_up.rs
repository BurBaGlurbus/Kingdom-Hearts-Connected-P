//! The level-up panels (`dk::LEVELUP_INFO`, drivefx-02 §1): queue A holds a
//! character's level-ups (`FUN_14017dd80`) and bonus levels
//! (`FUN_14017dd40`) at the top right; queue B the form level-ups
//! (`FUN_14017ddc0`) at the left. Each queue starts up to four panels as one
//! batch of 'gaug' sprites stacked by the group's padding (`FUN_14017e050` /
//! `FUN_14017e1d0`), and `FUN_14017d630` / `FUN_14017d6f0` draw the title,
//! level and two text lines over each.
use crate::field_command_menu::Font;
use crate::gauge::Draw;
use crate::sprite_player::text_quads;
use data::sprite_sequence::{Modifiers, Player, Sequence};
use std::collections::VecDeque;

/// DAT_14071ad78: the {one line, two lines} groups by form (0 = Sora).
pub const FORM_GROUPS: [[usize; 2]; 7] = [
    [0x5c, 0x5d],
    [0x5e, 0x5f],
    [0x5a, 0x5b],
    [0x58, 0x59],
    [0x62, 0x63],
    [0x60, 0x61],
    [0x60, 0x61],
];
/// DAT_14071ad38: a character's level-up groups by character index
/// (Sora, Donald, Goofy, others).
pub const LEVEL_GROUPS: [[usize; 2]; 4] = [[0x68, 0x69], [0x64, 0x65], [0x66, 0x67], [0x6a, 0x6b]];
/// DAT_14071ad58: the bonus-level groups by character index.
pub const BONUS_GROUPS: [[usize; 2]; 4] = [[0x54, 0x55], [0x50, 0x51], [0x52, 0x53], [0x56, 0x57]];
/// Form panels anchor to the left edge of the 16:9 frame, queue A to the
/// right one.
const FORM_ORIGIN: f32 = -85.;
const NORMAL_ORIGIN: f32 = 85.;
/// Text size 14 against the font's 18-unit cell.
const TEXT_SIZE: f32 = 14.;
/// Pending panels a queue keeps (`FUN_14017ddc0` drops a fourth, queue A
/// holds 15).
const FORM_PENDING: usize = 3;
const NORMAL_PENDING: usize = 15;

/// Which queue and group table an entry uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A Drive form's level-up (queue B).
    Form,
    /// A character's level-up (queue A, kind 0).
    Level,
    /// A bonus level (queue A, kind 1).
    Bonus,
}

/// One queued panel: its kind, the form (forms) or the character index
/// (0 Sora, 1 Donald, 2 Goofy, 3 other), its title, level (the bonus count
/// for a bonus) and one or two encoded lines.
pub struct Entry {
    pub kind: Kind,
    pub form: u8,
    pub level: u8,
    pub title: Vec<u8>,
    pub line1: Vec<u8>,
    pub line2: Option<Vec<u8>>,
}

impl Entry {
    fn group(&self) -> usize {
        let two = self.line2.is_some() as usize;
        match self.kind {
            Kind::Form => FORM_GROUPS[(self.form as usize).min(6)][two],
            Kind::Level => LEVEL_GROUPS[(self.form as usize).min(3)][two],
            Kind::Bonus => BONUS_GROUPS[(self.form as usize).min(3)][two],
        }
    }

    fn origin(&self) -> f32 {
        match self.kind {
            Kind::Form => FORM_ORIGIN,
            _ => NORMAL_ORIGIN,
        }
    }

    /// Form panels print the level only for a form (Sora's own panel has
    /// none); queue A always does.
    fn shows_level(&self) -> bool {
        self.kind != Kind::Form || self.form != 0
    }
}

struct Panel {
    entry: Entry,
    player: Player,
    /// The stack offset (+0x1c4).
    y: f32,
}

/// One queue: the pending panels and the batch that plays.
#[derive(Default)]
struct Queue {
    pending: VecDeque<Entry>,
    batch: Vec<Panel>,
}

impl Queue {
    fn update(&mut self, seq: &Sequence) {
        for p in &mut self.batch {
            p.player.update(seq, 1.);
        }
        if self.batch.first().is_some_and(|p| p.player.finished()) {
            self.batch.clear();
        }
        if !self.batch.is_empty() {
            return;
        }
        let mut y = 0.;
        while self.batch.len() < 4
            && let Some(entry) = self.pending.pop_front()
        {
            let group = entry.group();
            let Some(g) = seq.groups.get(group) else {
                continue;
            };
            let padding = g.ui_padding as f32;
            self.batch.push(Panel {
                entry,
                player: Player::new(seq, group),
                y,
            });
            y += padding;
        }
    }
}

#[derive(Default)]
pub struct Panels {
    form: Queue,
    normal: Queue,
}

impl Panels {
    /// `FUN_14017ddc0` / `FUN_14017df70`: a panel past the queue's pending
    /// limit is dropped.
    pub fn push(&mut self, entry: Entry) {
        let (queue, limit) = match entry.kind {
            Kind::Form => (&mut self.form, FORM_PENDING),
            _ => (&mut self.normal, NORMAL_PENDING),
        };
        if queue.pending.len() < limit {
            queue.pending.push_back(entry);
        }
    }

    /// Whether any panel is playing or waiting.
    pub fn busy(&self) -> bool {
        [&self.form, &self.normal]
            .iter()
            .any(|q| !q.batch.is_empty() || !q.pending.is_empty())
    }

    /// The queues' step (`FUN_14017e1d0`): a batch plays; once its first
    /// panel is gone the pending ones start together, each 44 (one line) or
    /// 64 (two lines) units above the previous.
    pub fn update(&mut self, seq: &Sequence) {
        self.form.update(seq);
        self.normal.update(seq);
    }

    /// The panels' sprites, then their text (`FUN_14017d6f0` /
    /// `FUN_14017d630`): line 1 left at the group's (+0x10, +0x14), the
    /// title right-aligned at the text anchor, the level ("%2d") at +0x20
    /// and line 2 17 units under line 1. `sheet` is the gauge sheet,
    /// `font_sheet` the system font's.
    pub fn draw(&self, seq: &Sequence, font: &Font, sheet: usize, font_sheet: usize) -> Vec<Draw> {
        let mut out = Vec::new();
        let m = Modifiers::default();
        for p in self.form.batch.iter().chain(&self.normal.batch) {
            let origin = [p.entry.origin(), p.y];
            out.push(Draw::Quads {
                quads: seq.quads(&p.player, origin, &m),
                z: 0,
                sheet,
            });
            let g = &seq.groups[p.player.group];
            let ctx = seq.child_context(&p.player, &m).unwrap_or([0.; 2]);
            let alpha = seq
                .context_color(&p.player, &m)
                .map_or(1., |c| c[3].min(1.));
            if alpha <= 0. {
                continue;
            }
            let rgba = [1., 1., 1., alpha];
            let (x, y) = (origin[0] + ctx[0], origin[1] + ctx[1]);
            // `FUN_1403afd90`'s y is the top of the line, as the menus' text
            // (sprite_player::text_quads): font 1 (shadowed) for the title
            // and level, font 0 for the lines.
            let mut text = |s: &[u8], at: [f32; 2], align: u8, shadow: bool| {
                let q = text_quads(s, at, TEXT_SIZE, align, 0., rgba, shadow, font);
                out.push(Draw::Quads {
                    quads: q.glyphs,
                    z: 0,
                    sheet: font_sheet,
                });
            };
            let e = &p.entry;
            let left = x + g.light_position_x as f32;
            text(&e.line1, [left, y + g.text_position_y as f32], 0, false);
            if let Some(l2) = &e.line2 {
                text(l2, [left, y + 17. + g.text_position_y as f32], 0, false);
            }
            text(&e.title, [x, y], 1, true);
            if e.shows_level() {
                let level = crate::pause_menu::encode_number(&format!("{:2}", e.level));
                text(&level, [x + g.text_position_x as f32, y], 0, true);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: Kind, form: u8, two: bool) -> Entry {
        Entry {
            kind,
            form,
            level: 5,
            title: Vec::new(),
            line1: Vec::new(),
            line2: two.then(Vec::new),
        }
    }

    #[test]
    fn groups_follow_the_exe_tables() {
        assert_eq!(entry(Kind::Level, 0, false).group(), 0x68);
        assert_eq!(entry(Kind::Level, 0, true).group(), 0x69);
        assert_eq!(entry(Kind::Level, 1, false).group(), 0x64);
        assert_eq!(entry(Kind::Bonus, 0, false).group(), 0x54);
        assert_eq!(entry(Kind::Bonus, 2, true).group(), 0x53);
        assert_eq!(entry(Kind::Form, 4, true).group(), 0x63);
        // An out-of-range character index uses the last row.
        assert_eq!(entry(Kind::Level, 9, false).group(), 0x6a);
    }

    #[test]
    fn queues_anchor_and_limit_independently() {
        assert_eq!(entry(Kind::Form, 1, false).origin(), -85.);
        assert_eq!(entry(Kind::Level, 0, false).origin(), 85.);
        assert!(!entry(Kind::Form, 0, false).shows_level());
        assert!(entry(Kind::Level, 0, false).shows_level());
        let mut p = Panels::default();
        for _ in 0..5 {
            p.push(entry(Kind::Form, 1, false));
        }
        assert_eq!(p.form.pending.len(), FORM_PENDING);
        for _ in 0..20 {
            p.push(entry(Kind::Level, 0, false));
        }
        assert_eq!(p.normal.pending.len(), NORMAL_PENDING);
        assert!(p.busy());
    }
}
