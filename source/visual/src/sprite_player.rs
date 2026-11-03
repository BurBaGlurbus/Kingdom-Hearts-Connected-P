//! dk::Sprite / dk::SpriteMessage as the menus use them: a SEQD player
//! with a queued group (+0x1d8), a parent chain (FUN_140300bd0: base =
//! parent context + offset + base; flags 0x200|0x400 inherit the parent's
//! context alpha), and an optional text drawn by FUN_14016ae30 in font 1
//! (sys). Sprites update and draw in ascending priority, then creation
//! order.
use crate::dialogue;
use crate::field_command_menu::Font;
use data::sprite_sequence::{Modifiers, Player, Quad, Sequence, color};

/// System font reference height and non-JP line spacing (command-spec
/// §5.3).
const FONT_REFERENCE: f32 = 18.;
const LINE_SPACING: f32 = 5.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub bytes: Vec<u8>,
    /// +0x208, PS2 0xAABBGGRR.
    pub colour: u32,
    /// +0x20c (the creation group's TextScale, else 15).
    pub size: f32,
    /// +0x214: 0 left, 1 right, 2 centred.
    pub align: u8,
    /// +0x200 / +0x204.
    pub offset: [f32; 2],
    /// Flag 0x10: add the group's (LightPositionX, TextPositionY).
    pub group_position: bool,
    /// Flag 4: colour from the animation; flag 8: keep +0x208's RGB.
    pub anim_colour: bool,
    pub keep_rgb: bool,
    /// Line spacing override (else the group's UiPadding).
    pub extra: Option<f32>,
}

impl Text {
    /// FUN_14016aa50's defaults: flags 4|8|0x10, colour 0x80e5e5e5, left.
    pub fn new(bytes: Vec<u8>, size: f32) -> Self {
        Self {
            bytes,
            colour: 0x80e5e5e5,
            size,
            align: 0,
            offset: [0.; 2],
            group_position: true,
            anim_colour: true,
            keep_rgb: true,
            extra: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Sprite {
    pub prio: u32,
    /// Index of the art sheet the caller binds.
    pub sheet: usize,
    pub player: Player,
    pub queued: Option<usize>,
    pub base: [f32; 2],
    pub offset: [f32; 2],
    pub parent: Option<Id>,
    /// Flags 0x200|0x400: multiplier (1, 1, 1, parent context alpha).
    pub inherit_alpha: bool,
    /// Tz::MenuItem rows die with their parent (FUN_1402f76b0).
    pub die_with_parent: bool,
    pub multiplier: [f32; 4],
    pub text: Option<Text>,
    /// A draw callback's text (+0x220, e.g. the stock list's "×N" count,
    /// FUN_14034e330), laid out like `text` after it.
    pub after: Option<Text>,
    /// Flag 2 (FUN_140166bd0 by tag): updated but not drawn.
    pub hidden: bool,
    pub dead: bool,
    /// Set by a close: the sprite dies when its play-once group ends.
    pub die_when_done: bool,
    /// Player scale multipliers (+0x178 / +0x17c; gauge fills).
    pub scale: [f32; 2],
    /// The player's child context and colour as last found (FUN_140192be0
    /// keeps them while no IsActive animation runs).
    pub ctx: [f32; 2],
    pub ctx_color: [f32; 4],
}

impl Sprite {
    fn refresh(&mut self, seq: &Sequence) {
        let m = Modifiers::default();
        if let Some(c) = seq.child_context(&self.player, &m) {
            self.ctx = c;
        }
        if let Some(c) = seq
            .context_color(&self.player, &m)
            .or_else(|| seq.current_color(&self.player))
        {
            self.ctx_color = c;
        }
    }
}

#[derive(Default)]
pub struct Sprites {
    pub list: Vec<Sprite>,
}

/// What one draw item samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    Art(usize),
    Font,
    /// The font image's `icon` plane (inline 0x09 icons).
    Icon,
}

/// Splits encoded text into lines at 0x02 and drops other control codes
/// with their parameters (0x01 stays: it is a narrow space).
pub fn lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![Vec::new()];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x02 {
            out.push(Vec::new());
            i += 1;
        } else if b == 0x01 || b >= 0x20 {
            out.last_mut().unwrap().push(b);
            i += 1;
        } else {
            i += dialogue::code_length(b).max(1);
        }
    }
    out
}

impl Sprites {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &mut self,
        seq: &Sequence,
        prio: u32,
        sheet: usize,
        group: usize,
        queued: Option<usize>,
        text: Option<Text>,
    ) -> Id {
        self.list.push(Sprite {
            prio,
            sheet,
            player: Player::new(seq, group),
            queued,
            base: [0.; 2],
            offset: [0.; 2],
            parent: None,
            inherit_alpha: false,
            die_with_parent: false,
            multiplier: [1.; 4],
            text,
            after: None,
            hidden: false,
            dead: false,
            die_when_done: false,
            scale: [1.; 2],
            ctx: [0.; 2],
            ctx_color: [1.; 4],
        });
        let id = Id(self.list.len() - 1);
        self.list[id.0].refresh(seq);
        id
    }

    pub fn get(&self, id: Id) -> &Sprite {
        &self.list[id.0]
    }

    pub fn get_mut(&mut self, id: Id) -> &mut Sprite {
        &mut self.list[id.0]
    }

    /// FUN_140300bd0.
    pub fn attach(&mut self, child: Id, parent: Id) {
        let s = self.get_mut(child);
        s.parent = Some(parent);
        s.inherit_alpha = true;
    }

    /// Restart on `group` with `queued` next.
    pub fn set(&mut self, seq: &Sequence, id: Id, group: usize, queued: Option<usize>) {
        let s = self.get_mut(id);
        s.player = Player::new(seq, group);
        s.queued = queued;
        s.die_when_done = false;
        s.refresh(seq);
    }

    /// Plays a close group, then dies (FUN_140300c50(·, group) with no
    /// follow-up).
    pub fn close(&mut self, seq: &Sequence, id: Id, group: usize) {
        self.set(seq, id, group, None);
        self.get_mut(id).die_when_done = true;
    }

    pub fn kill(&mut self, id: Id) {
        self.get_mut(id).dead = true;
    }

    pub fn alive(&self, id: Id) -> bool {
        !self.get(id).dead
    }

    /// Settled in a play-once group with nothing queued.
    pub fn done(&self, id: Id) -> bool {
        let s = self.get(id);
        s.dead || (s.player.finished() && s.queued.is_none())
    }

    fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.list.len()).collect();
        order.sort_by_key(|&i| self.list[i].prio);
        order
    }

    /// One frame for every live sprite.
    pub fn update(&mut self, seq: &Sequence) {
        for i in self.order() {
            let parent_dead = self.list[i].parent.is_some_and(|p| self.list[p.0].dead);
            let s = &mut self.list[i];
            if s.dead {
                continue;
            }
            if s.die_with_parent && parent_dead {
                s.dead = true;
                continue;
            }
            s.player.update(seq, 1.);
            if seq.groups[s.player.group].play_count != 0 && s.player.finished() {
                if let Some(next) = s.queued.take() {
                    s.player = Player::new(seq, next);
                    s.player.update(seq, 1.);
                } else if s.die_when_done {
                    s.dead = true;
                }
            }
            s.refresh(seq);
        }
    }

    fn modifiers(&self, i: usize) -> Modifiers {
        let s = &self.list[i];
        let mut m = Modifiers {
            color: s.multiplier,
            scale_x: s.scale[0],
            scale_y: s.scale[1],
            ..Modifiers::default()
        };
        if s.inherit_alpha
            && let Some(p) = s.parent
        {
            let pm = self.modifiers(p.0);
            let a = self.list[p.0].ctx_color[3] * pm.color[3];
            m.color = [s.multiplier[0], s.multiplier[1], s.multiplier[2], a];
        }
        m
    }

    /// The alpha a child attached to `id` (flags 0x200|0x400) draws with.
    pub fn child_alpha(&self, id: Id) -> f32 {
        self.list[id.0].ctx_color[3] * self.modifiers(id.0).color[3]
    }

    /// Resolved base (FUN_14016abf0): the parent's context + offset + base.
    pub fn resolved_base(&self, id: Id) -> [f32; 2] {
        let s = self.get(id);
        match s.parent {
            Some(p) => {
                let pb = self.resolved_base(p);
                let ps = self.get(p);
                [
                    pb[0] + ps.offset[0] + ps.ctx[0],
                    pb[1] + ps.offset[1] + ps.ctx[1],
                ]
            }
            None => s.base,
        }
    }

    /// Where this sprite's children sit (its context in screen space).
    pub fn context(&self, id: Id) -> [f32; 2] {
        let b = self.resolved_base(id);
        let s = self.get(id);
        [b[0] + s.offset[0] + s.ctx[0], b[1] + s.offset[1] + s.ctx[1]]
    }

    /// The draw list: art, then text, per sprite in priority order.
    pub fn draw(&self, seq: &Sequence, font: &Font) -> Vec<(Sheet, Vec<Quad>)> {
        self.draw_where(seq, font, |_| true)
    }

    /// `draw` for the sprites whose priority passes `keep`.
    pub fn draw_where(
        &self,
        seq: &Sequence,
        font: &Font,
        keep: impl Fn(u32) -> bool,
    ) -> Vec<(Sheet, Vec<Quad>)> {
        let mut out = Vec::new();
        for i in self.order() {
            let s = &self.list[i];
            if !keep(s.prio) || s.dead || s.hidden || s.parent.is_some_and(|p| self.list[p.0].dead)
            {
                continue;
            }
            let id = Id(i);
            let m = self.modifiers(i);
            let b = self.resolved_base(id);
            let origin = [b[0] + s.offset[0], b[1] + s.offset[1]];
            out.push((Sheet::Art(s.sheet), seq.quads(&s.player, origin, &m)));
            for t in s.text.iter().chain(s.after.iter()) {
                let q = Self::text_draw(seq, s, &m, b, t, font);
                out.push((Sheet::Font, q.glyphs));
                if !q.icons.is_empty() {
                    out.push((Sheet::Icon, q.icons));
                }
            }
        }
        out
    }

    /// FUN_14016ae30 for one text of sprite `s` (resolved base `b`).
    fn text_draw(
        seq: &Sequence,
        s: &Sprite,
        m: &Modifiers,
        b: [f32; 2],
        t: &Text,
        font: &Font,
    ) -> TextQuads {
        let mut c = [s.ctx[0] + s.offset[0], s.ctx[1] + s.offset[1]];
        let g = &seq.groups[s.player.group];
        if t.group_position {
            c[0] += g.light_position_x as f32;
            c[1] += g.text_position_y as f32;
        }
        let mut rgba = color(t.colour);
        if t.anim_colour {
            let anim: [f32; 4] = std::array::from_fn(|k| s.ctx_color[k] * m.color[k]);
            rgba = if t.keep_rgb {
                [rgba[0], rgba[1], rgba[2], anim[3]]
            } else {
                anim
            };
            rgba = std::array::from_fn(|k| (rgba[k] * m.color[k]).min(255. / 128.));
        }
        let x = (t.offset[0].trunc() + b[0] + c[0]).trunc();
        let y = (t.offset[1].trunc() + b[1] + c[1]).trunc();
        text_quads(
            &t.bytes,
            [x, y],
            t.size,
            t.align,
            t.extra.unwrap_or(g.ui_padding as f32),
            rgba,
            true,
            font,
        )
    }
}

/// One laid-out text: glyph quads (font sheet) and inline icons (icon
/// sheet).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextQuads {
    pub glyphs: Vec<Quad>,
    pub icons: Vec<Quad>,
}

/// Lines for the draw: split at 0x02, keeping 0x01, 0x03 (reset), 0x07
/// (colour) and 0x09 (icon) with their parameters; other codes go.
fn draw_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![Vec::new()];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let n = if b < 0x20 {
            dialogue::code_length(b).max(1)
        } else {
            1
        };
        match b {
            0x02 => out.push(Vec::new()),
            0x01 | 0x03 | 0x07 | 0x09 | 0x20.. => out
                .last_mut()
                .unwrap()
                .extend_from_slice(&bytes[i..(i + n).min(bytes.len())]),
            _ => {}
        }
        i += n;
    }
    out
}

/// FUN_1403e2130's width table (0x1405c8680): 18 for icons 0–9, 12–19 and
/// 26–29, 24 for the rest.
pub fn icon_width(icon: u8) -> u8 {
    match icon {
        0..=9 | 12..=19 | 26..=29 => 18,
        _ => 24,
    }
}

/// FUN_1403af9e0 for a pad (`kind` 1 Xbox, 2 PlayStation, 3 generic,
/// confirm not swapped): the icon drawn for `09 b`.
pub fn prompt_icon(b: u8, kind: u8) -> u8 {
    // Table 0x1405c4d40 from 0xdc: (Xbox, PlayStation, generic).
    const T: [[u8; 3]; 30] = [
        [37, 70, 92],
        [35, 68, 90],
        [36, 69, 91],
        [34, 67, 89],
        [32, 73, 95],
        [30, 71, 93],
        [33, 74, 96],
        [31, 72, 94],
        [14, 76, 14],
        [15, 75, 15],
        [20, 79, 97],
        [21, 80, 98],
        [25, 78, 25],
        [16, 77, 16],
        [60, 82, 60],
        [22, 81, 22],
        [61, 83, 61],
        [62, 84, 62],
        [63, 85, 63],
        [64, 86, 64],
        [65, 87, 65],
        [66, 88, 66],
        [99, 107, 99],
        [100, 108, 100],
        [101, 109, 101],
        [102, 110, 102],
        [103, 202, 103],
        [104, 203, 104],
        [105, 204, 105],
        [106, 205, 106],
    ];
    let k = match kind {
        2 => 1,
        3 => 2,
        _ => 0,
    };
    match b {
        0xda => [35, 68, 90][k],
        0xdb => [37, 70, 92][k],
        0xdc..=0xf9 => T[(b - 0xdc) as usize][k],
        _ => b,
    }
}

/// FUN_1403af130 + FUN_1404e4a30 + FUN_1404e89d0 with the exe's default
/// key bindings (.data 0x14078278c..0x14078281c: Left mouse confirm, Space
/// cancel, right mouse, F, Left Shift / R, C / E, X / middle mouse, M / N,
/// arrows, WASD): the icon-plane cells drawn for `09 xx` while the keyboard
/// or mouse was the last device. None: xx is outside 0xda..=0xf9 (the raw
/// cell is drawn).
pub fn keyboard_prompt(xx: u8) -> Option<&'static [u8]> {
    Some(match xx {
        0xda | 0xdd => &[190],
        0xdb | 0xdc => &[145],
        0xde => &[191],
        0xdf => &[116],
        0xe0 => &[137],
        0xe1 => &[128],
        0xe2 => &[113],
        0xe3 => &[115],
        0xe4 => &[134],
        0xe5 => &[192],
        0xe6 => &[124],
        0xe7 => &[123],
        0xe8 => &[133, 111, 129, 114],
        0xe9 => &[201],
        // No keyboard branch: the table value resolves to cell 0.
        0xea => &[0],
        0xeb => &[174, 176, 175, 177],
        0xec => &[174, 175],
        0xed => &[176, 177],
        0xee => &[174],
        0xef => &[175],
        0xf0 => &[176],
        0xf1 => &[177],
        0xf2 => &[133],
        0xf3 => &[129],
        0xf4 => &[111],
        0xf5 => &[114],
        0xf6 => &[195],
        0xf7 => &[196],
        0xf8 => &[197],
        0xf9 => &[198],
        _ => return None,
    })
}

/// System font text as FUN_1403aed80 / FUN_1403afd90 / FUN_1403b0280 /
/// FUN_1404070f0 lay it out (fonts 0..2 share the sys glyphs, cell 18x24,
/// reference 18, spacing 5): `origin` is the top of the first line; each
/// glyph's cell bottom sits at line top + lineH + 2. Alignment 1 / 2 moves
/// left by the widest line's width / half of it. Font 1 (`shadow`) draws a
/// black glyph at (+2, +1) with half alpha under each glyph. 0x07 replaces
/// the RGB (alpha scaled) until 0x03; 0x09 draws an icon (FUN_140406ed0,
/// text-codes-01 §1): a square of trunc(28k) − 4 px, 2 px right of the pen,
/// its bottom at line baseline + 4k − 1, untinted at the text alpha, no
/// shadow; it measures its raw width and advances width·k.
#[allow(clippy::too_many_arguments)]
pub fn text_quads(
    bytes: &[u8],
    origin: [f32; 2],
    size: f32,
    align: u8,
    extra: f32,
    rgba: [f32; 4],
    shadow: bool,
    font: &Font,
) -> TextQuads {
    let k = size / FONT_REFERENCE;
    let line_h = ((FONT_REFERENCE * k + 0.5) as i32).max((FONT_REFERENCE * k) as i32) as f32;
    let (qw, qh) = (
        ((18. * 16. * k) as i32) as f32 / 16.,
        ((24. * 16. * k) as i32) as f32 / 16.,
    );
    let narrow = (k * 6. + 0.5) as i32 as f32;
    let advance = |w: u8| (w as f64 * k as f64 + 0.5) as i32 as f32;
    let icon_advance = |w: u8| ((w as f32 * 16.) * k) as i32 as f32 / 16.;
    let text: Vec<u8> = bytes.iter().copied().take_while(|&b| b != 0x10).collect();
    let lines = draw_lines(&text);
    let width = |line: &[u8]| -> f32 {
        let mut w = 0.;
        let mut i = 0;
        while i < line.len() {
            match line[i] {
                0x01 | 0x20 => w += narrow,
                0x09 => w += line.get(i + 1).map_or(0., |&c| icon_width(c) as f32),
                b if b < 0x20 => {}
                _ => {
                    w += dialogue::glyph_index(line, i)
                        .and_then(|g| font.widths.get(g))
                        .map_or(0., |&w| advance(w))
                }
            }
            i += if line[i] < 0x20 {
                dialogue::code_length(line[i]).max(1)
            } else {
                1
            };
        }
        w
    };
    let w = lines.iter().map(|l| width(l)).fold(0., f32::max);
    let x0 = match align {
        1 => origin[0] - w,
        2 => origin[0] - (w as i32 >> 1) as f32,
        _ => origin[0],
    };
    let shadow_rgba = [0., 0., 0., ((rgba[3] * 128.) as i32 >> 1) as f32 / 128.];
    let mut out = TextQuads::default();
    let mut colour = rgba;
    for (j, line) in lines.iter().enumerate() {
        let line_top = origin[1] + j as f32 * (line_h + LINE_SPACING + extra);
        let top = line_top + line_h + 2. - qh;
        let baseline16 = (line_top + line_h) * 16.;
        let mut x = x0;
        let mut i = 0;
        while i < line.len() {
            let b = line[i];
            let n = if b < 0x20 {
                dialogue::code_length(b).max(1)
            } else {
                1
            };
            match b {
                0x01 | 0x20 => {
                    x += narrow;
                    i += n;
                    continue;
                }
                0x03 => {
                    colour = rgba;
                    i += n;
                    continue;
                }
                0x07 => {
                    if let Some(p) = line.get(i + 1..i + 5) {
                        let a = (((rgba[3] * 128.) as i32 * p[3] as i32) >> 7).min(255);
                        colour = [
                            p[0] as f32 / 128.,
                            p[1] as f32 / 128.,
                            p[2] as f32 / 128.,
                            a as f32 / 128.,
                        ];
                    }
                    i += n;
                    continue;
                }
                0x09 => {
                    let raw = line.get(i + 1).copied().unwrap_or(0);
                    // Keyboard prompts (pad 0): each bound key's cap, one
                    // after the other (FUN_1403af9e0).
                    let cells: Vec<u8> = match keyboard_prompt(raw) {
                        Some(cells) if font.pad == 0 => cells.to_vec(),
                        _ => vec![prompt_icon(raw, font.pad.max(1))],
                    };
                    for icon in cells {
                        let side = ((28. * k) as i32 - 4) as f32;
                        let t = (k * 64. + baseline16) as i32;
                        let bottom = (t - 16) as f32 / 16.;
                        let left = x + 2.;
                        let (u, v) = ((icon as i32 % 21) * 24, (icon as i32 / 21) * 24);
                        out.icons.push(Quad {
                            corners: [
                                [left, bottom - side],
                                [left + side, bottom - side],
                                [left, bottom],
                                [left + side, bottom],
                            ],
                            uv: [u, v, u + 24, v + 24],
                            colors: [[1., 1., 1., colour[3]]; 4],
                            blend: 0,
                        });
                        x += icon_advance(icon_width(icon));
                    }
                    i += n;
                    continue;
                }
                _ if b < 0x20 => {
                    i += n;
                    continue;
                }
                _ => {}
            }
            let Some(g) = dialogue::glyph_index(line, i) else {
                i += 1;
                continue;
            };
            i += 1;
            let Some(&gw) = font.widths.get(g) else {
                continue;
            };
            let (u, v) = (((g % 280) % 28) as i32 * 18, ((g % 280) / 28) as i32 * 24);
            let quad = |dx: f32, dy: f32, colour: [f32; 4]| Quad {
                corners: [
                    [x + dx, top + dy],
                    [x + dx + qw, top + dy],
                    [x + dx, top + dy + qh],
                    [x + dx + qw, top + dy + qh],
                ],
                uv: [u, v, u + 18, v + 24],
                colors: [colour; 4],
                blend: 0,
            };
            if shadow {
                out.glyphs.push(quad(2., 1., shadow_rgba));
            }
            out.glyphs.push(quad(0., 0., colour));
            x += advance(gw);
        }
    }
    out
}
/// Tz::LightCur's motion (FUN_1402faf40 / FUN_1402fb3f0 / FUN_14034b890):
/// the first attach snaps; a re-target glides from the last drawn position
/// over 30 ticks with f(s) = 2.74981(s^3 - 2s^2 + s) + (3s^2 - 2s^3).
#[derive(Clone, Debug, Default)]
pub struct LightCur {
    pub id: Option<Id>,
    from: Option<[f32; 2]>,
    tick: f32,
    drawn: Option<[f32; 2]>,
}

pub fn glide(s: f32) -> f32 {
    2.74981 * (s * s * s - 2. * s * s + s) + (3. * s * s - 2. * s * s * s)
}

impl LightCur {
    pub fn new(id: Id) -> Self {
        Self {
            id: Some(id),
            ..Self::default()
        }
    }

    pub fn retarget(&mut self) {
        self.from = self.drawn;
        self.tick = 0.;
    }

    /// Places the orb for this frame toward `target`: the glide clock
    /// advances (clamped at 30) before the draw, and each axis moves by
    /// (int)(f(t/30) x distance).
    pub fn place(&mut self, sprites: &mut Sprites, target: [f32; 2]) {
        let Some(id) = self.id else { return };
        self.tick = (self.tick + 1.).min(30.);
        let at = match self.from {
            Some(from) => {
                let f = glide(self.tick / 30.);
                [
                    from[0] + ((target[0] - from[0]) * f).trunc(),
                    from[1] + ((target[1] - from[1]) * f).trunc(),
                ]
            }
            None => target,
        };
        self.drawn = Some(at);
        sprites.get_mut(id).base = at;
    }

    /// Instant removal (vtable +0x20).
    pub fn kill(&mut self, sprites: &mut Sprites) {
        if let Some(id) = self.id.take() {
            sprites.kill(id);
        }
        self.from = None;
        self.drawn = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_split_and_drop_codes() {
        // "AB" newline "C" then a colour code with its parameters.
        let l = lines(&[0x2e, 0x2f, 0x02, 0x30, 0x07, 0xf0, 0xf0, 0xf0, 0xf0]);
        assert_eq!(l, vec![vec![0x2e, 0x2f], vec![0x30]]);
    }

    #[test]
    fn icons_and_colour_codes_follow_the_text_draw() {
        let widths = [12u8; 200];
        let font = Font {
            widths: &widths,
            pad: 2,
        };
        // Icon 0, then red 'A', a reset, 'A'; size 18 (k = 1).
        let bytes = [0x09, 0x00, 0x07, 0xff, 0x00, 0x00, 0x80, 0x21, 0x03, 0x21];
        let q = text_quads(&bytes, [100., 50.], 18., 0, 0., [1.; 4], false, &font);
        assert_eq!(q.icons.len(), 1);
        let icon = &q.icons[0];
        // 24 px square, 2 px right of the pen, bottom at baseline (top +
        // 18) + 3.
        assert_eq!(icon.corners[0], [102., 47.]);
        assert_eq!(icon.corners[3], [126., 71.]);
        assert_eq!(icon.colors[0], [1., 1., 1., 1.]);
        assert_eq!(q.glyphs.len(), 2);
        // The glyph follows the icon's 18 px advance; 0x07 sets its RGB.
        assert_eq!(q.glyphs[0].corners[0][0], 118.);
        assert_eq!(q.glyphs[0].colors[0][1], 0.);
        assert_eq!(q.glyphs[1].colors[0], [1.; 4]);
        // Keyboard: LT / RT are C and E, the stick four keys.
        assert_eq!(keyboard_prompt(0xe2), Some(&[113u8][..]));
        assert_eq!(keyboard_prompt(0xe8), Some(&[133u8, 111, 129, 114][..]));
        assert_eq!(keyboard_prompt(0x20), None);
        // PlayStation prompts: L2 / R2.
        assert_eq!(prompt_icon(0xe2, 2), 74);
        assert_eq!(prompt_icon(0xe3, 1), 31);
        assert_eq!(prompt_icon(0x04, 1), 4);
    }
}
