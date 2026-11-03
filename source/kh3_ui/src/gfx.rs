//! Reads a Scaleform GFx movie (SWF 9+ with AS3, as KH3 cooks them into `SwfMovie` packages) into
//! the plain description `def::Pack::movie_linked` builds a movie from: stage, images, shapes as
//! edges per fill, sprites with their timelines and frame scripts, text fields, symbol classes.
//! Ported from StrawHatPro's `tools/lib/gfx.js`, `abc.js`, `gfx_font.js` and the movie half of
//! `tools/sora_ui_pack.js` (MIT; see NOTICE). Nothing here is stored: it is made from the
//! player's own install each run.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value, json};

use crate::def::{Font, Glyph, Seg};

#[derive(Debug)]
pub struct GfxError(pub String);

impl std::fmt::Display for GfxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for GfxError {}

type Res<T> = Result<T, GfxError>;

fn bad<T>(why: &str) -> Res<T> {
    Err(GfxError(why.to_string()))
}

struct Bits<'a> {
    b: &'a [u8],
    p: usize,
    bit: u32,
}

#[derive(Clone, Copy, Default)]
struct M {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

#[derive(Clone, Copy, Default)]
struct R4 {
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
}

impl<'a> Bits<'a> {
    fn new(b: &'a [u8], p: usize) -> Self {
        Bits { b, p, bit: 0 }
    }
    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.p += 1;
        }
    }
    fn byte(&self, at: usize) -> u8 {
        self.b.get(at).copied().unwrap_or(0)
    }
    fn ub(&mut self, mut n: u32) -> u32 {
        let mut v: u64 = 0;
        while n > 0 {
            let left = 8 - self.bit;
            let take = left.min(n);
            let sh = left - take;
            v = (v << take) | ((u64::from(self.byte(self.p)) >> sh) & ((1 << take) - 1));
            self.bit += take;
            n -= take;
            if self.bit == 8 {
                self.bit = 0;
                self.p += 1;
            }
        }
        v as u32
    }
    fn sb(&mut self, n: u32) -> i32 {
        if n == 0 {
            return 0;
        }
        let v = i64::from(self.ub(n));
        (if v >= 1 << (n - 1) { v - (1 << n) } else { v }) as i32
    }
    fn fb(&mut self, n: u32) -> f64 {
        f64::from(self.sb(n)) / 65536.
    }
    fn u8(&mut self) -> u8 {
        self.align();
        let v = self.byte(self.p);
        self.p += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        self.align();
        let v = u16::from_le_bytes([self.byte(self.p), self.byte(self.p + 1)]);
        self.p += 2;
        v
    }
    fn s16(&mut self) -> i16 {
        self.u16() as i16
    }
    fn u32(&mut self) -> u32 {
        self.align();
        let v = u32::from_le_bytes([
            self.byte(self.p),
            self.byte(self.p + 1),
            self.byte(self.p + 2),
            self.byte(self.p + 3),
        ]);
        self.p += 4;
        v
    }
    fn s32(&mut self) -> i32 {
        self.u32() as i32
    }
    fn fixed8(&mut self) -> f64 {
        f64::from(self.s16()) / 256.
    }
    fn fixed(&mut self) -> f64 {
        f64::from(self.s32()) / 65536.
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }
    fn str(&mut self) -> String {
        self.align();
        let mut e = self.p;
        while self.byte(e) != 0 && e < self.b.len() {
            e += 1;
        }
        let s = String::from_utf8_lossy(self.b.get(self.p..e).unwrap_or_default()).into_owned();
        self.p = e + 1;
        s
    }
    fn latin(&mut self, n: usize) -> String {
        let s: String = self
            .b
            .get(self.p..self.p + n)
            .unwrap_or_default()
            .iter()
            .map(|&c| c as char)
            .collect();
        self.p += n;
        s
    }
    fn rect(&mut self) -> R4 {
        self.align();
        let n = self.ub(5);
        let r = R4 {
            x0: self.sb(n),
            x1: self.sb(n),
            y0: self.sb(n),
            y1: self.sb(n),
        };
        self.align();
        r
    }
    fn matrix(&mut self) -> M {
        self.align();
        let mut m = M {
            a: 1.,
            d: 1.,
            ..M::default()
        };
        if self.ub(1) != 0 {
            let n = self.ub(5);
            m.a = self.fb(n);
            m.d = self.fb(n);
        }
        if self.ub(1) != 0 {
            let n = self.ub(5);
            m.b = self.fb(n);
            m.c = self.fb(n);
        }
        let n = self.ub(5);
        m.tx = f64::from(self.sb(n));
        m.ty = f64::from(self.sb(n));
        self.align();
        m
    }
    fn cxform(&mut self, alpha: bool) -> [i32; 8] {
        self.align();
        let add = self.ub(1);
        let mul = self.ub(1);
        let n = self.ub(4);
        let mut c = [256, 256, 256, 256, 0, 0, 0, 0];
        if mul != 0 {
            c[0] = self.sb(n);
            c[1] = self.sb(n);
            c[2] = self.sb(n);
            if alpha {
                c[3] = self.sb(n);
            }
        }
        if add != 0 {
            c[4] = self.sb(n);
            c[5] = self.sb(n);
            c[6] = self.sb(n);
            if alpha {
                c[7] = self.sb(n);
            }
        }
        self.align();
        c
    }
    fn rgb(&mut self) -> [u8; 4] {
        [self.u8(), self.u8(), self.u8(), 255]
    }
    fn rgba(&mut self) -> [u8; 4] {
        [self.u8(), self.u8(), self.u8(), self.u8()]
    }
    fn eu32(&mut self) -> u32 {
        self.align();
        let (mut v, mut sh) = (0u32, 0);
        for _ in 0..5 {
            let b = self.u8();
            v |= u32::from(b & 127) << sh;
            sh += 7;
            if b & 128 == 0 {
                break;
            }
        }
        v
    }
}

struct Tag {
    code: u16,
    pos: usize,
    len: usize,
}

fn read_tags(b: &[u8], mut pos: usize, end: usize) -> Vec<Tag> {
    let mut tags = Vec::new();
    while pos + 2 <= end {
        let h = u16::from_le_bytes([b[pos], b[pos + 1]]);
        pos += 2;
        let code = h >> 6;
        let mut len = usize::from(h & 63);
        if len == 63 {
            let Some(w) = b.get(pos..pos + 4) else { break };
            len = u32::from_le_bytes([w[0], w[1], w[2], w[3]]) as usize;
            pos += 4;
        }
        tags.push(Tag { code, pos, len });
        pos += len;
        if code == 0 {
            break;
        }
    }
    tags
}

#[derive(Clone, Default)]
struct Style {
    ty: u8,
    color: [u8; 4],
    matrix: M,
    stops: Vec<(u8, [u8; 4])>,
    focal: Option<f64>,
    bitmap: Option<u16>,
}

#[derive(Clone, Default)]
struct LineStyle {
    width: f64,
    color: Option<[u8; 4]>,
}

fn fill_style(s: &mut Bits, ver: u32) -> Res<Style> {
    let t = s.u8();
    let mut f = Style {
        ty: t,
        ..Style::default()
    };
    match t {
        0 => f.color = if ver >= 3 { s.rgba() } else { s.rgb() },
        0x10 | 0x12 | 0x13 => {
            f.matrix = s.matrix();
            s.ub(4);
            let n = s.ub(4);
            for _ in 0..n {
                let ratio = s.u8();
                let c = if ver >= 3 { s.rgba() } else { s.rgb() };
                f.stops.push((ratio, c));
            }
            if t == 0x13 {
                f.focal = Some(s.fixed8());
            }
        }
        0x40..=0x43 => {
            f.bitmap = Some(s.u16());
            f.matrix = s.matrix();
        }
        _ => return bad("fill type"),
    }
    Ok(f)
}

fn line_style(s: &mut Bits, ver: u32) -> Res<LineStyle> {
    let width = f64::from(s.u16());
    let mut l = LineStyle { width, color: None };
    if ver == 4 {
        s.ub(2);
        let join = s.ub(2);
        let has_fill = s.ub(1);
        s.ub(3);
        s.ub(5);
        s.ub(1);
        s.ub(2);
        if join == 2 {
            s.fixed8();
        }
        if has_fill != 0 {
            fill_style(s, ver)?;
        } else {
            l.color = Some(s.rgba());
        }
    } else {
        l.color = Some(if ver >= 3 { s.rgba() } else { s.rgb() });
    }
    Ok(l)
}

fn styles(s: &mut Bits, ver: u32) -> Res<(Vec<Style>, Vec<LineStyle>)> {
    let mut n = usize::from(s.u8());
    if n == 255 && ver >= 2 {
        n = usize::from(s.u16());
    }
    let mut fills = Vec::with_capacity(n);
    for _ in 0..n {
        fills.push(fill_style(s, ver)?);
    }
    let mut n = usize::from(s.u8());
    if n == 255 {
        n = usize::from(s.u16());
    }
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        lines.push(line_style(s, ver)?);
    }
    Ok((fills, lines))
}

#[derive(Clone)]
enum Rec {
    Style {
        pos: Option<(f64, f64)>,
        f0: Option<usize>,
        f1: Option<usize>,
        ln: Option<usize>,
        new_styles: bool,
    },
    Line(f64, f64),
    Curve(f64, f64, f64, f64),
}

fn shape_records(
    s: &mut Bits,
    ver: u32,
    fills: &mut Vec<Style>,
    lines: &mut Vec<LineStyle>,
) -> Res<Vec<Rec>> {
    s.align();
    let (mut fb, mut lb) = (s.ub(4), s.ub(4));
    let mut recs = Vec::new();
    let (mut x, mut y) = (0i32, 0i32);
    let (mut fbase, mut lbase) = (0usize, 0usize);
    loop {
        if s.p > s.b.len() {
            return bad("shape past its data");
        }
        if s.ub(1) == 0 {
            let fl = s.ub(5);
            if fl == 0 {
                break;
            }
            let (mut pos, mut f0, mut f1, mut ln, mut new_styles) = (None, None, None, None, false);
            if fl & 1 != 0 {
                let n = s.ub(5);
                x = s.sb(n);
                y = s.sb(n);
                pos = Some((f64::from(x), f64::from(y)));
            }
            let idx = |v: u32, base: usize| if v != 0 { v as usize + base } else { 0 };
            if fl & 2 != 0 {
                f0 = Some(idx(s.ub(fb), fbase));
            }
            if fl & 4 != 0 {
                f1 = Some(idx(s.ub(fb), fbase));
            }
            if fl & 8 != 0 {
                ln = Some(idx(s.ub(lb), lbase));
            }
            if fl & 16 != 0 {
                s.align();
                fbase = fills.len();
                lbase = lines.len();
                let (nf, nl) = styles(s, ver)?;
                fills.extend(nf);
                lines.extend(nl);
                s.align();
                fb = s.ub(4);
                lb = s.ub(4);
                new_styles = true;
            }
            recs.push(Rec::Style {
                pos,
                f0,
                f1,
                ln,
                new_styles,
            });
        } else if s.ub(1) != 0 {
            let n = s.ub(4) + 2;
            let (mut dx, mut dy) = (0, 0);
            if s.ub(1) != 0 {
                dx = s.sb(n);
                dy = s.sb(n);
            } else if s.ub(1) != 0 {
                dy = s.sb(n);
            } else {
                dx = s.sb(n);
            }
            x += dx;
            y += dy;
            recs.push(Rec::Line(f64::from(x), f64::from(y)));
        } else {
            let n = s.ub(4) + 2;
            let cx = x + s.sb(n);
            let cy = y + s.sb(n);
            x = cx + s.sb(n);
            y = cy + s.sb(n);
            recs.push(Rec::Curve(
                f64::from(cx),
                f64::from(cy),
                f64::from(x),
                f64::from(y),
            ));
        }
    }
    s.align();
    Ok(recs)
}

struct ShapeData {
    bounds: [f64; 4],
    fills: Vec<Style>,
    lines: Vec<LineStyle>,
    recs: Vec<Rec>,
}

fn parse_shape(b: &[u8], t: &Tag, ver: u32) -> Res<(u32, ShapeData)> {
    let mut s = Bits::new(b, t.pos);
    let id = u32::from(s.u16());
    let r = s.rect();
    if ver == 4 {
        s.rect();
        s.u8();
    }
    let (mut fills, mut lines) = styles(&mut s, ver)?;
    let recs = shape_records(&mut s, ver, &mut fills, &mut lines)?;
    Ok((
        id,
        ShapeData {
            bounds: [
                f64::from(r.x0),
                f64::from(r.y0),
                f64::from(r.x1),
                f64::from(r.y1),
            ],
            fills,
            lines,
            recs,
        },
    ))
}

struct MorphFill {
    ty: u8,
    c: [[u8; 4]; 2],
    m: [M; 2],
    stops: Vec<(u8, [u8; 4], u8, [u8; 4])>,
    bitmap: Option<u16>,
}

struct MorphData {
    bounds: [R4; 2],
    fills: Vec<MorphFill>,
    lines: Vec<(u16, u16, [u8; 4], [u8; 4])>,
    recs: [Vec<Rec>; 2],
}

fn parse_morph(b: &[u8], t: &Tag, v2: bool) -> Res<(u32, MorphData)> {
    let mut s = Bits::new(b, t.pos);
    let id = u32::from(s.u16());
    let (b0, b1) = (s.rect(), s.rect());
    if v2 {
        s.rect();
        s.rect();
        s.u8();
    }
    let off = s.u32() as usize;
    let end_pos = s.p + off;
    let fill = |s: &mut Bits| -> Res<MorphFill> {
        let ty = s.u8();
        let mut f = MorphFill {
            ty,
            c: [[0; 4]; 2],
            m: [M::default(); 2],
            stops: Vec::new(),
            bitmap: None,
        };
        match ty {
            0 => f.c = [s.rgba(), s.rgba()],
            0x10 | 0x12 | 0x13 => {
                f.m = [s.matrix(), s.matrix()];
                let n = s.u8() & 15;
                for _ in 0..n {
                    let (r0, c0, r1, c1) = (s.u8(), s.rgba(), s.u8(), s.rgba());
                    f.stops.push((r0, c0, r1, c1));
                }
                if ty == 0x13 {
                    s.fixed8();
                    s.fixed8();
                }
            }
            0x40..=0x43 => {
                f.bitmap = Some(s.u16());
                f.m = [s.matrix(), s.matrix()];
            }
            _ => return bad("morph fill"),
        }
        Ok(f)
    };
    let mut n = usize::from(s.u8());
    if n == 255 {
        n = usize::from(s.u16());
    }
    let mut fills = Vec::with_capacity(n);
    for _ in 0..n {
        fills.push(fill(&mut s)?);
    }
    let mut n = usize::from(s.u8());
    if n == 255 {
        n = usize::from(s.u16());
    }
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        let (w0, w1) = (s.u16(), s.u16());
        let mut cs = ([0; 4], [0; 4]);
        if v2 {
            s.ub(2);
            let join = s.ub(2);
            let hf = s.ub(1);
            s.ub(3);
            s.ub(5);
            s.ub(1);
            s.ub(2);
            if join == 2 {
                s.fixed8();
            }
            if hf != 0 {
                let f = fill(&mut s)?;
                cs = (f.c[0], f.c[1]);
            } else {
                cs = (s.rgba(), s.rgba());
            }
        } else {
            cs = (s.rgba(), s.rgba());
        }
        lines.push((w0, w1, cs.0, cs.1));
    }
    let (mut xf, mut xl) = (Vec::new(), Vec::new());
    let recs0 = shape_records(&mut s, 1, &mut xf, &mut xl)?;
    let mut s2 = Bits::new(b, end_pos);
    let (mut xf, mut xl) = (Vec::new(), Vec::new());
    let recs1 = shape_records(&mut s2, 1, &mut xf, &mut xl)?;
    Ok((
        id,
        MorphData {
            bounds: [b0, b1],
            fills,
            lines,
            recs: [recs0, recs1],
        },
    ))
}

/// A morph at `t` as a plain shape (`morphAt`).
fn morph_at(m: &MorphData, t: f64) -> ShapeData {
    let lerp = |a: f64, b: f64| a + (b - a) * t;
    let lc = |a: [u8; 4], b: [u8; 4]| {
        [0, 1, 2, 3].map(|i| lerp(f64::from(a[i]), f64::from(b[i])).round() as u8)
    };
    let lm = |a: M, b: M| M {
        a: lerp(a.a, b.a),
        b: lerp(a.b, b.b),
        c: lerp(a.c, b.c),
        d: lerp(a.d, b.d),
        tx: lerp(a.tx, b.tx),
        ty: lerp(a.ty, b.ty),
    };
    let fills = m
        .fills
        .iter()
        .map(|f| Style {
            ty: f.ty,
            color: lc(f.c[0], f.c[1]),
            matrix: lm(f.m[0], f.m[1]),
            stops: f
                .stops
                .iter()
                .map(|s| {
                    (
                        lerp(f64::from(s.0), f64::from(s.2)).round() as u8,
                        lc(s.1, s.3),
                    )
                })
                .collect(),
            focal: None,
            bitmap: f.bitmap,
        })
        .collect();
    let lines = m
        .lines
        .iter()
        .map(|l| LineStyle {
            width: lerp(f64::from(l.0), f64::from(l.1)),
            color: Some(lc(l.2, l.3)),
        })
        .collect();
    // Walks the end records in step, with their pen.
    let mut ends: Vec<(Rec, f64, f64)> = Vec::new();
    let (mut px, mut py) = (0., 0.);
    for r in &m.recs[1] {
        match r {
            Rec::Style { pos, .. } => {
                if let Some(p) = pos {
                    (px, py) = *p;
                }
            }
            Rec::Line(x, y) => {
                ends.push((r.clone(), px, py));
                (px, py) = (*x, *y);
            }
            Rec::Curve(_, _, x, y) => {
                ends.push((r.clone(), px, py));
                (px, py) = (*x, *y);
            }
        }
    }
    let mut recs = Vec::with_capacity(m.recs[0].len());
    let mut k = 0usize;
    let (mut px, mut py) = (0., 0.);
    for r in &m.recs[0] {
        match r {
            Rec::Style {
                pos,
                f0,
                f1,
                ln,
                new_styles,
            } => {
                let npos = pos.map(|p| {
                    let e = ends.get(k).map_or(p, |e| (e.1, e.2));
                    (lerp(p.0, e.0), lerp(p.1, e.1))
                });
                if let Some(p) = pos {
                    (px, py) = *p;
                }
                recs.push(Rec::Style {
                    pos: npos,
                    f0: *f0,
                    f1: *f1,
                    ln: *ln,
                    new_styles: *new_styles,
                });
            }
            _ => {
                let Some(e) = ends.get(k) else {
                    recs.push(r.clone());
                    continue;
                };
                k += 1;
                let (ax, ay, x, y) = match r {
                    Rec::Line(x, y) => ((px + x) / 2., (py + y) / 2., *x, *y),
                    Rec::Curve(cx, cy, x, y) => (*cx, *cy, *x, *y),
                    Rec::Style { .. } => continue,
                };
                let (qx, qy, ex, ey) = match &e.0 {
                    Rec::Line(ex, ey) => ((e.1 + ex) / 2., (e.2 + ey) / 2., *ex, *ey),
                    Rec::Curve(cx, cy, ex, ey) => (*cx, *cy, *ex, *ey),
                    Rec::Style { .. } => continue,
                };
                recs.push(Rec::Curve(
                    lerp(ax, qx),
                    lerp(ay, qy),
                    lerp(x, ex),
                    lerp(y, ey),
                ));
                (px, py) = (x, y);
            }
        }
    }
    let [a, b] = m.bounds;
    ShapeData {
        bounds: [
            lerp(f64::from(a.x0), f64::from(b.x0)),
            lerp(f64::from(a.y0), f64::from(b.y0)),
            lerp(f64::from(a.x1), f64::from(b.x1)),
            lerp(f64::from(a.y1), f64::from(b.y1)),
        ],
        fills,
        lines,
        recs,
    }
}

fn px(tw: f64) -> f64 {
    tw / 20.
}

fn shape_out(sh: &ShapeData) -> Value {
    let mut fills: BTreeMap<usize, Vec<Value>> = BTreeMap::new();
    let mut strokes: BTreeMap<usize, Vec<Value>> = BTreeMap::new();
    let (mut x, mut y) = (0f64, 0f64);
    let (mut f0, mut f1, mut ln) = (0usize, 0usize, 0usize);
    for r in &sh.recs {
        let e = match r {
            Rec::Style {
                pos,
                f0: a,
                f1: b,
                ln: c,
                new_styles,
            } => {
                if *new_styles {
                    (f0, f1, ln) = (0, 0, 0);
                }
                if let Some(p) = pos {
                    (x, y) = *p;
                }
                if let Some(v) = a {
                    f0 = *v;
                }
                if let Some(v) = b {
                    f1 = *v;
                }
                if let Some(v) = c {
                    ln = *v;
                }
                continue;
            }
            Rec::Line(nx, ny) => {
                let e = json!([px(x), px(y), px(*nx), px(*ny)]);
                (x, y) = (*nx, *ny);
                e
            }
            Rec::Curve(cx, cy, nx, ny) => {
                let e = json!([px(x), px(y), px(*cx), px(*cy), px(*nx), px(*ny)]);
                (x, y) = (*nx, *ny);
                e
            }
        };
        if f0 != 0 {
            fills.entry(f0).or_default().push(e.clone());
        }
        if f1 != 0 {
            fills.entry(f1).or_default().push(e.clone());
        }
        if ln != 0 {
            strokes.entry(ln).or_default().push(e);
        }
    }
    let mat20 = |m: &M| {
        json!([
            m.a / 20.,
            m.b / 20.,
            m.c / 20.,
            m.d / 20.,
            m.tx / 20.,
            m.ty / 20.
        ])
    };
    let mut out_fills = Vec::new();
    for (fi, edges) in fills {
        let Some(f) = sh.fills.get(fi - 1) else {
            continue;
        };
        let mut o = if f.ty == 0 {
            json!({"kind": "solid", "color": f.color})
        } else if let Some(bitmap) = f.bitmap {
            if bitmap == 65535 {
                json!({"kind": "none"})
            } else {
                json!({"kind": "bitmap", "image": bitmap,
                    "repeat": f.ty == 0x40 || f.ty == 0x42,
                    "smooth": f.ty == 0x40 || f.ty == 0x41,
                    "matrix": mat20(&f.matrix)})
            }
        } else {
            let stops: Vec<Value> = f
                .stops
                .iter()
                .map(|s| json!([s.0, s.1[0], s.1[1], s.1[2], s.1[3]]))
                .collect();
            json!({"kind": if f.ty == 0x10 { "linear" } else { "radial" },
                "matrix": mat20(&f.matrix), "stops": stops})
        };
        o["edges"] = Value::Array(edges);
        out_fills.push(o);
    }
    let mut out_strokes = Vec::new();
    for (li, edges) in strokes {
        let Some(l) = sh.lines.get(li - 1) else {
            continue;
        };
        out_strokes.push(json!({"width": px(l.width), "color": l.color, "edges": edges}));
    }
    json!({"bounds": [px(sh.bounds[0]), px(sh.bounds[1]), px(sh.bounds[2]), px(sh.bounds[3])],
        "fills": out_fills, "strokes": out_strokes})
}

fn edit_text(b: &[u8], t: &Tag) -> (u32, Value) {
    let mut s = Bits::new(b, t.pos);
    let id = u32::from(s.u16());
    let r = s.rect();
    s.align();
    let has_text = s.ub(1) != 0;
    let word_wrap = s.ub(1) != 0;
    let multiline = s.ub(1) != 0;
    s.ub(1);
    s.ub(1);
    let has_color = s.ub(1) != 0;
    let has_max = s.ub(1) != 0;
    let has_font = s.ub(1) != 0;
    let has_class = s.ub(1) != 0;
    s.ub(1);
    let has_layout = s.ub(1) != 0;
    s.ub(1);
    s.ub(1);
    s.ub(1);
    let html = s.ub(1) != 0;
    s.ub(1);
    let mut o = json!({"bounds": [px(f64::from(r.x0)), px(f64::from(r.y0)), px(f64::from(r.x1)), px(f64::from(r.y1))],
        "multiline": multiline, "word_wrap": word_wrap, "html": html});
    if has_font {
        o["font"] = json!(s.u16());
    }
    if has_class {
        o["font_class"] = json!(s.str());
    }
    if has_font || has_class {
        o["size"] = json!(px(f64::from(s.u16())));
    }
    if has_color {
        o["color"] = json!(s.rgba());
    }
    if has_max {
        s.u16();
    }
    if has_layout {
        let align = s.u8();
        o["align"] = json!(["left", "right", "center", "justify"][usize::from(align.min(3))]);
        s.u16();
        s.u16();
        s.u16();
        o["leading"] = json!(px(f64::from(s.s16())));
    }
    s.str();
    o["initial"] = json!(if has_text { s.str() } else { String::new() });
    (id, o)
}

fn place(b: &[u8], t: &Tag, v3: bool) -> Value {
    let mut s = Bits::new(b, t.pos);
    let fl = s.u8();
    let fl2 = if v3 { s.u8() } else { 0 };
    let mut q = Map::new();
    q.insert("d".into(), json!(s.u16()));
    if fl2 & 8 != 0 {
        q.insert("class".into(), json!(s.str()));
    }
    if fl & 1 != 0 {
        q.insert("move".into(), json!(true));
    }
    if fl & 2 != 0 {
        q.insert("ch".into(), json!(s.u16()));
    }
    if fl & 4 != 0 {
        let m = s.matrix();
        q.insert("m".into(), json!([m.a, m.b, m.c, m.d, px(m.tx), px(m.ty)]));
    }
    if fl & 8 != 0 {
        let c = s.cxform(true);
        q.insert(
            "cx".into(),
            json!([
                f64::from(c[0]) / 256.,
                f64::from(c[1]) / 256.,
                f64::from(c[2]) / 256.,
                f64::from(c[3]) / 256.,
                c[4],
                c[5],
                c[6],
                c[7]
            ]),
        );
    }
    if fl & 16 != 0 {
        q.insert("ratio".into(), json!(s.u16()));
    }
    if fl & 32 != 0 {
        q.insert("name".into(), json!(s.str()));
    }
    if fl & 64 != 0 {
        q.insert("clip".into(), json!(s.u16()));
    }
    if fl2 & 1 != 0 {
        let n = s.u8();
        for _ in 0..n {
            let id = s.u8();
            match id {
                0 => {
                    s.rgba();
                    s.fixed();
                    s.fixed();
                    s.fixed();
                    s.fixed();
                    s.fixed8();
                    s.u8();
                }
                1 => {
                    s.fixed();
                    s.fixed();
                    s.u8();
                }
                2 => {
                    s.rgba();
                    s.fixed();
                    s.fixed();
                    s.fixed8();
                    s.u8();
                }
                3 => {
                    s.rgba();
                    s.rgba();
                    s.fixed();
                    s.fixed();
                    s.fixed();
                    s.fixed();
                    s.fixed8();
                    s.u8();
                }
                4 | 7 => {
                    let nc = usize::from(s.u8());
                    s.p += nc * 5 + 16 + 2 + 1;
                }
                5 => {
                    let (mx, my) = (usize::from(s.u8()), usize::from(s.u8()));
                    s.p += 8 + mx * my * 4 + 4 + 1;
                }
                6 => {
                    for _ in 0..20 {
                        s.f32();
                    }
                }
                _ => {}
            }
        }
    }
    if fl2 & 2 != 0 {
        q.insert("blend".into(), json!(s.u8()));
    }
    if fl2 & 4 != 0 {
        s.u8();
    }
    if fl2 & 32 != 0 {
        q.insert("visible".into(), json!(s.u8() != 0));
    }
    Value::Object(q)
}

#[derive(Default)]
struct Timeline {
    frames: Vec<(Vec<Value>, Vec<String>)>,
}

/// One image of a movie.
#[derive(Clone, Debug)]
pub struct Image {
    pub id: u32,
    pub w: u32,
    pub h: u32,
    pub export_name: Option<String>,
    pub file: String,
}

#[derive(Default)]
struct Swf {
    stage: (f64, f64),
    rate: f64,
    shapes: BTreeMap<u32, ShapeData>,
    morphs: BTreeMap<u32, MorphData>,
    texts: BTreeMap<u32, Value>,
    sprites: BTreeMap<u32, Timeline>,
    images: Vec<Image>,
    subs: Vec<(u32, u32, [u32; 4])>,
    symbols: BTreeMap<u32, String>,
    grids: BTreeMap<u32, R4>,
    abc: Vec<(usize, usize)>,
    fonts: Vec<(u32, usize, usize)>,
}

fn timeline(b: &[u8], tags: &[Tag], swf: &mut Swf) -> Res<Timeline> {
    let mut tl = Timeline::default();
    let mut cur: (Vec<Value>, Vec<String>) = (Vec::new(), Vec::new());
    for t in tags {
        match t.code {
            1 => tl.frames.push(std::mem::take(&mut cur)),
            26 => cur.0.push(place(b, t, false)),
            70 => cur.0.push(place(b, t, true)),
            5 | 28 => {
                let at = t.pos + if t.code == 5 { 2 } else { 0 };
                let d = u16::from_le_bytes([b[at], b[at + 1]]);
                cur.0.push(json!({"d": d, "remove": true}));
            }
            43 => {
                let mut s = Bits::new(b, t.pos);
                cur.1.push(s.str());
            }
            _ => define(b, t, swf)?,
        }
    }
    Ok(tl)
}

fn define(b: &[u8], t: &Tag, swf: &mut Swf) -> Res<()> {
    match t.code {
        2 | 22 | 32 | 83 => {
            let ver = match t.code {
                2 => 1,
                22 => 2,
                32 => 3,
                _ => 4,
            };
            let (id, sh) = parse_shape(b, t, ver)?;
            swf.shapes.insert(id, sh);
        }
        46 | 84 => {
            let (id, m) = parse_morph(b, t, t.code == 84)?;
            swf.morphs.insert(id, m);
        }
        37 => {
            let (id, o) = edit_text(b, t);
            swf.texts.insert(id, o);
        }
        39 => {
            let id = u32::from(u16::from_le_bytes([b[t.pos], b[t.pos + 1]]));
            let sub = read_tags(b, t.pos + 4, t.pos + t.len);
            let tl = timeline(b, &sub, swf)?;
            swf.sprites.insert(id, tl);
        }
        76 => {
            let mut s = Bits::new(b, t.pos);
            let n = s.u16();
            for _ in 0..n {
                let id = u32::from(s.u16());
                let name = s.str();
                swf.symbols.insert(id, name);
            }
        }
        78 => {
            let mut s = Bits::new(b, t.pos);
            let id = u32::from(s.u16());
            let r = s.rect();
            swf.grids.insert(id, r);
        }
        82 => {
            let mut s = Bits::new(b, t.pos);
            s.u32();
            s.str();
            swf.abc.push((s.p, t.pos + t.len - s.p));
        }
        1001 | 1009 => {
            let mut s = Bits::new(b, t.pos);
            let id = if t.code == 1009 {
                s.u32()
            } else {
                u32::from(s.u16())
            };
            s.u16();
            let (w, h) = (u32::from(s.u16()), u32::from(s.u16()));
            let mut export_name = None;
            if t.code == 1009 {
                let el = usize::from(s.u8());
                export_name = Some(s.latin(el));
            }
            let fl = usize::from(s.u8());
            let file = s.latin(fl);
            swf.images.push(Image {
                id: id & 0xffff,
                w,
                h,
                export_name,
                file,
            });
        }
        1008 => {
            let mut s = Bits::new(b, t.pos);
            let id = u32::from(s.u16());
            let image = u32::from(s.u16());
            let r = [s.u16(), s.u16(), s.u16(), s.u16()].map(u32::from);
            swf.subs.push((id, image, r));
        }
        1005 => {
            let id = u32::from(u16::from_le_bytes([b[t.pos], b[t.pos + 1]]));
            swf.fonts.push((id, t.pos + 2, t.len - 2));
        }
        86 => {
            // Scene labels are in the frame labels too.
            let mut s = Bits::new(b, t.pos);
            s.eu32();
        }
        _ => {}
    }
    Ok(())
}

fn parse(raw: &[u8]) -> Res<(Swf, Timeline)> {
    if raw.len() < 12 || !(raw.starts_with(b"GFX") || raw.starts_with(b"FWS")) {
        return bad("not an uncompressed GFX movie");
    }
    let mut s = Bits::new(raw, 8);
    let frame = s.rect();
    let rate = f64::from(s.u16()) / 256.;
    s.u16();
    let tags = read_tags(raw, s.p, raw.len());
    let mut swf = Swf {
        stage: (
            f64::from(frame.x1 - frame.x0) / 20.,
            f64::from(frame.y1 - frame.y0) / 20.,
        ),
        rate,
        ..Swf::default()
    };
    let root = timeline(raw, &tags, &mut swf)?;
    Ok((swf, root))
}

// ---- ABC: the classes' superclasses and constants, and their frame scripts

struct AbcR<'a> {
    b: &'a [u8],
    p: usize,
}

impl AbcR<'_> {
    fn u8(&mut self) -> u8 {
        let v = self.b.get(self.p).copied().unwrap_or(0);
        self.p += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        u16::from(self.u8()) | u16::from(self.u8()) << 8
    }
    fn s24(&mut self) -> i32 {
        let v = u32::from(self.u8()) | u32::from(self.u8()) << 8 | u32::from(self.u8()) << 16;
        ((v << 8) as i32) >> 8
    }
    fn u30(&mut self) -> u32 {
        let (mut v, mut sh) = (0u32, 0);
        for _ in 0..5 {
            let x = self.u8();
            v |= u32::from(x & 127) << sh;
            sh += 7;
            if x & 128 == 0 {
                break;
            }
        }
        v
    }
    fn str(&mut self) -> String {
        let n = self.u30() as usize;
        let s = String::from_utf8_lossy(self.b.get(self.p..self.p + n).unwrap_or_default())
            .into_owned();
        self.p += n;
        s
    }
}

#[derive(Default, Clone)]
struct Mn {
    kind: u8,
    ns: u32,
    name: u32,
    rtname: bool,
    rtns: bool,
}

#[derive(Default)]
struct Trait {
    name: u32,
    kind: u8,
    vindex: u32,
    vkind: u8,
    method: Option<u32>,
}

#[derive(Default)]
struct Abc {
    ints: Vec<i32>,
    uints: Vec<u32>,
    doubles: Vec<f64>,
    strings: Vec<String>,
    ns: Vec<(u8, u32)>,
    mn: Vec<Mn>,
    instances: Vec<(u32, u32, u32, Vec<Trait>)>,
    classes: Vec<Vec<Trait>>,
    bodies: HashMap<u32, (usize, usize)>,
}

impl Abc {
    fn string(&self, i: u32) -> &str {
        self.strings.get(i as usize).map_or("", String::as_str)
    }
    fn mname(&self, i: u32) -> String {
        if i == 0 {
            return "*".into();
        }
        let Some(m) = self.mn.get(i as usize) else {
            return format!("mn#{i}");
        };
        if m.rtname {
            return "[rt]".into();
        }
        self.string(m.name).to_string()
    }
    fn qname(&self, i: u32) -> String {
        let Some(m) = self.mn.get(i as usize).filter(|_| i != 0) else {
            return "*".into();
        };
        if m.kind == 0x07 || m.kind == 0x0d {
            let (kind, name) = self.ns.get(m.ns as usize).copied().unwrap_or((0, 0));
            let ns = self.string(name);
            let nm = self.string(m.name);
            return if !ns.is_empty() && kind != 5 {
                format!("{ns}.{nm}")
            } else {
                nm.to_string()
            };
        }
        self.mname(i)
    }
}

fn traits(r: &mut AbcR) -> Res<Vec<Trait>> {
    let c = r.u30();
    let mut out = Vec::with_capacity(c as usize);
    for _ in 0..c {
        let mut t = Trait {
            name: r.u30(),
            ..Trait::default()
        };
        let k = r.u8();
        t.kind = k & 15;
        let attr = k >> 4;
        match t.kind {
            0 | 6 => {
                r.u30();
                r.u30();
                t.vindex = r.u30();
                if t.vindex != 0 {
                    t.vkind = r.u8();
                }
            }
            4 => {
                r.u30();
                r.u30();
            }
            5 => {
                r.u30();
                t.method = Some(r.u30());
            }
            1..=3 => {
                r.u30();
                t.method = Some(r.u30());
            }
            _ => return bad("trait kind"),
        }
        if attr & 4 != 0 {
            let mc = r.u30();
            for _ in 0..mc {
                r.u30();
            }
        }
        out.push(t);
    }
    Ok(out)
}

fn parse_abc(buf: &[u8], pos: usize) -> Res<Abc> {
    let mut r = AbcR { b: buf, p: pos };
    r.u16();
    r.u16();
    let mut a = Abc::default();
    let n = r.u30();
    a.ints.push(0);
    for _ in 1..n {
        a.ints.push(r.u30() as i32);
    }
    let n = r.u30();
    a.uints.push(0);
    for _ in 1..n {
        a.uints.push(r.u30());
    }
    let n = r.u30();
    a.doubles.push(f64::NAN);
    for _ in 1..n {
        let b = r.b.get(r.p..r.p + 8).unwrap_or(&[0; 8]);
        let mut w = [0; 8];
        w.copy_from_slice(&b[..8]);
        a.doubles.push(f64::from_le_bytes(w));
        r.p += 8;
    }
    let n = r.u30();
    a.strings.push(String::new());
    for _ in 1..n {
        a.strings.push(r.str());
    }
    let n = r.u30();
    a.ns.push((0, 0));
    for _ in 1..n {
        let kind = r.u8();
        a.ns.push((kind, r.u30()));
    }
    let n = r.u30();
    for _ in 1..n {
        let c = r.u30();
        for _ in 0..c {
            r.u30();
        }
    }
    let n = r.u30();
    a.mn.push(Mn::default());
    for _ in 1..n {
        let kind = r.u8();
        let mut m = Mn {
            kind,
            ..Mn::default()
        };
        match kind {
            0x07 | 0x0d => {
                m.ns = r.u30();
                m.name = r.u30();
            }
            0x0f | 0x10 => {
                m.name = r.u30();
                m.rtns = true;
            }
            0x11 | 0x12 => {
                m.rtns = true;
                m.rtname = true;
            }
            0x09 | 0x0e => {
                m.name = r.u30();
                r.u30();
            }
            0x1b | 0x1c => {
                r.u30();
                m.rtname = true;
            }
            0x1d => {
                r.u30();
                let c = r.u30();
                for _ in 0..c {
                    r.u30();
                }
            }
            _ => return bad("multiname kind"),
        }
        a.mn.push(m);
    }
    let n = r.u30();
    for _ in 0..n {
        let pc = r.u30();
        r.u30();
        for _ in 0..pc {
            r.u30();
        }
        r.u30();
        let flags = r.u8();
        if flags & 0x08 != 0 {
            let oc = r.u30();
            for _ in 0..oc {
                r.u30();
                r.u8();
            }
        }
        if flags & 0x80 != 0 {
            for _ in 0..pc {
                r.u30();
            }
        }
    }
    let n = r.u30();
    for _ in 0..n {
        r.u30();
        let c = r.u30();
        for _ in 0..c * 2 {
            r.u30();
        }
    }
    let n = r.u30();
    for _ in 0..n {
        let (name, sup) = (r.u30(), r.u30());
        let flags = r.u8();
        if flags & 8 != 0 {
            r.u30();
        }
        let ic = r.u30();
        for _ in 0..ic {
            r.u30();
        }
        let iinit = r.u30();
        let t = traits(&mut r)?;
        a.instances.push((name, sup, iinit, t));
    }
    for _ in 0..n {
        r.u30();
        a.classes.push(traits(&mut r)?);
    }
    let sc = r.u30();
    for _ in 0..sc {
        r.u30();
        traits(&mut r)?;
    }
    let bn = r.u30();
    for _ in 0..bn {
        let method = r.u30();
        r.u30();
        r.u30();
        r.u30();
        r.u30();
        let cl = r.u30() as usize;
        a.bodies.insert(method, (r.p, cl));
        r.p += cl;
        let ec = r.u30();
        for _ in 0..ec {
            r.u30();
            r.u30();
            r.u30();
            r.u30();
            r.u30();
        }
        traits(&mut r)?;
    }
    Ok(a)
}

/// The statements of a straight-line method, as text (`decompile`, for the forms frame scripts
/// and constructors have). Anything else ends it with what it has.
fn decompile(a: &Abc, buf: &[u8], method: u32) -> Vec<String> {
    let Some(&(pos, len)) = a.bodies.get(&method) else {
        return Vec::new();
    };
    let mut r = AbcR { b: buf, p: pos };
    let end = pos + len;
    let mut st: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let pop = |st: &mut Vec<String>| st.pop().unwrap_or_else(|| "/*empty*/".to_string());
    let prop = |a: &Abc, st: &mut Vec<String>, mi: u32| -> String {
        let m = a.mn.get(mi as usize).cloned().unwrap_or_default();
        let nm = if m.rtname {
            format!("[{}]", st.pop().unwrap_or_default())
        } else {
            a.mname(mi)
        };
        if m.rtns {
            st.pop();
        }
        let o = st.pop().unwrap_or_default();
        if o == "<scope>" {
            nm
        } else {
            format!("{o}.{nm}")
        }
    };
    let args = |st: &mut Vec<String>, n: u32| -> String {
        let mut v = Vec::with_capacity(n as usize);
        for _ in 0..n {
            v.push(st.pop().unwrap_or_default());
        }
        v.reverse();
        v.join(", ")
    };
    while r.p < end {
        let op = r.u8();
        match op {
            0x02 | 0x09 | 0xf0 | 0x1d => {
                if op == 0xf0 {
                    r.u30();
                }
            }
            0xef => {
                r.u8();
                r.u30();
                r.u8();
                r.u30();
            }
            0xf1 => {
                r.u30();
            }
            0xd0 => st.push("this".into()),
            0xd1..=0xd3 => st.push(format!("a{}", op - 0xd0)),
            0x62 => {
                let n = r.u30();
                st.push(format!("l{n}"));
            }
            0x30 | 0x1c => {
                st.pop();
            }
            0x24 => {
                let v = r.u8() as i8;
                st.push(v.to_string());
            }
            0x25 => {
                let v = r.u30() as i16;
                st.push(v.to_string());
            }
            0x2d => {
                let v = r.u30();
                st.push(a.ints.get(v as usize).copied().unwrap_or(0).to_string());
            }
            0x2e => {
                let v = r.u30();
                st.push(a.uints.get(v as usize).copied().unwrap_or(0).to_string());
            }
            0x2f => {
                let v = r.u30();
                st.push(a.doubles.get(v as usize).copied().unwrap_or(0.).to_string());
            }
            0x2c => {
                let v = r.u30();
                st.push(format!("\"{}\"", a.string(v)));
            }
            0x26 => st.push("true".into()),
            0x27 => st.push("false".into()),
            0x20 => st.push("null".into()),
            0x21 => st.push("undefined".into()),
            0x29 => {
                let e = pop(&mut st);
                if e.contains('(') || e.contains('=') {
                    out.push(format!("{e};"));
                }
            }
            0x2a => {
                let e = pop(&mut st);
                st.push(e.clone());
                st.push(e);
            }
            0x64 => st.push("<scope>".into()),
            0x65 => {
                let v = r.u8();
                st.push(if v == 0 {
                    "this".into()
                } else {
                    "<scope>".into()
                });
            }
            0x5d | 0x5e => {
                let mi = r.u30();
                let m = a.mn.get(mi as usize).cloned().unwrap_or_default();
                if m.rtname {
                    st.pop();
                }
                if m.rtns {
                    st.pop();
                }
                st.push("<scope>".into());
            }
            0x60 => {
                let mi = r.u30();
                st.push(a.mname(mi));
            }
            0x66 => {
                let mi = r.u30();
                let p = prop(a, &mut st, mi);
                st.push(p);
            }
            0x61 | 0x68 => {
                let mi = r.u30();
                let v = pop(&mut st);
                let p = prop(a, &mut st, mi);
                out.push(format!("{p} = {v};"));
            }
            0x46 | 0x4c => {
                let mi = r.u30();
                let n = r.u30();
                let x = args(&mut st, n);
                let p = prop(a, &mut st, mi);
                st.push(format!("{p}({x})"));
            }
            0x4f => {
                let mi = r.u30();
                let n = r.u30();
                let x = args(&mut st, n);
                let p = prop(a, &mut st, mi);
                out.push(format!("{p}({x});"));
            }
            0x4a => {
                let mi = r.u30();
                let n = r.u30();
                let x = args(&mut st, n);
                let p = prop(a, &mut st, mi);
                st.push(format!("new {p}({x})"));
            }
            0x42 => {
                let n = r.u30();
                let x = args(&mut st, n);
                let f = pop(&mut st);
                st.push(format!("new {f}({x})"));
            }
            0x49 => {
                let n = r.u30();
                let x = args(&mut st, n);
                st.pop();
                out.push(format!("super({x});"));
            }
            0x4e => {
                let mi = r.u30();
                let n = r.u30();
                let x = args(&mut st, n);
                st.pop();
                out.push(format!("super.{}({x});", a.mname(mi)));
            }
            0x47 => {}
            0x48 => {
                st.pop();
            }
            0x70 | 0x85 | 0x75 | 0x76 | 0x77 | 0x82 | 0x73 | 0x74 => {}
            0x80 | 0x86 => {
                r.u30();
            }
            // Control flow and the rest are skipped: what is sought stands in a straight run
            // (`addFrameScript` after a constructor's own branches).
            0x0c..=0x1a => {
                r.s24();
                st.clear();
            }
            0x1b => {
                r.s24();
                let c = r.u30();
                for _ in 0..=c {
                    r.s24();
                }
                st.clear();
            }
            0x08
            | 0x92
            | 0x94
            | 0xc2
            | 0xc3
            | 0x63
            | 0x6c..=0x6f
            | 0x5a
            | 0x06
            | 0x04
            | 0x05
            | 0x59
            | 0x6a
            | 0xb2
            | 0x31
            | 0x40
            | 0x41
            | 0x53
            | 0x55
            | 0x56
            | 0x58 => {
                r.u30();
                st.push("?".into());
            }
            0x32 | 0x43 | 0x44 | 0x45 => {
                r.u30();
                r.u30();
                st.push("?".into());
            }
            _ => st.push("?".into()),
        }
    }
    out
}

struct ClassInfo {
    sup: String,
    frame_scripts: BTreeMap<u32, Vec<String>>,
    consts: Map<String, Value>,
}

fn classes_of(raw: &[u8], swf: &Swf) -> Res<HashMap<String, ClassInfo>> {
    let mut out = HashMap::new();
    for &(pos, _) in &swf.abc {
        let a = parse_abc(raw, pos)?;
        for (ci, (name, sup, iinit, inst_traits)) in a.instances.iter().enumerate() {
            let cname = a.qname(*name);
            let mut info = ClassInfo {
                sup: a.qname(*sup),
                frame_scripts: BTreeMap::new(),
                consts: Map::new(),
            };
            for t in a.classes.get(ci).into_iter().flatten() {
                if t.kind == 6 && t.vindex != 0 {
                    let v = match t.vkind {
                        3 => a.ints.get(t.vindex as usize).map(|&v| json!(v)),
                        4 => a.uints.get(t.vindex as usize).map(|&v| json!(v)),
                        6 => a.doubles.get(t.vindex as usize).map(|&v| json!(v)),
                        _ => None,
                    };
                    if let Some(v) = v {
                        info.consts.insert(a.mname(t.name), v);
                    }
                }
            }
            // `addFrameScript(0, this.frame1, 9, this.frame10, ...)` in the constructor.
            let mut map: HashMap<String, u32> = HashMap::new();
            for line in decompile(&a, raw, *iinit) {
                if let Some(rest) = line.split("addFrameScript(").nth(1) {
                    let rest = rest.trim_end_matches(';').trim_end_matches(')');
                    let parts: Vec<&str> = rest.split(',').map(str::trim).collect();
                    for pair in parts.chunks_exact(2) {
                        if let Ok(f) = pair[0].parse::<u32>() {
                            map.insert(pair[1].trim_start_matches("this.").to_string(), f + 1);
                        }
                    }
                }
            }
            for t in inst_traits {
                if let Some(m) = t.method
                    && let Some(&frame) = map.get(&a.mname(t.name))
                {
                    let lines: Vec<String> = decompile(&a, raw, m)
                        .into_iter()
                        .map(|s| s.trim().to_string())
                        .filter(|s| s != "return;")
                        .collect();
                    info.frame_scripts.insert(frame, lines);
                }
            }
            out.insert(cname, info);
        }
    }
    Ok(out)
}

fn sprite_out(tl: &Timeline, class: Option<&str>, classes: &HashMap<String, ClassInfo>) -> Value {
    let mut labels = Map::new();
    let mut time = Map::new();
    for (i, (ops, ls)) in tl.frames.iter().enumerate() {
        for l in ls {
            labels.insert(l.clone(), json!(i + 1));
        }
        if !ops.is_empty() {
            time.insert((i + 1).to_string(), Value::Array(ops.clone()));
        }
    }
    let mut s = json!({"frames": tl.frames.len(), "labels": labels, "timeline": time});
    if let Some(c) = class {
        s["class"] = json!(c);
        if let Some(info) = classes.get(&c.replace("::", "."))
            && !info.frame_scripts.is_empty()
        {
            let scripts: Map<String, Value> = info
                .frame_scripts
                .iter()
                .map(|(f, l)| (f.to_string(), json!(l)))
                .collect();
            s["scripts"] = Value::Object(scripts);
        }
    }
    s
}

/// The images a movie names, for its reader to find the textures of.
pub struct Movie {
    pub json: Value,
    pub images: Vec<Image>,
    /// Compacted fonts: class name, and their byte range in the raw movie.
    pub fonts: Vec<(String, usize, usize)>,
}

/// Reads a movie. An image's `texture` is its export name (or file stem), which the reader then
/// resolves: `texture_key` gives that key for an image.
pub fn read_movie(name: &str, raw: &[u8]) -> Res<Movie> {
    let (swf, root) = parse(raw)?;
    let classes = classes_of(raw, &swf)?;
    let mut images = Map::new();
    for im in &swf.images {
        images.insert(
            im.id.to_string(),
            json!({"export_name": im.export_name, "source_file": im.file,
                "declared": [im.w, im.h], "texture": texture_key(im)}),
        );
    }
    for (id, parent, r) in &swf.subs {
        images.insert(
            id.to_string(),
            json!({"sub_image_of": parent, "rect": [r[0], r[1], r[2] - r[0], r[3] - r[1]]}),
        );
    }
    let symbols: Map<String, Value> = swf
        .symbols
        .iter()
        .map(|(id, n)| (id.to_string(), json!(n)))
        .collect();
    let scale9: Map<String, Value> = swf
        .grids
        .iter()
        .map(|(id, g)| {
            (
                id.to_string(),
                json!([
                    px(f64::from(g.x0)),
                    px(f64::from(g.y0)),
                    px(f64::from(g.x1)),
                    px(f64::from(g.y1))
                ]),
            )
        })
        .collect();
    let shapes: Map<String, Value> = swf
        .shapes
        .iter()
        .map(|(id, s)| (id.to_string(), shape_out(s)))
        .collect();
    let morphs: Map<String, Value> = swf
        .morphs
        .iter()
        .map(|(id, m)| {
            (
                id.to_string(),
                json!({"start": shape_out(&morph_at(m, 0.)), "end": shape_out(&morph_at(m, 1.))}),
            )
        })
        .collect();
    let texts: Map<String, Value> = swf
        .texts
        .iter()
        .map(|(id, t)| (id.to_string(), t.clone()))
        .collect();
    let sprites: Map<String, Value> = swf
        .sprites
        .iter()
        .map(|(id, tl)| {
            (
                id.to_string(),
                sprite_out(tl, swf.symbols.get(id).map(String::as_str), &classes),
            )
        })
        .collect();
    let doc = swf.symbols.get(&0).map(String::as_str);
    let root_out = sprite_out(&root, doc, &classes);
    let consts = doc
        .and_then(|d| classes.get(&d.replace("::", ".")))
        .map_or_else(Map::new, |c| c.consts.clone());
    let class_table: Map<String, Value> = classes
        .iter()
        .map(|(k, c)| (k.clone(), json!({"super": c.sup})))
        .collect();
    let json = json!({"format": "kh3-ui-movie/1", "name": name,
        "stage": [swf.stage.0, swf.stage.1], "fps": swf.rate,
        "images": images, "symbols": symbols, "scale9": scale9,
        "shapes": shapes, "morphs": morphs, "texts": texts, "sprites": sprites, "root": root_out,
        "as3": {"constants": consts, "classes": class_table}});
    let fonts = swf
        .fonts
        .iter()
        .filter_map(|(id, pos, len)| Some((swf.symbols.get(id)?.clone(), *pos, *len)))
        .collect();
    Ok(Movie {
        json,
        images: swf.images.clone(),
        fonts,
    })
}

/// The name an image's texture goes by.
pub fn texture_key(im: &Image) -> String {
    im.export_name.clone().unwrap_or_else(|| {
        im.file
            .rsplit_once('.')
            .map_or(im.file.clone(), |(stem, _)| stem.to_string())
    })
}

// ---- GFx compacted fonts

const FONT_RANGES: [(u32, u32); 10] = [
    (0x20, 0x7e),
    (0xa0, 0x17f),
    (0x2010, 0x2027),
    (0x2030, 0x203a),
    (0x20ac, 0x20ac),
    (0x2122, 0x2122),
    (0x2190, 0x2193),
    (0x25a0, 0x25cf),
    (0x3000, 0x3002),
    (0xff01, 0xff5e),
];

/// Decodes a compacted font's glyphs in the ranges the HUD shows.
pub fn read_font(class_name: &str, raw: &[u8], pos: usize, _len: usize) -> Res<Font> {
    let byte = |at: usize| raw.get(at).copied().unwrap_or(0);
    let u16_at = |at: usize| u16::from_le_bytes([byte(at), byte(at + 1)]);
    let i16_at = |at: usize| u16_at(at) as i16;
    let u32_at =
        |at: usize| u32::from_le_bytes([byte(at), byte(at + 1), byte(at + 2), byte(at + 3)]);
    let mut p = pos;
    while byte(p) != 0 && p < raw.len() {
        p += 1;
    }
    p += 1;
    let nominal = f32::from(u16_at(p + 2).max(1));
    let (ascent, descent, leading) = (i16_at(p + 4), i16_at(p + 6), i16_at(p + 8));
    let count = u32_at(p + 10) as usize;
    let bytes = u32_at(p + 14) as usize;
    p += 18 + bytes;
    let mut font = Font {
        class_name: class_name.to_string(),
        units_per_em: nominal,
        ascent: f32::from(ascent),
        descent: f32::from(descent),
        leading: f32::from(leading),
        glyphs: HashMap::new(),
    };
    const EDGE: [(u8, u32, u32); 16] = [
        (b'H', 12, 1),
        (b'H', 20, 1),
        (b'V', 12, 1),
        (b'V', 20, 1),
        (b'L', 6, 2),
        (b'L', 10, 2),
        (b'L', 14, 2),
        (b'L', 18, 2),
        (b'C', 5, 4),
        (b'C', 7, 4),
        (b'C', 9, 4),
        (b'C', 11, 4),
        (b'C', 13, 4),
        (b'C', 15, 4),
        (b'C', 17, 4),
        (b'C', 19, 4),
    ];
    for i in 0..count {
        let at = p + i * 8;
        let code = u32::from(u16_at(at));
        if !FONT_RANGES.iter().any(|r| (r.0..=r.1).contains(&code)) {
            continue;
        }
        let advance = f32::from(i16_at(at + 2));
        let mut q = pos + u32_at(at + 4) as usize;
        let u15 = |q: &mut usize| -> i32 {
            let t = byte(*q);
            if t & 1 == 0 {
                *q += 1;
                i32::from(t >> 1)
            } else {
                let v = u16_at(*q);
                *q += 2;
                i32::from(v >> 1)
            }
        };
        let s15 = |q: &mut usize| -> i32 {
            let t = byte(*q);
            if t & 1 == 0 {
                *q += 1;
                i32::from((t as i8) >> 1)
            } else {
                let v = i16_at(*q);
                *q += 2;
                i32::from(v >> 1)
            }
        };
        let u30 = |q: &mut usize| -> u32 {
            let n = usize::from(byte(*q) & 3) + 1;
            let mut v = 0u64;
            for k in 0..n {
                v += u64::from(byte(*q + k)) << (8 * k);
            }
            *q += n;
            (v / 4) as u32
        };
        for _ in 0..4 {
            s15(&mut q);
        }
        let nc = u15(&mut q);
        if nc > 2000 {
            return bad("font: implausible glyph");
        }
        let mut contours = Vec::with_capacity(nc as usize);
        for _ in 0..nc {
            let (mut x, mut y) = (s15(&mut q), s15(&mut q));
            let mut path = vec![Seg::Move(x as f32, y as f32)];
            let mut n = u30(&mut q);
            let mut ret = None;
            if n & 1 != 0 {
                ret = Some(q);
                q = pos + (n >> 1) as usize;
                s15(&mut q);
                s15(&mut q);
                n = u30(&mut q);
            }
            n >>= 1;
            if n > 20000 {
                return bad("font: implausible contour");
            }
            for _ in 0..n {
                let (kind, bits, cnt) = EDGE[usize::from(byte(q) & 15)];
                let total = 4 + bits * cnt;
                let mut vals = [0i32; 4];
                let mut bitpos = 4u32;
                for v in vals.iter_mut().take(cnt as usize) {
                    let mut w: i64 = 0;
                    for k in 0..bits {
                        let bp = bitpos + k;
                        if byte(q + (bp >> 3) as usize) & (1 << (bp & 7)) != 0 {
                            w |= 1 << k;
                        }
                    }
                    if w >= 1 << (bits - 1) {
                        w -= 1 << bits;
                    }
                    *v = w as i32;
                    bitpos += bits;
                }
                q += (total >> 3) as usize;
                match kind {
                    b'H' => {
                        x += vals[0];
                        path.push(Seg::Line(x as f32, y as f32));
                    }
                    b'V' => {
                        y += vals[0];
                        path.push(Seg::Line(x as f32, y as f32));
                    }
                    b'L' => {
                        x += vals[0];
                        y += vals[1];
                        path.push(Seg::Line(x as f32, y as f32));
                    }
                    _ => {
                        let (cx, cy) = (x + vals[0], y + vals[1]);
                        x = cx + vals[2];
                        y = cy + vals[3];
                        path.push(Seg::Quad(cx as f32, cy as f32, x as f32, y as f32));
                    }
                }
            }
            if let Some(r) = ret {
                q = r;
            }
            contours.push(path);
        }
        font.glyphs.insert(code, Glyph { advance, contours });
    }
    Ok(font)
}

// ---- Block-compressed textures to RGBA

const P2: [&[u8; 16]; 64] = [
    b"0011001100110011",
    b"0001000100010001",
    b"0111011101110111",
    b"0001001100110111",
    b"0000000100010011",
    b"0011011101111111",
    b"0001001101111111",
    b"0000000100110111",
    b"0000000000010011",
    b"0011011111111111",
    b"0000000101111111",
    b"0000000000010111",
    b"0001011111111111",
    b"0000000011111111",
    b"0000111111111111",
    b"0000000000001111",
    b"0000100011101111",
    b"0111000100000000",
    b"0000000010001110",
    b"0111001100010000",
    b"0011000100000000",
    b"0000100011001110",
    b"0000000010001100",
    b"0111001100110001",
    b"0011000100010000",
    b"0000100010001100",
    b"0110011001100110",
    b"0011011001101100",
    b"0001011111101000",
    b"0000111111110000",
    b"0111000110001110",
    b"0011100110011100",
    b"0101010101010101",
    b"0000111100001111",
    b"0101101001011010",
    b"0011001111001100",
    b"0011110000111100",
    b"0101010110101010",
    b"0110100101101001",
    b"0101101010100101",
    b"0111001111001110",
    b"0001001111001000",
    b"0011001001001100",
    b"0011101111011100",
    b"0110100110010110",
    b"0011110011000011",
    b"0110011010011001",
    b"0000011001100000",
    b"0100111001000000",
    b"0010011100100000",
    b"0000001001110010",
    b"0000010011100100",
    b"0110110010010011",
    b"0011011011001001",
    b"0110001110011100",
    b"0011100111000110",
    b"0110110011001001",
    b"0110001100111001",
    b"0111111010000001",
    b"0001100011100111",
    b"0000111100110011",
    b"0011001111110000",
    b"0010001011101110",
    b"0100010001110111",
];
const P3: [&[u8; 16]; 64] = [
    b"0011001102212222",
    b"0001001122112221",
    b"0000200122112211",
    b"0222002200110111",
    b"0000000011221122",
    b"0011001100220022",
    b"0022002211111111",
    b"0011001122112211",
    b"0000000011112222",
    b"0000111111112222",
    b"0000111122222222",
    b"0012001200120012",
    b"0112011201120112",
    b"0122012201220122",
    b"0011011211221222",
    b"0011200122002220",
    b"0001001101121122",
    b"0111001120012200",
    b"0000112211221122",
    b"0022002200221111",
    b"0111011102220222",
    b"0001000122212221",
    b"0000001101220122",
    b"0000110022102210",
    b"0122012200110000",
    b"0012001211222222",
    b"0110122112210110",
    b"0000011012211221",
    b"0022110211020022",
    b"0110011020022222",
    b"0011012201220011",
    b"0000200022112221",
    b"0000000211221222",
    b"0222002200120011",
    b"0011001200220222",
    b"0120012001200120",
    b"0000111122220000",
    b"0120120120120120",
    b"0120201212010120",
    b"0011220011220011",
    b"0011112222000011",
    b"0101010122222222",
    b"0000000021212121",
    b"0022112200221122",
    b"0022001100220011",
    b"0220122102201221",
    b"0101222222220101",
    b"0000212121212121",
    b"0101010101012222",
    b"0222011102220111",
    b"0002111200021112",
    b"0000211221122112",
    b"0222011101110222",
    b"0002111211120002",
    b"0110011001102222",
    b"0000000021122112",
    b"0110011022222222",
    b"0022001100110022",
    b"0022112211220022",
    b"0000000000002112",
    b"0002000100020001",
    b"0222122202221222",
    b"0101222222222222",
    b"0111201122012220",
];
const A2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8,
    2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2,
    2, 15, 15, 15, 15, 15, 2, 2, 15,
];
const A3A: [u8; 64] = [
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5,
    15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5,
    10, 8, 13, 15, 12, 3, 3,
];
const A3B: [u8; 64] = [
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6,
    10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
];
const W2: [u32; 4] = [0, 21, 43, 64];
const W3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const W4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];
const MODES: [[u32; 10]; 8] = [
    [3, 4, 0, 0, 4, 0, 1, 0, 3, 0],
    [2, 6, 0, 0, 6, 0, 0, 1, 3, 0],
    [3, 6, 0, 0, 5, 0, 0, 0, 2, 0],
    [2, 6, 0, 0, 7, 0, 1, 0, 2, 0],
    [1, 0, 2, 1, 5, 6, 0, 0, 2, 3],
    [1, 0, 2, 0, 7, 8, 0, 0, 2, 2],
    [1, 0, 0, 0, 7, 7, 1, 0, 4, 0],
    [2, 6, 0, 0, 5, 5, 1, 0, 2, 0],
];

fn weight(bits: u32, i: u32) -> u32 {
    match bits {
        2 => W2[i as usize],
        3 => W3[i as usize],
        _ => W4[i as usize],
    }
}

fn bc7_block(src: &[u8], out: &mut [u8; 64]) {
    let mut w = [0u8; 16];
    w.copy_from_slice(&src[..16]);
    let v = u128::from_le_bytes(w);
    let mut pos = 0u32;
    let mut bits = |n: u32| -> u32 {
        if n == 0 {
            return 0;
        }
        let r = ((v >> pos) & ((1u128 << n) - 1)) as u32;
        pos += n;
        r
    };
    let mut mode = 0usize;
    while mode < 8 && bits(1) == 0 {
        mode += 1;
    }
    if mode == 8 {
        out.fill(0);
        return;
    }
    let [ns, pb, rb, isb, cb, ab, epb, spb, ib, ib2] = MODES[mode];
    let part = bits(pb) as usize;
    let rot = bits(rb);
    let idx_sel = bits(isb);
    let ne = (ns * 2) as usize;
    let mut ep = [[0u32, 0, 0, 255]; 6];
    for c in 0..3 {
        for e in ep.iter_mut().take(ne) {
            e[c] = bits(cb);
        }
    }
    if ab != 0 {
        for e in ep.iter_mut().take(ne) {
            e[3] = bits(ab);
        }
    }
    if epb != 0 {
        for e in ep.iter_mut().take(ne) {
            let p = bits(1);
            for x in e.iter_mut().take(if ab != 0 { 4 } else { 3 }) {
                *x = (*x << 1) | p;
            }
        }
    } else if spb != 0 {
        for s in 0..ns as usize {
            let p = bits(1);
            for i in 0..2 {
                for c in 0..3 {
                    ep[s * 2 + i][c] = (ep[s * 2 + i][c] << 1) | p;
                }
            }
        }
    }
    let cprec = cb + u32::from(epb != 0 || spb != 0);
    let aprec = if ab != 0 { ab + u32::from(epb != 0) } else { 0 };
    for e in ep.iter_mut().take(ne) {
        for x in e.iter_mut().take(3) {
            let v = *x << (8 - cprec);
            *x = (v | (v >> cprec)) & 255;
        }
        if ab != 0 {
            let v = e[3] << (8 - aprec);
            e[3] = (v | (v >> aprec)) & 255;
        }
    }
    let subset = |i: usize| -> usize {
        match ns {
            1 => 0,
            2 => usize::from(P2[part][i] - b'0'),
            _ => usize::from(P3[part][i] - b'0'),
        }
    };
    let anchor = |i: usize| -> bool {
        i == 0
            || match ns {
                2 => i == usize::from(A2[part]),
                3 => i == usize::from(A3A[part]) || i == usize::from(A3B[part]),
                _ => false,
            }
    };
    let mut idx1 = [0u32; 16];
    let mut idx2 = [0u32; 16];
    for (i, x) in idx1.iter_mut().enumerate() {
        *x = bits(if anchor(i) { ib - 1 } else { ib });
    }
    if ib2 != 0 {
        for (i, x) in idx2.iter_mut().enumerate() {
            *x = bits(if i == 0 { ib2 - 1 } else { ib2 });
        }
    }
    for i in 0..16 {
        let s = subset(i);
        let (e0, e1) = (ep[s * 2], ep[s * 2 + 1]);
        let (cw, aw) = if ib2 == 0 {
            let w = weight(ib, idx1[i]);
            (w, w)
        } else if idx_sel != 0 {
            (weight(ib2, idx2[i]), weight(ib, idx1[i]))
        } else {
            (weight(ib, idx1[i]), weight(ib2, idx2[i]))
        };
        let mix = |a: u32, b: u32, w: u32| ((64 - w) * a + w * b + 32) >> 6;
        let (mut r, mut g, mut b) = (
            mix(e0[0], e1[0], cw),
            mix(e0[1], e1[1], cw),
            mix(e0[2], e1[2], cw),
        );
        let mut a = if ab != 0 { mix(e0[3], e1[3], aw) } else { 255 };
        match rot {
            1 => std::mem::swap(&mut a, &mut r),
            2 => std::mem::swap(&mut a, &mut g),
            3 => std::mem::swap(&mut a, &mut b),
            _ => {}
        }
        out[i * 4..i * 4 + 4].copy_from_slice(&[r as u8, g as u8, b as u8, a as u8]);
    }
}

fn c565(c: u16) -> [u32; 3] {
    let (r, g, b) = (
        u32::from(c >> 11) & 31,
        u32::from(c >> 5) & 63,
        u32::from(c) & 31,
    );
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

fn bc1_block(src: &[u8], out: &mut [u8; 64], four: bool) {
    let (c0, c1) = (
        u16::from_le_bytes([src[0], src[1]]),
        u16::from_le_bytes([src[2], src[3]]),
    );
    let (a, b) = (c565(c0), c565(c1));
    let third = |x: u32, y: u32| ((2 * x + y) as f32 / 3. + 0.5) as u32;
    let pal: [[u32; 4]; 4] = if c0 > c1 || four {
        [
            [a[0], a[1], a[2], 255],
            [b[0], b[1], b[2], 255],
            [third(a[0], b[0]), third(a[1], b[1]), third(a[2], b[2]), 255],
            [third(b[0], a[0]), third(b[1], a[1]), third(b[2], a[2]), 255],
        ]
    } else {
        [
            [a[0], a[1], a[2], 255],
            [b[0], b[1], b[2], 255],
            [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2, 255],
            [0, 0, 0, 0],
        ]
    };
    let bits = u32::from_le_bytes([src[4], src[5], src[6], src[7]]);
    for i in 0..16 {
        let p = pal[((bits >> (2 * i)) & 3) as usize];
        out[i * 4..i * 4 + 4].copy_from_slice(&[p[0] as u8, p[1] as u8, p[2] as u8, p[3] as u8]);
    }
}

fn alpha_block(src: &[u8], out: &mut [u8; 64], channel: usize) {
    let (a0, a1) = (u32::from(src[0]), u32::from(src[1]));
    let mut pal = [a0, a1, 0, 0, 0, 0, 0, 0];
    if a0 > a1 {
        for i in 1..7u32 {
            pal[i as usize + 1] = (((7 - i) * a0 + i * a1) as f32 / 7. + 0.5) as u32;
        }
    } else {
        for i in 1..5u32 {
            pal[i as usize + 1] = (((5 - i) * a0 + i * a1) as f32 / 5. + 0.5) as u32;
        }
        pal[7] = 255;
    }
    let mut bits = 0u64;
    for i in 0..6 {
        bits |= u64::from(src[2 + i]) << (8 * i);
    }
    for i in 0..16 {
        out[i * 4 + channel] = pal[((bits >> (3 * i)) & 7) as usize] as u8;
    }
}

/// A cooked mip as straight RGBA8.
pub fn decode_texture(format: &str, w: u32, h: u32, data: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (w as usize, h as usize);
    let mut rgba = vec![0u8; w * h * 4];
    match format {
        "PF_B8G8R8A8" => {
            for (o, s) in rgba.chunks_exact_mut(4).zip(data.chunks_exact(4)) {
                o.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
            }
        }
        "PF_G8" => {
            for (o, s) in rgba.chunks_exact_mut(4).zip(data) {
                o.copy_from_slice(&[*s, *s, *s, 255]);
            }
        }
        "PF_DXT1" | "PF_DXT5" | "PF_BC4" | "PF_BC5" | "PF_BC7" => {
            let bs = if matches!(format, "PF_DXT1" | "PF_BC4") {
                8
            } else {
                16
            };
            let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
            let mut px = [0u8; 64];
            for by in 0..bh {
                for bx in 0..bw {
                    let o = (by * bw + bx) * bs;
                    let src = data.get(o..o + bs)?;
                    match format {
                        "PF_DXT1" => bc1_block(src, &mut px, false),
                        "PF_DXT5" => {
                            bc1_block(&src[8..], &mut px, true);
                            alpha_block(src, &mut px, 3);
                        }
                        "PF_BC4" => {
                            alpha_block(src, &mut px, 0);
                            for i in 0..16 {
                                px[i * 4 + 1] = px[i * 4];
                                px[i * 4 + 2] = px[i * 4];
                                px[i * 4 + 3] = 255;
                            }
                        }
                        "PF_BC5" => {
                            alpha_block(src, &mut px, 0);
                            alpha_block(&src[8..], &mut px, 1);
                            for i in 0..16 {
                                px[i * 4 + 2] = 0;
                                px[i * 4 + 3] = 255;
                            }
                        }
                        _ => bc7_block(src, &mut px),
                    }
                    for y in 0..4 {
                        let yy = by * 4 + y;
                        if yy >= h {
                            break;
                        }
                        for x in 0..4 {
                            let xx = bx * 4 + x;
                            if xx >= w {
                                break;
                            }
                            let t = (yy * w + xx) * 4;
                            let f = (y * 4 + x) * 4;
                            rgba[t..t + 4].copy_from_slice(&px[f..f + 4]);
                        }
                    }
                }
            }
        }
        _ => return None,
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc7_partition_tables_are_consistent() {
        for p in 0..64 {
            assert_eq!(P2[p][0], b'0');
            assert_eq!(P3[p][0], b'0');
            assert_eq!(P2[p][usize::from(A2[p])], b'1', "two-subset anchor {p}");
            assert_eq!(P3[p][usize::from(A3A[p])], b'1', "three-subset anchor {p}");
            assert_eq!(P3[p][usize::from(A3B[p])], b'2', "three-subset anchor {p}");
        }
    }

    #[test]
    fn a_bgra_mip_swaps_its_channels() {
        let out = decode_texture("PF_B8G8R8A8", 1, 1, &[1, 2, 3, 4]).unwrap();
        assert_eq!(out, [3, 2, 1, 4]);
    }

    #[test]
    fn a_format_it_lacks_is_refused() {
        assert!(decode_texture("PF_NOPE", 1, 1, &[0; 4]).is_none());
    }

    #[test]
    fn something_else_is_not_a_movie() {
        assert!(read_movie("x", b"nothing like a movie").is_err());
    }
}
