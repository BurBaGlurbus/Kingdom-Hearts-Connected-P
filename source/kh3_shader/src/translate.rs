//! A compiled pixel shader, instruction by instruction, as a WGSL function over the harness's
//! inputs. The shader's inputs are named by their D3D semantics and constant-buffer slots; nothing
//! is tuned by eye. Ported from StrawHatPro's `tools/sora_vfx_shaders.js` (MIT). What cannot be
//! translated (integer code that reaches the colour, scene-colour reads, dynamic buffer indexing)
//! is an error with the reason, and the material keeps the generic path.

use crate::dxbc::{ImmType, Input, Instruction, Kind, Operand, Program};
use std::collections::{BTreeSet, HashMap, HashSet};

pub const MAX_TEXTURES: u32 = 10;
pub const MAX_CB: u32 = 64;

/// The slots of the fork's View, Primitive and sprite buffers that its pixel shaders read: the
/// harness gives each a value.
const VIEW_SLOTS: [u32; 20] = [
    8, 9, 10, 53, 54, 55, 58, 117, 118, 119, 122, 123, 124, 125, 126, 127, 128, 130, 137, 161,
];
const PRIM_SLOTS: [u32; 5] = [8, 9, 11, 17, 18];
/// The vertex stage's own view of the buffers.
const VS_VIEW_SLOTS: [u32; 18] = [
    8, 9, 10, 11, 12, 13, 14, 15, 32, 33, 34, 35, 48, 55, 56, 58, 128, 161,
];
const VS_PRIM_SLOTS: [u32; 6] = [0, 1, 2, 3, 8, 9];
/// The interpolants the harness carries from a translated vertex shader to the pixel shader.
const VS_OUT: [(&str, &str); 8] = [
    ("TEXCOORD10", "tc10"),
    ("TEXCOORD11", "tc11"),
    ("TEXCOORD0", "tc0"),
    ("TEXCOORD1", "tc1"),
    ("COLOR0", "col0"),
    ("COLOR1", "col1"),
    ("COLOR2", "col2"),
    ("COLOR3", "col3"),
];

fn semantic(name: &str) -> Option<&'static str> {
    Some(match name {
        "TEXCOORD10" => "tc10",
        "TEXCOORD11" => "tc11",
        "TEXCOORD0" => "tc0",
        "TEXCOORD1" => "tc1",
        "TEXCOORD2" => "tc2",
        "TEXCOORD3" => "tc3",
        "TEXCOORD7" => "tc7",
        "COLOR0" => "col0",
        "COLOR1" => "col1",
        "COLOR2" => "col2",
        "COLOR3" => "col3",
        "PARTICLE_TIME0" => "ptime",
        "PARTICLE_POSITION0" => "ppos",
        "PARTICLE_SUBUVS0" => "psubuv",
        "SV_Position0" | "SV_POSITION0" => "svpos",
        "SV_IsFrontFace0" | "SV_ISFRONTFACE0" => "front",
        "PSIZE0" => "psize",
        "PARTICLE_TRANSFORM0" => "ptr0",
        "PARTICLE_TRANSFORM1" => "ptr1",
        "PARTICLE_TRANSFORM2" => "ptr2",
        _ => return None,
    })
}

/// What the translator is told of a shader beyond its code.
#[derive(Clone, Debug, Default)]
pub struct Meta {
    /// Uniform buffer names by constant-buffer slot.
    pub buffers: Vec<String>,
    /// Texture binding to the material's 2D texture.
    pub textures: HashMap<u32, u32>,
    /// Sampler binding: `Tex(n)` a texture's own, or a shared one.
    pub samplers: HashMap<u32, SamplerOf>,
    /// Semantics that a vertex factory names differently.
    pub semantics: HashMap<String, &'static str>,
    /// Only the clip of a masked material: nothing goes out.
    pub mask_only: bool,
    /// `FDeferredDecalPS`: its loose constants are the decal's.
    pub decal: bool,
    /// The distortion pass: its only loose constant is DistortionParams.
    pub distort: bool,
    /// A vertex shader: inputs are vertex attributes, the last step is the harness's.
    pub vertex: bool,
    /// Material buffer rows the harness fills per draw: row to WGSL text.
    pub rows: HashMap<u32, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplerOf {
    Tex(u32),
    Wrap,
    Clamp,
    Beyond,
}

#[derive(Clone, Debug, Default)]
pub struct Translated {
    pub wgsl: String,
    pub outputs: usize,
    pub textures: BTreeSet<u32>,
    pub cb_len: u32,
    pub uses_time: bool,
}

/// The material's resources from the shader's resource table: entries of the Material buffer are
/// `buffer << 24 | resource << 8 | binding`.
pub fn resource_tables(
    buffers: &[String],
    textures: &[u32],
    samplers: &[u32],
    n2d: u32,
    ncube: u32,
) -> (HashMap<u32, u32>, HashMap<u32, SamplerOf>) {
    let Some(mat) = buffers.iter().position(|b| b == "Material") else {
        return Default::default();
    };
    let decode = |arr: &[u32]| {
        let mut out = Vec::new();
        let Some(&off) = arr.get(mat) else { return out };
        let off = off as usize;
        if off == 0 || off >= arr.len() {
            return out;
        }
        for &e in &arr[off..] {
            if e == u32::MAX || (e >> 24) as usize != mat {
                break;
            }
            out.push((e & 0xff, (e >> 8) & 0xffff));
        }
        out
    };
    let mut tex = HashMap::new();
    for (bind, res) in decode(textures) {
        tex.insert(
            bind,
            if res < 2 * n2d && res % 2 == 0 {
                res >> 1
            } else {
                1000 + res
            },
        );
    }
    let mut samp = HashMap::new();
    for (bind, res) in decode(samplers) {
        let s = if res < 2 * n2d {
            SamplerOf::Tex(res >> 1)
        } else {
            match res.wrapping_sub(2 * (n2d + ncube)) {
                0 => SamplerOf::Wrap,
                1 => SamplerOf::Clamp,
                _ => SamplerOf::Beyond,
            }
        };
        samp.insert(bind, s);
    }
    (tex, samp)
}

/// A literal's value as a float: what its printed form reads as. `None` when it is bits that only
/// an integer instruction could mean.
fn literal(bits: u32, kind: ImmType) -> Option<f32> {
    let int = bits as i32;
    let float = f32::from_bits(bits);
    let small = (-10000..=10000).contains(&int);
    let from_bits = || {
        if !float.is_finite() || (float != 0. && float.abs() < 1e-30) {
            None
        } else {
            Some(float)
        }
    };
    match kind {
        ImmType::Float => {
            if float.is_infinite() {
                Some(if float > 0. { 1e30 } else { -1e30 })
            } else if float.is_nan() {
                None
            } else {
                Some(float)
            }
        }
        ImmType::Int => {
            if small {
                Some(int as f32)
            } else {
                from_bits()
            }
        }
        ImmType::Any => {
            if small {
                Some(int as f32)
            } else {
                from_bits()
            }
        }
    }
}

fn text_of(v: f32) -> String {
    let t = if v == v.trunc() && v.abs() < 1e7 {
        format!("{v:.1}")
    } else {
        format!("{v:e}")
    };
    if v < 0. || t.starts_with('-') {
        format!("({t})")
    } else {
        t
    }
}

fn index_of(c: char) -> usize {
    match c {
        'x' => 0,
        'y' => 1,
        'z' => 2,
        _ => 3,
    }
}

fn comp_at(swz: &str, k: usize) -> char {
    let s: Vec<char> = swz.chars().collect();
    if s.len() == 1 {
        s[0]
    } else {
        s.get(k).or(s.last()).copied().unwrap_or('x')
    }
}

/// A statement kept for the body: an instruction, or text the idioms stand in with.
enum Stmt {
    Ins(Instruction),
    Raw {
        text: String,
        dest: String,
        src: Vec<String>,
    },
}

struct Gen<'a> {
    meta: &'a Meta,
    inputs: HashMap<u32, Input>,
    why: Option<String>,
    textures: BTreeSet<u32>,
    uses_time: bool,
    max_cb: i64,
    masks: HashSet<String>,
    loops: u32,
    distort_row: Option<u32>,
    rows_used: BTreeSet<u32>,
    /// Samples of a decal whose result is unpacked as a normal.
    normal_samples: HashSet<usize>,
    at: usize,
}

impl Gen<'_> {
    fn bad(&mut self, why: String) -> String {
        if self.why.is_none() {
            self.why = Some(why);
        }
        "0.0".to_owned()
    }

    fn buffer_name(&self, cb: u32) -> &str {
        self.meta
            .buffers
            .get(cb as usize)
            .map_or("?", String::as_str)
    }

    fn base(&mut self, o: &Operand) -> String {
        match o.kind {
            Kind::Temp => format!("r{}", o.idx),
            Kind::Output => format!("o{}", o.idx),
            Kind::Input => {
                let Some(s) = self.inputs.get(&o.idx).cloned() else {
                    return self.bad(format!("input v{}", o.idx));
                };
                if self.meta.vertex {
                    return match s.name.strip_prefix("ATTRIBUTE") {
                        Some(n) if s.float => format!("i.a{n}"),
                        _ => self.bad(format!("vertex input {}", s.name)),
                    };
                }
                if s.name.eq_ignore_ascii_case("TEXCOORD9") {
                    return "(vec4<f32>(i.twp, 1.0))".to_owned();
                }
                match self
                    .meta
                    .semantics
                    .get(&s.name)
                    .copied()
                    .or_else(|| semantic(&s.name))
                {
                    Some(f) => format!("i.{f}"),
                    None => self.bad(format!("input {}", s.name)),
                }
            }
            Kind::Buffer => {
                let ub = self.buffer_name(o.cb).to_owned();
                if self.meta.vertex && ub != "Material" {
                    return match ub.as_str() {
                        "View" if VS_VIEW_SLOTS.contains(&o.idx) => format!("i.v{}", o.idx),
                        "Primitive" if VS_PRIM_SLOTS.contains(&o.idx) => format!("i.p{}", o.idx),
                        "MeshParticleVF" | "FMeshParticleUniformParameters" if o.idx <= 5 => {
                            format!("i.mp{}", o.idx)
                        }
                        other => self.bad(format!("vertex buffer {other}[{}]", o.idx)),
                    };
                }
                if ub.is_empty() && self.meta.distort {
                    return match self.distort_row {
                        Some(row) if row != o.idx => {
                            self.bad("a second constant in the distortion shader".to_owned())
                        }
                        _ => {
                            self.distort_row = Some(o.idx);
                            "i.dist12".to_owned()
                        }
                    };
                }
                if ub.is_empty() && self.meta.decal {
                    return match o.idx {
                        12 => "i.d12".to_owned(),
                        23 => "i.d23".to_owned(),
                        row => self.bad(format!("decal buffer[{row}]")),
                    };
                }
                match ub.as_str() {
                    "Material" => {
                        if o.idx >= MAX_CB {
                            return self.bad("material constant buffer too large".to_owned());
                        }
                        self.max_cb = self.max_cb.max(i64::from(o.idx));
                        if self.meta.rows.contains_key(&o.idx) {
                            self.rows_used.insert(o.idx);
                            return format!("m{}", o.idx);
                        }
                        format!("kh.m[{}]", o.idx)
                    }
                    "View" => {
                        if !VIEW_SLOTS.contains(&o.idx) {
                            return self.bad(format!("View[{}]", o.idx));
                        }
                        if o.idx == 128 {
                            self.uses_time = true;
                        }
                        format!("i.v{}", o.idx)
                    }
                    "Primitive" if o.idx == 10 => {
                        let has_position =
                            self.inputs.values().any(|s| s.name == "PARTICLE_POSITION0");
                        if has_position {
                            if o.swz.contains('w') {
                                return self.bad("Primitive[10].w".to_owned());
                            }
                            "(i.ppos - i.v58)".to_owned()
                        } else {
                            "i.p8".to_owned()
                        }
                    }
                    "Primitive" => {
                        if !PRIM_SLOTS.contains(&o.idx) {
                            return self.bad(format!("Primitive[{}]", o.idx));
                        }
                        format!("i.p{}", o.idx)
                    }
                    "SpriteVF" | "FParticleSpriteUniformParameters" => {
                        if o.idx != 10 {
                            return self.bad(format!("sprite buffer[{}]", o.idx));
                        }
                        "i.s10".to_owned()
                    }
                    "PrecomputedLightingBuffer" | "FPrecomputedLightingParameters" => {
                        if o.idx != 5 {
                            return self.bad(format!("precomputed lighting[{}]", o.idx));
                        }
                        "i.l5".to_owned()
                    }
                    other => self.bad(format!("uniform buffer {other}")),
                }
            }
            kind => self.bad(format!("operand {kind:?}")),
        }
    }

    fn wrap(o: &Operand, mut e: String) -> String {
        if o.abs {
            e = format!("abs({e})");
        }
        if o.neg {
            e = format!("(-{e})");
        }
        e
    }

    /// The operand for the destination components `comps`.
    fn vec(&mut self, o: &Operand, comps: &[usize]) -> String {
        if o.kind == Kind::Literal {
            let mut v = Vec::new();
            for &k in comps {
                let bits = if o.imm.len() == 1 { o.imm[0] } else { o.imm[k] };
                match literal(bits, o.imm_type) {
                    Some(f) => v.push(text_of(f)),
                    None => return self.bad("integer immediate".to_owned()),
                }
            }
            let e = if v.len() == 1 {
                v.remove(0)
            } else {
                format!("vec{}<f32>({})", v.len(), v.join(", "))
            };
            return Self::wrap(o, e);
        }
        let swz: String = comps.iter().map(|&k| comp_at(&o.swz, k)).collect();
        if o.kind == Kind::Buffer
            && self.buffer_name(o.cb) == "View"
            && o.idx == 161
            && swz.contains('z')
        {
            self.uses_time = true;
        }
        let b = self.base(o);
        Self::wrap(o, format!("{b}.{swz}"))
    }

    fn first(&mut self, o: &Operand, n: usize) -> String {
        let comps: Vec<usize> = (0..n).collect();
        self.vec(o, &comps)
    }

    fn assign(&mut self, o: &Operand, expr: String, n: usize, sat: bool) -> String {
        let d = match o.kind {
            Kind::Temp => format!("r{}", o.idx),
            Kind::Output => format!("o{}", o.idx),
            kind => return self.bad(format!("destination {kind:?}")),
        };
        let expr = if sat {
            format!("saturate({expr})")
        } else {
            expr
        };
        let comps: Vec<char> = o.swz.chars().collect();
        if comps.len() == 1 {
            let tail = if n > 1 { ".x" } else { "" };
            return format!("{d}.{} = {expr}{tail};", comps[0]);
        }
        if n == 1 {
            let each: Vec<String> = comps.iter().map(|c| format!("{d}.{c} = t;")).collect();
            return format!("{{ let t = {expr}; {} }}", each.join(" "));
        }
        if o.swz == "xyzw" {
            return format!("{d} = {expr};");
        }
        let each: Vec<String> = comps
            .iter()
            .enumerate()
            .map(|(k, c)| format!("{d}.{c} = t.{};", ['x', 'y', 'z', 'w'][k]))
            .collect();
        format!("{{ let t = {expr}; {} }}", each.join(" "))
    }

    fn texel(
        &mut self,
        t_op: &Operand,
        s_op: Option<&Operand>,
        uv: &str,
        lod: &str,
        grad: Option<(String, String)>,
    ) -> Option<String> {
        let t = *self.meta.textures.get(&t_op.idx)?;
        if t >= MAX_TEXTURES {
            self.bad(format!("more than {MAX_TEXTURES} textures"));
            return Some("vec4<f32>(0.0)".to_owned());
        }
        let sampler = match s_op.and_then(|s| self.meta.samplers.get(&s.idx)).copied() {
            None => format!("kh_s{t}"),
            Some(SamplerOf::Wrap) => "kh_swrap".to_owned(),
            Some(SamplerOf::Clamp) => "kh_sclamp".to_owned(),
            Some(SamplerOf::Tex(n)) if n < MAX_TEXTURES => format!("kh_s{n}"),
            Some(_) => {
                self.bad("sampler of a texture beyond the slots".to_owned());
                "kh_s0".to_owned()
            }
        };
        self.textures.insert(t);
        if let Some(sampler_n) = sampler
            .strip_prefix("kh_s")
            .and_then(|n| n.parse::<u32>().ok())
        {
            self.textures.insert(sampler_n);
        }
        Some(match grad {
            Some((x, y)) => format!("textureSampleGrad(kh_t{t}, {sampler}, {uv}, {x}, {y})"),
            None => format!("textureSampleLevel(kh_t{t}, {sampler}, {uv}, {lod})"),
        })
    }

    fn source_masks(&self, o: &Operand, comps: &[usize]) -> Vec<bool> {
        if o.kind != Kind::Temp {
            return vec![false; comps.len()];
        }
        comps
            .iter()
            .map(|&c| {
                self.masks
                    .contains(&format!("r{}.{}", o.idx, comp_at(&o.swz, c)))
            })
            .collect()
    }

    fn generate(&mut self, n: &Instruction) -> String {
        let a = &n.a;
        match n.op.as_str() {
            "ret" => return String::new(),
            "else" => return "} else {".to_owned(),
            "endif" => return "}".to_owned(),
            "if_nz" => return format!("if ({} != 0.0) {{", self.vec(&a[0], &[0])),
            "if_z" => return format!("if ({} == 0.0) {{", self.vec(&a[0], &[0])),
            "discard_nz" => return format!("if ({} != 0.0) {{ discard; }}", self.vec(&a[0], &[0])),
            "discard_z" => return format!("if ({} == 0.0) {{ discard; }}", self.vec(&a[0], &[0])),
            "loop" => {
                self.loops += 1;
                return "loop {".to_owned();
            }
            "endloop" => {
                self.loops -= 1;
                return "}".to_owned();
            }
            "breakc_nz" => return format!("if ({} != 0.0) {{ break; }}", self.vec(&a[0], &[0])),
            "breakc_z" => return format!("if ({} == 0.0) {{ break; }}", self.vec(&a[0], &[0])),
            "break" => return "break;".to_owned(),
            "sincos" => {
                let mut parts = Vec::new();
                for (k, f) in [(0, "sin"), (1, "cos")] {
                    if a[k].kind != Kind::Null {
                        let comps: Vec<usize> = a[k].swz.chars().map(index_of).collect();
                        let v = self.vec(&a[2], &comps);
                        let dest = a[k].clone();
                        parts.push(self.assign(&dest, format!("{f}({v})"), comps.len(), n.sat));
                    }
                }
                return parts.join(" ");
            }
            _ => {}
        }
        let Some(d) = a.first().cloned() else {
            return self.bad(format!("instruction {}", n.op));
        };
        if d.kind != Kind::Temp && d.kind != Kind::Output {
            return self.bad(format!("instruction {}", n.op));
        }
        let comps: Vec<usize> = d.swz.chars().map(index_of).collect();
        let k = comps.len();
        let zero = |n: usize| {
            if n == 1 {
                "0.0".to_owned()
            } else {
                format!("vec{n}<f32>(0.0)")
            }
        };
        let one = |n: usize| {
            if n == 1 {
                "1.0".to_owned()
            } else {
                format!("vec{n}<f32>(1.0)")
            }
        };
        let dest_keys: Vec<String> = if d.kind == Kind::Temp {
            d.swz.chars().map(|c| format!("r{}.{c}", d.idx)).collect()
        } else {
            Vec::new()
        };
        let cmp = match n.op.as_str() {
            "lt" | "ilt" | "ult" => Some("<"),
            "ge" | "ige" | "uge" => Some(">="),
            "eq" | "ieq" => Some("=="),
            "ne" | "ine" => Some("!="),
            _ => None,
        };
        let bit_mask = (n.op == "and" || n.op == "or")
            && self.source_masks(&a[1], &comps).iter().all(|m| *m)
            && self.source_masks(&a[2], &comps).iter().all(|m| *m);
        if n.op == "itof" {
            let m = self.source_masks(&a[1], &comps);
            for key in &dest_keys {
                self.masks.remove(key);
            }
            if m.iter().all(|x| *x) {
                let v = self.vec(&a[1], &comps);
                return self.assign(&d, format!("(-{v})"), k, false);
            }
            if m.iter().any(|x| *x) {
                return self.bad("itof of a mixed operand".to_owned());
            }
            let v = self.vec(&a[1], &comps);
            return self.assign(&d, v, k, false);
        }
        if n.op == "utof" && self.source_masks(&a[1], &comps).iter().any(|x| *x) {
            return self.bad("utof of a comparison".to_owned());
        }
        if n.op == "ishl" {
            for key in &dest_keys {
                self.masks.remove(key);
            }
            let shift = (a[2].kind == Kind::Literal)
                .then(|| a[2].imm.first().copied())
                .flatten()
                .filter(|s| *s <= 16);
            let Some(shift) = shift else {
                return self.bad("ishl by a register".to_owned());
            };
            let v = self.vec(&a[1], &comps);
            return self.assign(&d, format!("({v} * {}.0)", 1u32 << shift), k, false);
        }
        for key in &dest_keys {
            if cmp.is_some() || bit_mask {
                self.masks.insert(key.clone());
            } else {
                self.masks.remove(key);
            }
        }
        let unary = |op: &str, v: &str| -> Option<String> {
            Some(match op {
                "mov" | "utof" => v.to_owned(),
                "sqrt" => format!("sqrt({v})"),
                "rsq" => format!("inverseSqrt({v})"),
                "rcp" => format!("(1.0 / {v})"),
                "log" => format!("log2({v})"),
                "exp" => format!("exp2({v})"),
                "frc" => format!("fract({v})"),
                "round_ni" => format!("floor({v})"),
                "round_pi" => format!("ceil({v})"),
                "round_z" | "ftoi" | "ftou" => format!("trunc({v})"),
                "round_ne" => format!("round({v})"),
                "deriv_rtx" => format!("dpdx({v})"),
                "deriv_rty" => format!("dpdy({v})"),
                "deriv_rtx_coarse" => format!("dpdxCoarse({v})"),
                "deriv_rty_coarse" => format!("dpdyCoarse({v})"),
                "deriv_rtx_fine" => format!("dpdxFine({v})"),
                "deriv_rty_fine" => format!("dpdyFine({v})"),
                _ => return None,
            })
        };
        if a.len() >= 2 && unary(&n.op, "").is_some() {
            let v = self.vec(&a[1], &comps);
            let e = unary(&n.op, &v).unwrap_or_default();
            return self.assign(&d, e, k, n.sat);
        }
        let binary = |op: &str, x: &str, y: &str| -> Option<String> {
            Some(match op {
                "iadd" | "add" => format!("({x} + {y})"),
                "imul" | "mul" | "and" => format!("({x} * {y})"),
                "div" => format!("({x} / {y})"),
                "min" => format!("min({x}, {y})"),
                "max" | "or" => format!("max({x}, {y})"),
                _ => return None,
            })
        };
        if a.len() >= 3 && binary(&n.op, "", "").is_some() {
            let x = self.vec(&a[1], &comps);
            let y = self.vec(&a[2], &comps);
            let e = binary(&n.op, &x, &y).unwrap_or_default();
            return self.assign(&d, e, k, n.sat);
        }
        if n.op == "mad" {
            let (x, y, z) = (
                self.vec(&a[1], &comps),
                self.vec(&a[2], &comps),
                self.vec(&a[3], &comps),
            );
            return self.assign(&d, format!("({x} * {y} + {z})"), k, n.sat);
        }
        if let Some(c) = cmp {
            let (x, y) = (self.vec(&a[1], &comps), self.vec(&a[2], &comps));
            let e = format!("select({}, {}, {x} {c} {y})", zero(k), one(k));
            return self.assign(&d, e, k, false);
        }
        if n.op == "movc" {
            let (c, x, y) = (
                self.vec(&a[1], &comps),
                self.vec(&a[2], &comps),
                self.vec(&a[3], &comps),
            );
            let e = format!("select({y}, {x}, {c} != {})", zero(k));
            return self.assign(&d, e, k, n.sat);
        }
        if let Some(m) =
            n.op.strip_prefix("dp")
                .and_then(|m| m.parse::<usize>().ok())
        {
            let (x, y) = (self.first(&a[1], m), self.first(&a[2], m));
            return self.assign(&d, format!("dot({x}, {y})"), 1, n.sat);
        }
        let swizzled =
            |t: &Operand| -> String { comps.iter().map(|&c| comp_at(&t.swz, c)).collect() };
        match n.op.as_str() {
            "sample" | "sample_indexable" | "sample_l" | "sample_l_indexable"
            | "sample_b_indexable" | "sample_b" => {
                if n.tex_type.is_some_and(|t| t != "texture2d") {
                    return self.bad(format!("texture type {:?}", n.tex_type));
                }
                let is_l = n.op.starts_with("sample_l");
                let lod = if is_l {
                    self.vec(&a[4], &[0])
                } else {
                    "0.0".to_owned()
                };
                let uv = self.first(&a[1], 2);
                let sw = swizzled(&a[2]);
                let s = match self.texel(&a[2], Some(&a[3]), &uv, &lod, None) {
                    Some(s) => s,
                    None => {
                        // Not a material texture: the scene depth, always read with sample_l.
                        if !is_l && self.meta.decal {
                            // GBufferA: the receiving surface's normal, taken as a floor.
                            return self.assign(
                                &d,
                                format!("vec4<f32>(0.5, 0.5, 1.0, 1.0).{sw}"),
                                k,
                                n.sat,
                            );
                        }
                        if !is_l {
                            return self.bad("scene texture read".to_owned());
                        }
                        if self.meta.decal {
                            // The receiver mask (0 = receives), or the packed normal.
                            let s = if self.normal_samples.contains(&self.at) {
                                "vec4<f32>(0.5, 0.5, 1.0, 1.0)"
                            } else {
                                "vec4<f32>(0.0)"
                            };
                            return self.assign(&d, format!("{s}.{sw}"), k, n.sat);
                        }
                        if self.loops > 0 {
                            return self.bad("scene colour read in a loop (blur)".to_owned());
                        }
                        "vec4<f32>(i.inv_scene_depth)".to_owned()
                    }
                };
                self.assign(&d, format!("{s}.{sw}"), k, n.sat)
            }
            "sample_d" | "sample_d_indexable" => {
                if n.tex_type.is_some_and(|t| t != "texture2d") {
                    return self.bad(format!("texture type {:?}", n.tex_type));
                }
                let uv = self.first(&a[1], 2);
                let grad = (self.first(&a[4], 2), self.first(&a[5], 2));
                let sw = swizzled(&a[2]);
                match self.texel(&a[2], Some(&a[3]), &uv, "0.0", Some(grad)) {
                    Some(s) => self.assign(&d, format!("{s}.{sw}"), k, n.sat),
                    None => self.bad("sample_d of a scene texture".to_owned()),
                }
            }
            "resinfo_indexable" => {
                if n.tex_type.is_some_and(|t| t != "texture2d") {
                    return self.bad(format!("texture type {:?}", n.tex_type));
                }
                let Some(&t) = self
                    .meta
                    .textures
                    .get(&a[2].idx)
                    .filter(|t| **t < MAX_TEXTURES)
                else {
                    return self.bad("resinfo of a scene texture".to_owned());
                };
                self.textures.insert(t);
                if a[1].kind != Kind::Literal {
                    return self.bad("resinfo of a register mip".to_owned());
                }
                let lod = a[1].imm.first().copied().unwrap_or(0);
                let all = format!(
                    "vec4<f32>(vec2<f32>(textureDimensions(kh_t{t}, {lod}u)), 0.0, f32(textureNumLevels(kh_t{t})))"
                );
                let sw = swizzled(&a[2]);
                self.assign(&d, format!("{all}.{sw}"), k, false)
            }
            "ld_indexable" | "ld" => {
                if self.meta.textures.contains_key(&a[2].idx) {
                    return self.bad("texel load of a material texture".to_owned());
                }
                self.assign(&d, one(k), k, false)
            }
            other => self.bad(format!("instruction {other}")),
        }
    }
}

fn op_of(s: &Stmt) -> &str {
    match s {
        Stmt::Ins(n) => &n.op,
        Stmt::Raw { .. } => "",
    }
}

fn is_control(s: &Stmt) -> bool {
    let op = op_of(s);
    op.starts_with("if_")
        || op == "else"
        || op == "endif"
        || op == "ret"
        || op.starts_with("discard")
        || op == "loop"
        || op == "endloop"
        || op.starts_with("breakc_")
        || op == "break"
}

fn key_of(o: &Operand, c: char) -> String {
    let k = match o.kind {
        Kind::Temp => 'r',
        _ => 'o',
    };
    format!("{k}{}.{c}", o.idx)
}

fn writes(s: &Stmt) -> Vec<String> {
    match s {
        Stmt::Raw { dest, .. } => ['x', 'y', 'z', 'w']
            .iter()
            .map(|c| format!("{dest}.{c}"))
            .collect(),
        Stmt::Ins(n) => {
            if is_control(s) {
                return Vec::new();
            }
            let dests = if n.op == "sincos" {
                &n.a[..n.a.len().min(2)]
            } else {
                &n.a[..n.a.len().min(1)]
            };
            dests
                .iter()
                .filter(|o| matches!(o.kind, Kind::Temp | Kind::Output))
                .flat_map(|o| o.swz.chars().map(|c| key_of(o, c)).collect::<Vec<_>>())
                .collect()
        }
    }
}

fn reads(s: &Stmt) -> Vec<String> {
    let n = match s {
        Stmt::Ins(n) => n,
        Stmt::Raw { src, .. } => return src.clone(),
    };
    let mut res = Vec::new();
    let mut add = |o: Option<&Operand>, comps: &[usize]| {
        if let Some(o) = o
            && matches!(o.kind, Kind::Temp | Kind::Output)
        {
            for &k in comps {
                res.push(key_of(o, comp_at(&o.swz, k)));
            }
        }
    };
    let op = n.op.as_str();
    if op.starts_with("if_") || op.starts_with("discard") || op.starts_with("breakc_") {
        add(n.a.first(), &[0]);
    } else if is_control(s) || op.starts_with("resinfo") || op.starts_with("ld") {
    } else if let Some(m) = op.strip_prefix("dp").and_then(|m| m.parse::<usize>().ok()) {
        let all: Vec<usize> = (0..m).collect();
        add(n.a.get(1), &all);
        add(n.a.get(2), &all);
    } else if op.starts_with("sample_d") {
        add(n.a.get(1), &[0, 1]);
        add(n.a.get(4), &[0, 1]);
        add(n.a.get(5), &[0, 1]);
    } else if op.starts_with("sample") {
        add(n.a.get(1), &[0, 1]);
        add(n.a.get(4), &[0]);
    } else if op == "sincos" {
        for d in n.a.iter().take(2) {
            if d.kind != Kind::Null {
                let comps: Vec<usize> = d.swz.chars().map(index_of).collect();
                add(n.a.get(2), &comps);
            }
        }
    } else if let Some(d) = n.a.first() {
        let comps: Vec<usize> = d.swz.chars().map(index_of).collect();
        for o in &n.a[1..] {
            add(Some(o), &comps);
        }
    }
    res
}

struct Live<'a> {
    out: &'a [Stmt],
    needed: Vec<bool>,
    ifs: HashMap<usize, (Option<usize>, usize)>,
    if_of_end: HashMap<usize, usize>,
    loop_of_end: HashMap<usize, usize>,
    exits: Vec<HashSet<String>>,
}

impl Live<'_> {
    /// `out[from..to)` is a block; returns what is live before it.
    fn walk(&mut self, from: usize, to: usize, live_out: &HashSet<String>) -> HashSet<String> {
        let mut live = live_out.clone();
        let mut x = to;
        while x > from {
            x -= 1;
            let op = op_of(&self.out[x]).to_owned();
            if op == "endif" {
                let if_at = self.if_of_end[&x];
                let (else_at, end_at) = self.ifs[&if_at];
                let then_live = self.walk(if_at + 1, else_at.unwrap_or(end_at), &live);
                let else_live = match else_at {
                    Some(e) => self.walk(e + 1, end_at, &live),
                    None => live.clone(),
                };
                let any = (if_at + 1..x).any(|y| self.needed[y] && op_of(&self.out[y]) != "else");
                if any {
                    live = then_live.into_iter().chain(else_live).collect();
                    live.extend(reads(&self.out[if_at]));
                    self.needed[x] = true;
                    self.needed[if_at] = true;
                    if let Some(e) = else_at {
                        self.needed[e] = true;
                    }
                }
                x = if_at;
                continue;
            }
            if op == "endloop" {
                let loop_at = self.loop_of_end[&x];
                self.exits.push(live.clone());
                let mut entry = HashSet::new();
                for _ in 0..8 {
                    let next = self.walk(loop_at + 1, x, &entry);
                    let before = entry.len();
                    entry.extend(next);
                    if entry.len() == before {
                        break;
                    }
                }
                self.exits.pop();
                live.extend(entry);
                self.needed[x] = true;
                self.needed[loop_at] = true;
                x = loop_at;
                continue;
            }
            if op == "break" {
                self.needed[x] = true;
                live = self.exits.last().cloned().unwrap_or_default();
                continue;
            }
            if op.starts_with("breakc_") {
                self.needed[x] = true;
                live.extend(reads(&self.out[x]));
                if let Some(e) = self.exits.last() {
                    live.extend(e.iter().cloned());
                }
                continue;
            }
            if is_control(&self.out[x]) {
                self.needed[x] = true;
                live.extend(reads(&self.out[x]));
                continue;
            }
            let w = writes(&self.out[x]);
            if w.iter().any(|r| live.contains(r)) {
                self.needed[x] = true;
                for r in &w {
                    live.remove(r);
                }
                live.extend(reads(&self.out[x]));
            }
        }
        live
    }
}

/// The fork's depth-offset block of the mesh-particle vertex shader is gated by MeshParticleVF[5].x
/// through conditional moves. The evaluator applies the offset itself, so the constant is 0 here:
/// comparisons of constants fold and a `movc` on a folded condition becomes a `mov`.
fn fold_depth_offset(ins: &mut [Instruction], meta: &Meta) {
    let mut known: HashMap<String, bool> = HashMap::new();
    for n in ins.iter_mut() {
        if n.op.starts_with("if_")
            || matches!(n.op.as_str(), "else" | "endif" | "loop" | "endloop")
            || n.op.starts_with("breakc")
        {
            known.clear();
            continue;
        }
        let Some(d) = n.a.first().cloned().filter(|d| d.kind == Kind::Temp) else {
            continue;
        };
        if n.op == "movc" && n.a[1].kind == Kind::Temp {
            let c = n.a[1].clone();
            let vals: Vec<Option<bool>> = d
                .swz
                .chars()
                .map(|ch| {
                    known
                        .get(&format!("r{}.{}", c.idx, comp_at(&c.swz, index_of(ch))))
                        .copied()
                })
                .collect();
            if let Some(Some(first)) = vals.first().copied()
                && vals.iter().all(|v| *v == Some(first))
            {
                let source = if first {
                    n.a[2].clone()
                } else {
                    n.a[3].clone()
                };
                n.op = "mov".to_owned();
                n.a = vec![d.clone(), source];
            }
        }
        for ch in d.swz.chars() {
            known.remove(&format!("r{}.{ch}", d.idx));
        }
        if matches!(n.op.as_str(), "lt" | "ne" | "ge" | "eq") && d.swz.len() == 1 {
            let value = |o: &Operand| -> Option<f32> {
                if o.kind == Kind::Literal {
                    return literal(*o.imm.first()?, o.imm_type);
                }
                let name = meta.buffers.get(o.cb as usize)?;
                (o.kind == Kind::Buffer
                    && (name == "MeshParticleVF" || name == "FMeshParticleUniformParameters")
                    && o.idx == 5
                    && o.swz.chars().all(|c| c == 'x'))
                .then_some(0.)
            };
            if let (Some(x), Some(y)) = (value(&n.a[1]), value(&n.a[2])) {
                let holds = match n.op.as_str() {
                    "lt" => x < y,
                    "ne" => x != y,
                    "ge" => x >= y,
                    _ => x == y,
                };
                known.insert(format!("r{}.{}", d.idx, d.swz), holds);
            }
        }
    }
}

/// Translates one shader.
pub fn pixel(program: &Program, meta: &Meta) -> Result<Translated, String> {
    let inputs: HashMap<u32, Input> = program.inputs.iter().cloned().collect();
    let sv_reg = inputs
        .iter()
        .find(|(_, s)| s.name.eq_ignore_ascii_case("SV_Position0"))
        .map(|(r, _)| *r);
    let buffer = |o: &Operand| meta.buffers.get(o.cb as usize).map_or("?", String::as_str);
    // ---- the two view constants that gate whole blocks, and the SvPosition -> world idiom
    let const_val = |o: &Operand| -> Option<f32> {
        if o.kind == Kind::Literal && !o.imm.is_empty() {
            let v = literal(o.imm[0], o.imm_type)?;
            return o
                .imm
                .iter()
                .all(|b| literal(*b, o.imm_type) == Some(v))
                .then_some(v);
        }
        if o.kind == Kind::Buffer && buffer(o) == "View" {
            let c = o.swz.chars().next()?;
            if o.swz.chars().all(|x| x == c) {
                return match (o.idx, c) {
                    (126, 'x') => Some(0.),
                    (161, 'x') => Some(1.),
                    _ => None,
                };
            }
        }
        None
    };
    let mut ins = program.ins.clone();
    if meta.vertex {
        if program.packed_outputs {
            return Err("two outputs share a register".to_owned());
        }
        fold_depth_offset(&mut ins, meta);
    }
    let mut out: Vec<Stmt> = Vec::new();
    let mut saw_clip = false;
    // Which decal samples are unpacked as normals, `mad r, r, l(2), l(-1)` right after them.
    let mut normal_at: HashSet<usize> = HashSet::new();
    let mut known: Option<(u32, String, f32, usize)> = None;
    let mut last_is: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < ins.len() {
        let n = ins[i].clone();
        let is_if = n.op == "if_nz" || n.op == "if_z";
        if is_if
            && let Some((reg, comp, value, at)) = known.clone()
            && n.a[0].kind == Kind::Temp
            && n.a[0].idx == reg
            && n.a[0].swz == comp
        {
            let taken = (n.op == "if_nz") == (value != 0.);
            let (mut depth, mut else_at, mut end_at) = (0, None, None);
            for (j, m) in ins.iter().enumerate().skip(i + 1) {
                match m.op.as_str() {
                    "if_nz" | "if_z" => depth += 1,
                    "endif" => {
                        if depth == 0 {
                            end_at = Some(j);
                            break;
                        }
                        depth -= 1;
                    }
                    "else" if depth == 0 => else_at = Some(j),
                    _ => {}
                }
            }
            let end_at = end_at.ok_or("unbalanced if")?;
            let (from, to) = if taken {
                (i + 1, else_at.unwrap_or(end_at))
            } else {
                (else_at.map_or(end_at, |e| e + 1), end_at)
            };
            let keep: Vec<Instruction> = ins[from..to].to_vec();
            ins.splice(i..=end_at, keep);
            if last_is.last() == Some(&at) {
                out.pop();
                last_is.pop();
            }
            known = None;
            continue;
        }
        known = None;
        if matches!(n.op.as_str(), "lt" | "ne" | "ge" | "eq")
            && n.a[0].kind == Kind::Temp
            && n.a[0].swz.len() == 1
            && let (Some(x), Some(y)) = (const_val(&n.a[1]), const_val(&n.a[2]))
        {
            let holds = match n.op.as_str() {
                "lt" => x < y,
                "ne" => x != y,
                "ge" => x >= y,
                _ => x == y,
            };
            known = Some((
                n.a[0].idx,
                n.a[0].swz.clone(),
                f32::from(u8::from(holds)),
                i,
            ));
        }
        if meta.decal
            && n.op.starts_with("sample")
            && let Some(m) = ins.get(i + 1)
            && m.op == "mad"
            && m.a.len() >= 4
            && m.a[1].kind == Kind::Temp
            && n.a[0].kind == Kind::Temp
            && m.a[1].idx == n.a[0].idx
            && m.a[2].kind == Kind::Literal
            && m.a[3].kind == Kind::Literal
            && literal(m.a[2].imm[0], m.a[2].imm_type) == Some(2.)
            && literal(m.a[3].imm[0], m.a[3].imm_type) == Some(-1.)
        {
            normal_at.insert(out.len());
        }
        if meta.decal
            && n.op == "mul"
            && n.a.len() == 3
            && n.a[2].kind == Kind::Buffer
            && buffer(&n.a[2]).is_empty()
            && n.a[2].idx == 1
            && n.a[1].kind == Kind::Input
            && n.a[0].swz == "xyzw"
        {
            let nx = ins.get(i + 1..i + 4).ok_or("SvPositionToDecal")?;
            let ok = nx[0].op == "mad"
                && nx[0].a[2].idx == 0
                && nx[1].op == "mad"
                && nx[1].a[2].idx == 2
                && nx[2].op == "add"
                && nx[2].a[2].idx == 3
                && nx
                    .iter()
                    .all(|x| x.a[0].kind == Kind::Temp && x.a[0].swz == "xyzw")
                && nx[1].a[0].idx == nx[2].a[0].idx;
            if !ok {
                return Err("SvPositionToDecal used outside the usual idiom".to_owned());
            }
            let r = nx[2].a[0].idx;
            out.push(Stmt::Raw {
                text: format!("r{r} = vec4<f32>(i.decal_pos, 1.0);"),
                dest: format!("r{r}"),
                src: Vec::new(),
            });
            last_is.push(usize::MAX);
            i += 4;
            continue;
        }
        // A vertex shader's last step, translated world to clip: the harness projects with its
        // own camera, so the four instructions become the world position.
        if meta.vertex && n.op == "mul" && i + 3 < ins.len() {
            let row = |x: &Instruction, k: u32| {
                x.a.iter()
                    .any(|o| o.kind == Kind::Buffer && buffer(o) == "View" && o.idx == k)
            };
            let nx = &ins[i..i + 4];
            let is_out = nx[3].a.first().is_some_and(|d| {
                d.kind == Kind::Output
                    && program
                        .outputs
                        .iter()
                        .any(|(r, s)| *r == d.idx && s == "SV_POSITION0")
            });
            if row(&nx[0], 1) && row(&nx[1], 0) && row(&nx[2], 2) && row(&nx[3], 3) && is_out {
                let pick = |x: &Instruction, c: char| {
                    let all: String = std::iter::repeat_n(c, 4).collect();
                    x.a[1..]
                        .iter()
                        .find(|o| matches!(o.kind, Kind::Temp | Kind::Input) && o.swz == all)
                        .cloned()
                };
                let (Some(y), Some(x), Some(z)) =
                    (pick(&nx[0], 'y'), pick(&nx[1], 'x'), pick(&nx[2], 'z'))
                else {
                    return Err("clip transform outside the usual idiom".to_owned());
                };
                if x.idx != y.idx || z.idx != y.idx || y.kind != Kind::Temp || x.kind != y.kind {
                    return Err("clip transform outside the usual idiom".to_owned());
                }
                let r = y.idx;
                out.push(Stmt::Raw {
                    text: format!("kh_wpos = vec4<f32>(r{r}.xyz, 1.0);"),
                    dest: "kh_wpos".to_owned(),
                    src: ['x', 'y', 'z']
                        .iter()
                        .map(|c| format!("r{r}.{c}"))
                        .collect(),
                });
                last_is.push(usize::MAX);
                saw_clip = true;
                i += 4;
                continue;
            }
        }
        if n.op == "mul"
            && n.a.len() == 3
            && n.a[2].kind == Kind::Buffer
            && buffer(&n.a[2]) == "View"
            && n.a[2].idx == 37
            && n.a[1].kind == Kind::Input
            && Some(n.a[1].idx) == sv_reg
            && n.a[0].swz == "xyzw"
        {
            let nx = ins.get(i + 1..i + 4).ok_or("SVPositionToTranslatedWorld")?;
            let ok = nx[0].op == "mad"
                && nx[0].a[2].idx == 36
                && nx[1].op == "mad"
                && nx[1].a[2].idx == 38
                && nx[2].op == "add"
                && nx[2].a[2].idx == 39
                && nx
                    .iter()
                    .all(|x| x.a[0].kind == Kind::Temp && x.a[0].idx == n.a[0].idx);
            if !ok {
                return Err("SVPositionToTranslatedWorld used outside the usual idiom".to_owned());
            }
            let r = n.a[0].idx;
            out.push(Stmt::Raw {
                text: format!("r{r} = vec4<f32>(i.twp, 1.0);"),
                dest: format!("r{r}"),
                src: Vec::new(),
            });
            last_is.push(usize::MAX);
            i += 4;
            continue;
        }
        out.push(Stmt::Ins(n));
        last_is.push(i);
        i += 1;
    }
    if meta.vertex && !saw_clip {
        return Err("no clip transform found".to_owned());
    }
    if !meta.vertex
        && ins.iter().filter(|n| n.op == "ret").count()
            > usize::from(ins.last().is_some_and(|n| n.op == "ret"))
    {
        return Err("early return".to_owned());
    }
    // ---- what reaches the colour outputs is kept
    let vs_outs: Vec<(String, &str)> = program
        .outputs
        .iter()
        .filter_map(|(r, name)| {
            VS_OUT
                .iter()
                .find(|(s, _)| s == name)
                .map(|(_, field)| (format!("o{r}"), *field))
        })
        .collect();
    let wanted: Vec<&str> = if meta.vertex {
        std::iter::once("kh_wpos")
            .chain(vs_outs.iter().map(|(r, _)| r.as_str()))
            .collect()
    } else if meta.mask_only {
        Vec::new()
    } else if meta.decal || meta.distort {
        vec!["o0"]
    } else if program.targets >= 4 {
        vec!["o0", "o1", "o2", "o3"]
    } else {
        vec!["o0"]
    };
    let mut ifs = HashMap::new();
    let mut if_of_end = HashMap::new();
    let mut loop_of_end = HashMap::new();
    let mut loop_stack = Vec::new();
    for x in 0..out.len() {
        match op_of(&out[x]) {
            "if_nz" | "if_z" => {
                let (mut depth, mut else_at, mut end_at) = (0, None, None);
                for (j, s) in out.iter().enumerate().skip(x + 1) {
                    match op_of(s) {
                        "if_nz" | "if_z" => depth += 1,
                        "endif" => {
                            if depth == 0 {
                                end_at = Some(j);
                                break;
                            }
                            depth -= 1;
                        }
                        "else" if depth == 0 => else_at = Some(j),
                        _ => {}
                    }
                }
                let end_at = end_at.ok_or("unbalanced if")?;
                ifs.insert(x, (else_at, end_at));
                if_of_end.insert(end_at, x);
            }
            "loop" => loop_stack.push(x),
            "endloop" => {
                loop_of_end.insert(x, loop_stack.pop().ok_or("unbalanced loop")?);
            }
            _ => {}
        }
    }
    if !loop_stack.is_empty() {
        return Err("unbalanced loop".to_owned());
    }
    let start: HashSet<String> = wanted
        .iter()
        .flat_map(|o| ['x', 'y', 'z', 'w'].map(|c| format!("{o}.{c}")))
        .collect();
    let mut live = Live {
        out: &out,
        needed: vec![false; out.len()],
        ifs,
        if_of_end,
        loop_of_end,
        exits: Vec::new(),
    };
    live.walk(0, out.len(), &start);
    let needed = live.needed;
    let mut live_regs = HashSet::new();
    for (x, s) in out.iter().enumerate() {
        if needed[x] {
            for r in writes(s).into_iter().chain(reads(s)) {
                if let Some((reg, _)) = r.split_once('.') {
                    live_regs.insert(reg.to_owned());
                }
            }
        }
    }
    // ---- the text
    let mut g = Gen {
        meta,
        inputs,
        why: None,
        textures: BTreeSet::new(),
        uses_time: false,
        max_cb: -1,
        masks: HashSet::new(),
        loops: 0,
        distort_row: None,
        rows_used: BTreeSet::new(),
        normal_samples: normal_at,
        at: 0,
    };
    let mut code = Vec::new();
    for (x, s) in out.iter().enumerate() {
        if !needed[x] {
            continue;
        }
        g.at = x;
        let text = match s {
            Stmt::Raw { text, .. } => text.clone(),
            Stmt::Ins(n) => g.generate(n),
        };
        if !text.is_empty() {
            code.push(text);
        }
        if let Some(why) = g.why.take() {
            return Err(why);
        }
    }
    let mut body = Vec::new();
    for row in &g.rows_used {
        body.push(format!("let m{row} = {};", meta.rows[row]));
    }
    for r in 0..program.temps {
        if live_regs.contains(&format!("r{r}")) {
            body.push(format!("var r{r}: vec4<f32>;"));
        }
    }
    for o in &wanted {
        body.push(format!("var {o}: vec4<f32>;"));
    }
    body.extend(code);
    if meta.vertex {
        body.push("var kh_out: KhVsOut;".to_owned());
        for (reg, field) in &vs_outs {
            body.push(format!("kh_out.{field} = {reg};"));
        }
        body.push("kh_out.wpos = kh_wpos.xyz;".to_owned());
        body.push("return kh_out;".to_owned());
    } else if !meta.mask_only {
        let rest = if program.targets >= 4 && !meta.decal {
            "o1, o2, o3"
        } else {
            "vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)"
        };
        body.push(format!("return KhOut(o0, {rest});"));
    }
    Ok(Translated {
        wgsl: body
            .iter()
            .map(|l| format!("    {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
        outputs: program.targets,
        textures: g.textures,
        cb_len: (g.max_cb + 1) as u32,
        uses_time: g.uses_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxbc::Program;

    fn op(kind: Kind, idx: u32, swz: &str) -> Operand {
        Operand {
            kind,
            neg: false,
            abs: false,
            cb: 0,
            idx,
            swz: swz.to_owned(),
            imm: Vec::new(),
            imm_type: ImmType::Float,
        }
    }

    fn ins(name: &str, a: Vec<Operand>) -> Instruction {
        Instruction {
            op: name.to_owned(),
            sat: false,
            tex_type: None,
            a,
        }
    }

    fn program(body: Vec<Instruction>) -> Program {
        Program {
            inputs: vec![(
                0,
                Input {
                    name: "COLOR1".to_owned(),
                    float: true,
                },
            )],
            targets: 1,
            temps: 2,
            ins: body,
            ..Program::default()
        }
    }

    #[test]
    fn a_move_to_the_output_reads_the_input_by_its_semantic() {
        let p = program(vec![
            ins(
                "mov",
                vec![op(Kind::Output, 0, "xyzw"), op(Kind::Input, 0, "xyzw")],
            ),
            ins("ret", Vec::new()),
        ]);
        let t = pixel(&p, &Meta::default()).unwrap();
        assert!(t.wgsl.contains("o0 = i.col1.xyzw;"), "{}", t.wgsl);
        assert!(t.wgsl.contains("return KhOut(o0"));
    }

    #[test]
    fn what_does_not_reach_the_colour_is_left_out() {
        let p = program(vec![
            ins(
                "mov",
                vec![op(Kind::Temp, 1, "xyzw"), op(Kind::Input, 0, "xyzw")],
            ),
            ins(
                "mov",
                vec![op(Kind::Output, 0, "xyzw"), op(Kind::Input, 0, "xyzw")],
            ),
        ]);
        let t = pixel(&p, &Meta::default()).unwrap();
        assert!(!t.wgsl.contains("r1"), "{}", t.wgsl);
    }

    #[test]
    fn an_unknown_input_is_an_error_with_its_name() {
        let mut p = program(vec![ins(
            "mov",
            vec![op(Kind::Output, 0, "xyzw"), op(Kind::Input, 0, "xyzw")],
        )]);
        p.inputs[0].1.name = "TEXCOORD6".to_owned();
        assert_eq!(pixel(&p, &Meta::default()).unwrap_err(), "input TEXCOORD6");
    }

    #[test]
    fn literals_read_as_their_printed_form() {
        assert_eq!(literal(1.0f32.to_bits(), ImmType::Float), Some(1.));
        assert_eq!(literal(2, ImmType::Int), Some(2.));
        assert_eq!(literal(0x3f80_0000, ImmType::Int), Some(1.));
        assert_eq!(literal(0x7fff_ffff, ImmType::Int), None);
        assert_eq!(literal(5, ImmType::Any), Some(5.));
    }

    #[test]
    fn the_material_buffer_table_finds_textures_and_shared_samplers() {
        let buffers = vec!["View".to_owned(), "Material".to_owned()];
        // Offsets per buffer, then the Material buffer's entries.
        let textures = [0, 2, 1 << 24 | 2 << 8 | 3, u32::MAX];
        let samplers = [0, 2, 1 << 24 | 5 << 8 | 4, u32::MAX];
        let (t, s) = resource_tables(&buffers, &textures, &samplers, 2, 0);
        assert_eq!(t.get(&3), Some(&1));
        assert_eq!(s.get(&4), Some(&SamplerOf::Clamp));
    }
}
