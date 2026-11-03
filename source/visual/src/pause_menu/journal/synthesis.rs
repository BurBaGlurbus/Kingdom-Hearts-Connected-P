//! Synthesis Notes (state 0x27, Collection option 10; FUN_14033a7d0 /
//! FUN_1403397b0 / FUN_1403395e0): four lines of synthesis statistics -
//! the Moogle's level, the materials found, the collection lists
//! completed and the recipes synthesized.
use super::*;

pub(super) const STATE: u8 = 0x27;
/// The panel (ctx (60, 92), pad 28), the line groups (open, complete) and
/// a blank row.
const PANEL: usize = 101;
const LINE: [usize; 2] = [106, 107];
const BLANK: usize = 2;
/// NEW label (static).
const MARK: usize = 58;
const PRIO_PANEL: u32 = 0x755c;
const PRIO_NEW: u32 = 0x755f;
const PITCH: f32 = 28.;
const TEXT_COLOUR: u32 = 0x6000_0008;
/// Labels: "Moogle Level{u72}", "Types of materials obtained", "Complete
/// collection lists", "Synthesized recipes"; the masked label and the help.
const LABELS: [u16; 4] = [0x4b37, 0x4b38, 0x4b39, 0x4b3a];
const MASKED: u16 = 0x36f8;
const HELP: u16 = 0x48f9;
/// What counts as everything (mixdata: 60 materials, 54 collection lists, 59
/// recipes; the Moogle's top level is 9).
const TOTALS: [u32; 4] = [9, 60, 54, 59];

/// The save's synthesis fields (save+0x49f4 / 0x49fc / 0x4a10 counts, the
/// Moogle seen bits at +0x4a28 and the level at +0x4c34).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Synthesis {
    pub moogle_seen: bool,
    /// 0..8.
    pub level: u8,
    pub materials: u32,
    pub collections: u32,
    pub recipes: u32,
}

#[derive(Default)]
pub(super) struct Page {
    ids: Vec<Id>,
    /// Lines seen so far and when the journal opened (save+0x4d78).
    pub seen: HashSet<u8>,
    pub seen_at_open: HashSet<u8>,
}

impl Journal {
    /// FUN_14033aa40 / FUN_14033a9f0: the lines the save has reached.
    fn synthesis_available(&self, line: usize) -> bool {
        let s = self.data.synthesis;
        match line {
            0 | 3 => s.moogle_seen,
            _ => s.materials > 0,
        }
    }

    /// FUN_14033a980: the Collection option shows.
    pub(super) fn synthesis_any(&self) -> bool {
        self.show_all() || self.synthesis_available(0) || self.synthesis_available(1)
    }

    fn synthesis_count(&self, line: usize) -> u32 {
        let s = self.data.synthesis;
        match line {
            0 => s.level as u32 + 1,
            1 => s.materials,
            2 => s.collections,
            _ => s.recipes,
        }
    }

    /// FUN_1403498f0: a line is complete when it has reached its total.
    fn synthesis_complete(&self, line: usize) -> bool {
        self.synthesis_available(line) && self.synthesis_count(line) >= TOTALS[line]
    }

    /// FUN_14033a7d0 + FUN_1403397b0.
    pub(super) fn build_synthesis(&mut self, layout_: &Layout) {
        use crate::pause_menu::encode_number;
        let seq = &layout_.sequences[SEQUENCE];
        self.reset_list();
        self.no_cursor = true;
        self.cat.detail = false;
        self.category_frame(seq);
        let c = self.container;
        let panel = self.page_sprite(seq, PRIO_PANEL, PANEL, c, None);
        let all = self.show_all();
        self.synthesis.ids.clear();
        let size = Self::size(seq, LINE[0]);
        let mut slot = 0;
        for line in 0..4 {
            let available = self.synthesis_available(line);
            if !available && !all {
                continue;
            }
            let group = LINE[self.synthesis_complete(line) as usize];
            let (label, value) = if !available {
                (self.msg(MASKED), None)
            } else if line == 0 {
                let mut l = self.msg(LABELS[0]);
                l.extend(encode_number(&self.synthesis_count(0).to_string()));
                let level = self.data.synthesis.level as usize;
                (
                    l,
                    Some(
                        self.data
                            .moogle_names
                            .get(level)
                            .cloned()
                            .unwrap_or_default(),
                    ),
                )
            } else {
                let n = self.synthesis_count(line);
                let v = if all {
                    format!("{n}/{}", TOTALS[line])
                } else {
                    n.to_string()
                };
                (self.msg(LABELS[line]), Some(encode_number(&v)))
            };
            let mut t = Self::text(label, size);
            t.group_position = false;
            t.colour = TEXT_COLOUR;
            let row = self.page_sprite(seq, BASE, group, panel, Some(t));
            self.sprites.get_mut(row).offset = [0., PITCH * slot as f32];
            self.synthesis.ids.push(row);
            if let Some(v) = value {
                let mut t = Self::text(v, size);
                t.group_position = false;
                t.align = 1;
                t.colour = TEXT_COLOUR;
                let id = self.page_sprite(seq, BASE, BLANK, row, Some(t));
                self.sprites.get_mut(id).offset = [seq.groups[group].light_position_x as f32, 0.];
            }
            if available && !self.synthesis.seen_at_open.contains(&(line as u8)) {
                self.page_sprite(seq, PRIO_NEW, MARK, row, None);
            }
            // FUN_140346750: the four lines are marked seen.
            self.synthesis.seen.insert(line as u8);
            slot += 1;
        }
        let carrier = self.page_sprite(seq, BASE, BLANK, panel, None);
        self.list = MenuList::new(vec![carrier], 0);
        self.list.mask = 0x63;
        self.show_help(seq, HELP);
    }

    /// FUN_1403395e0 (not leaving).
    pub(super) fn synthesis_page(
        &mut self,
        layout_: &Layout,
        pad: &Pad,
        sounds: &mut Vec<u32>,
    ) -> bool {
        let seq = &layout_.sequences[SEQUENCE];
        let (input, moved) = self.input(pad);
        if self.substate == 2 {
            self.leave(layout_, input, moved, false);
            return false;
        }
        self.settle(seq, input, moved, true);
        if let Some(s) = sound(input, moved, true) {
            sounds.push(s);
        }
        match input {
            In::Start => self.exit = true,
            In::Cancel => {
                self.pill_out(seq);
                self.hide_help(seq);
                self.slide(seq, 1, 1);
                self.pending = Some(COLLECTION_PAGE);
                self.substate = 2;
            }
            _ => {}
        }
        moved
    }
}
