//! A party limit's gauge and combo counter, translated from the
//! executable's mission widgets: YS::MISSION_GAUGE (the limit object +0xe0)
//! shows dk::MISSION_GAUGE (FUN_14017b750 → FUN_14018c990, per frame
//! FUN_14018cf00, refresh FUN_14018d040) and YS::COMBOCOUNTER (+0x88, show
//! FUN_1403fa730) shows dk::COUNTER in its two-row mode (FUN_14017b500 →
//! FUN_14018be20, per frame FUN_14018c560). Both draw from the limit file's
//! own sheets (`gage`, `comb`; FUN_1403d94d0) and stack in the HUD slot
//! table at their kind (0xc) and side (FUN_140182b10 / FUN_140182c60).
use crate::gauge::Draw;
use crate::hud_slots::HudSlots;
use data::sprite_sequence::{Modifiers, Player, Sequence};
use player::follow_camera::spring_with;

/// Which edge a widget sits on (the widget's +0x2c): left, right, centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left = 0,
    Right = 1,
    Centre = 2,
}

/// The HUD column offset the left and right sides add (0x55).
const SIDE_SHIFT: f32 = 85.;
/// The limit widgets' slot (YS::MISSION2D +0x30, FUN_1403fa400(.., 0xc)).
pub const LIMIT_KIND: usize = 0xc;

/// Gauge sprites (FUN_14018c990): frame, fill, rest, marker, cap.
const GAUGE_GROUPS: [usize; 5] = [3, 0xc, 0xd, 0xe, 9];
/// The group whose position anchors each side (0x10 left, 0x11 right, 0xf
/// centre).
const GAUGE_ANCHOR: [usize; 3] = [0x10, 0x11, 0xf];
/// Counter sprites (FUN_14018be20): frame 3, label 9, digits 0xc; the
/// second row: separator 0x17, digits 0x18. Anchor groups 0x24, 0x25, 0x23.
const COUNTER_ANCHOR: [usize; 3] = [0x24, 0x25, 0x23];
const COUNTER_FRAME: usize = 3;
const COUNTER_LABEL: usize = 9;
const DIGIT: usize = 0xc;
const SEPARATOR: usize = 0x17;
const DIGIT_2: usize = 0x18;
/// Leading zeros (FUN_14018c560): 0x16 in the first row, 0x22 in the
/// second.
const BLANK: usize = 0x16;
const BLANK_2: usize = 0x22;

struct Element {
    player: Player,
    parent: Option<usize>,
    offset: [f32; 2],
    modifiers: Modifiers,
    context: [f32; 2],
    base: [f32; 2],
}

impl Element {
    fn new(seq: &Sequence, group: usize, parent: Option<usize>) -> Self {
        Element {
            player: Player::new(seq, group),
            parent,
            offset: [0.; 2],
            modifiers: Modifiers::default(),
            context: [0.; 2],
            base: [0.; 2],
        }
    }

    /// FUN_14016b270: another group, played from its start.
    fn set_group(&mut self, seq: &Sequence, group: usize) {
        if self.player.group != group {
            self.player = Player::new(seq, group);
        }
    }
}

/// FUN_140192bb0 / FUN_140192c30 after one step: where the group's
/// animation puts its children.
fn anchor(seq: &Sequence, group: usize) -> [f32; 2] {
    let mut p = Player::new(seq, group);
    p.update(seq, 1.);
    seq.child_context(&p, &Modifiers::default())
        .unwrap_or([0.; 2])
}

fn group_value(
    seq: &Sequence,
    group: usize,
    f: fn(&data::sprite_sequence::AnimationGroup) -> i32,
) -> i32 {
    seq.groups.get(group).map_or(0, f)
}

/// The stacking position shared by the widgets (FUN_140182c60).
struct Stack {
    kind: usize,
    side: usize,
    current: f32,
    previous: f32,
    y: i32,
}

impl Stack {
    /// FUN_140182b10: the widget's height (its frame group's UiPadding) in
    /// its slot, and its place below the lower slots.
    fn new(slots: &mut HudSlots, kind: usize, side: Side, height: i32) -> Self {
        let side = side as usize;
        slots.register(kind, side, height as i16);
        let y = slots.stack(kind, side);
        Stack {
            kind,
            side,
            current: y as f32,
            previous: y as f32,
            y,
        }
    }

    fn follow(&mut self, slots: &HudSlots) {
        self.previous = self.current;
        let target = slots.stack(self.kind, self.side);
        if target != self.y {
            let next = spring_with(
                [self.current, 0., 0.],
                [target as f32, 0., 0.],
                [self.previous, 0., 0.],
                0.4,
                [1.2, 0.5, 0.1],
            );
            self.current = next[0];
            self.y = next[0] as i32;
        }
    }
}

fn draw_elements(
    elements: &mut [Element],
    seq: &Sequence,
    root: [f32; 2],
    sheet: usize,
) -> Vec<Draw> {
    let mut out = Vec::new();
    for i in 0..elements.len() {
        let base = match elements[i].parent {
            Some(p) => {
                let p = &elements[p];
                [
                    p.context[0] + p.offset[0] + p.base[0],
                    p.context[1] + p.offset[1] + p.base[1],
                ]
            }
            None => root,
        };
        let e = &mut elements[i];
        e.base = base;
        e.context = seq
            .child_context(&e.player, &e.modifiers)
            .unwrap_or([0.; 2]);
        let origin = [e.offset[0] + base[0], e.offset[1] + base[1]];
        out.push(Draw::Quads {
            quads: seq.quads(&e.player, origin, &e.modifiers),
            z: 0,
            sheet,
        });
    }
    out
}

/// dk::MISSION_GAUGE: a bar the limit's time runs down.
pub struct LimitGauge {
    elements: Vec<Element>,
    x: f32,
    stack: Stack,
    max: f32,
    value: f32,
    /// +0xcec: the value the bar was last sized for (-1 at creation).
    shown: f32,
}

impl LimitGauge {
    /// FUN_14018c990 with `max` (+0xcd8) and the starting value; `None`
    /// when the sheet lacks one of the gauge's groups.
    pub fn new(
        seq: &Sequence,
        side: Side,
        kind: usize,
        max: f32,
        value: f32,
        slots: &mut HudSlots,
    ) -> Option<Self> {
        if !GAUGE_GROUPS
            .iter()
            .chain(&GAUGE_ANCHOR)
            .all(|&g| g < seq.groups.len())
        {
            return None;
        }
        let shift = match side {
            Side::Left => -SIDE_SHIFT,
            Side::Right => SIDE_SHIFT,
            Side::Centre => 0.,
        };
        let x = anchor(seq, GAUGE_ANCHOR[side as usize])[0] + shift;
        let mut elements: Vec<Element> = GAUGE_GROUPS
            .iter()
            .enumerate()
            .map(|(k, &g)| Element::new(seq, g, (k > 0).then_some(0)))
            .collect();
        // Every sprite hangs from the frame and sits at its child position
        // (FUN_140192bb0 / FUN_140192c30 are the context FUN_140192be0
        // gives, which an element's base already holds); the rest bar is
        // moved by the frame group's (LightPositionX, TextPositionY).
        elements[2].offset = [
            group_value(seq, GAUGE_GROUPS[0], |g| g.light_position_x) as f32,
            group_value(seq, GAUGE_GROUPS[0], |g| g.text_position_y) as f32,
        ];
        let height = group_value(seq, GAUGE_GROUPS[0], |g| g.ui_padding);
        let stack = Stack::new(slots, kind, side, height);
        let mut g = LimitGauge {
            elements,
            x,
            stack,
            max,
            value,
            shown: -1.,
        };
        g.refresh(seq);
        Some(g)
    }

    /// The value the bar follows (+0xce0's target, the limit's time).
    pub fn set(&mut self, value: f32) {
        self.value = value;
    }

    /// FUN_14018d040: the fill and rest bars scale to the ratio and the
    /// marker moves along the bar's width (the marker group's TextPositionX).
    fn refresh(&mut self, seq: &Sequence) {
        let r = if self.max > 0. {
            self.value / self.max
        } else {
            0.
        };
        let width = group_value(seq, GAUGE_GROUPS[3], |g| g.text_position_x) as f32;
        self.elements[1].modifiers.scale_x = r;
        self.elements[2].modifiers.scale_x = 1. - r;
        self.elements[3].offset = [(r * width).trunc(), 0.];
        self.shown = self.value;
    }

    /// FUN_14018cf00.
    pub fn update(&mut self, seq: &Sequence, slots: &HudSlots) {
        self.stack.follow(slots);
        if self.shown != self.value {
            self.refresh(seq);
        }
        for e in &mut self.elements {
            e.player.update(seq, 1.);
        }
    }

    pub fn draw(&mut self, seq: &Sequence, sheet: usize) -> Vec<Draw> {
        let root = [self.x, self.stack.y as f32];
        draw_elements(&mut self.elements, seq, root, sheet)
    }

    /// FUN_140182b80 when the widget goes.
    pub fn release(&self, slots: &mut HudSlots) {
        slots.release(self.stack.kind, self.stack.side);
    }
}

/// dk::COUNTER in YS::COMBOCOUNTER's two-row mode: the hits and the best.
pub struct ComboCounter {
    elements: Vec<Element>,
    x: f32,
    stack: Stack,
    digits: usize,
    value: i32,
    best: i32,
    /// +0x1ca4 / +0x1cb4: the values last drawn (-1 at creation).
    shown: i32,
    shown_best: i32,
}

/// FUN_1403b6f90: decimal digits of the counter's range.
pub fn digits_of(max: i32) -> usize {
    let (mut n, mut p) = (0, 1i64);
    if max > 0 {
        loop {
            n += 1;
            p *= 10;
            if p > i64::from(max) {
                break;
            }
        }
    }
    n
}

impl ComboCounter {
    /// FUN_14018be20 in mode 1 with `digits` (+0x1cb8); `None` when the
    /// sheet lacks one of the counter's groups.
    pub fn new(
        seq: &Sequence,
        side: Side,
        kind: usize,
        digits: usize,
        slots: &mut HudSlots,
    ) -> Option<Self> {
        let needed = [
            COUNTER_FRAME,
            COUNTER_LABEL,
            DIGIT + 9,
            BLANK,
            SEPARATOR,
            DIGIT_2 + 9,
            BLANK_2,
        ];
        if !needed
            .iter()
            .chain(&COUNTER_ANCHOR)
            .all(|&g| g < seq.groups.len())
        {
            return None;
        }
        let shift = match side {
            Side::Left => -SIDE_SHIFT,
            Side::Right => SIDE_SHIFT,
            Side::Centre => 0.,
        };
        let x = anchor(seq, COUNTER_ANCHOR[side as usize])[0] + shift;
        // The label hangs from the frame at its child position (as the
        // gauge's sprites do).
        let mut elements = vec![
            Element::new(seq, COUNTER_FRAME, None),
            Element::new(seq, COUNTER_LABEL, Some(0)),
        ];
        let width = group_value(seq, DIGIT, |g| g.text_position_x);
        // The first row: the ones digit rightmost.
        let mut span = 0;
        for i in 0..digits {
            let mut e = Element::new(seq, DIGIT, Some(0));
            e.offset = [(width * (digits - 1 - i) as i32) as f32, 0.];
            span += width;
            elements.push(e);
        }
        // The second row after the separator; the separator sprite comes
        // last (its slot follows the digit slots).
        let sep_width = group_value(seq, SEPARATOR, |g| g.text_position_x);
        let width_2 = group_value(seq, DIGIT_2, |g| g.text_position_x);
        for i in 0..digits {
            let mut e = Element::new(seq, DIGIT_2, Some(0));
            e.offset = [
                (width_2 * (digits - 1 - i) as i32 + span + sep_width) as f32,
                0.,
            ];
            elements.push(e);
        }
        let mut sep = Element::new(seq, SEPARATOR, Some(0));
        sep.offset = [span as f32, 0.];
        elements.push(sep);
        let height = group_value(seq, COUNTER_FRAME, |g| g.ui_padding);
        let stack = Stack::new(slots, kind, side, height);
        Some(ComboCounter {
            elements,
            x,
            stack,
            digits,
            value: 0,
            best: 0,
            shown: -1,
            shown_best: -1,
        })
    }

    /// The hits (+0x38) and the best (+0x50, FUN_1403fa8b0 only raises it).
    pub fn set(&mut self, value: i32) {
        self.value = value;
        self.best = self.best.max(value);
    }

    /// FUN_14018c560's digits: "%09d", the last `digits` characters, each
    /// '0'..'9' a group from 0xc (first row) or 0x18 (second); leading zeros
    /// but the last become blanks.
    fn write_row(&mut self, seq: &Sequence, first: usize, value: i32, digit: usize, blank: usize) {
        let text = format!("{:09}", value.max(0));
        let chars: Vec<u8> = text.bytes().rev().take(self.digits).collect();
        for (k, c) in chars.iter().enumerate() {
            let g = digit + usize::from(c.wrapping_sub(b'0')).min(9);
            self.elements[first + k].set_group(seq, g);
        }
        for k in (1..chars.len()).rev() {
            if chars[k] != b'0' {
                break;
            }
            self.elements[first + k].set_group(seq, blank);
        }
    }

    /// FUN_14018c560.
    pub fn update(&mut self, seq: &Sequence, slots: &HudSlots) {
        self.stack.follow(slots);
        if self.shown != self.value {
            self.write_row(seq, 2, self.value, DIGIT, BLANK);
            self.shown = self.value;
        }
        if self.shown_best != self.best {
            let first = 2 + self.digits;
            self.write_row(seq, first, self.best, DIGIT_2, BLANK_2);
            self.shown_best = self.best;
        }
        for e in &mut self.elements {
            e.player.update(seq, 1.);
        }
    }

    pub fn draw(&mut self, seq: &Sequence, sheet: usize) -> Vec<Draw> {
        let root = [self.x, self.stack.y as f32];
        draw_elements(&mut self.elements, seq, root, sheet)
    }

    pub fn release(&self, slots: &mut HudSlots) {
        slots.release(self.stack.kind, self.stack.side);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_gives_the_digit_count() {
        assert_eq!(digits_of(999), 3);
        assert_eq!(digits_of(1000), 4);
        assert_eq!(digits_of(9), 1);
        assert_eq!(digits_of(0), 0);
    }
}
