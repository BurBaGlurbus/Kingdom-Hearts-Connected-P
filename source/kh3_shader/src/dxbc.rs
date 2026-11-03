//! A DXBC (shader model 5) reader: the signatures and the instruction list, with the operands in
//! the shape the translator asks for. Follows StrawHatPro's `tools/lib/dxbc_disasm.js` (MIT), which
//! is checked against Microsoft's disassembler; an instruction it does not know is an error.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Temp,
    Input,
    Output,
    Texture,
    Sampler,
    Buffer,
    /// A constant buffer read at a computed row.
    BufferDynamic,
    Literal,
    Null,
    Unknown,
}

/// How a literal's bits read: as the instruction's own type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImmType {
    Float,
    Int,
    Any,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Operand {
    pub kind: Kind,
    pub neg: bool,
    pub abs: bool,
    /// The buffer's slot.
    pub cb: u32,
    pub idx: u32,
    /// Components, as written: `xyzw`, `xz`, `y`.
    pub swz: String,
    pub imm: Vec<u32>,
    pub imm_type: ImmType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instruction {
    pub op: String,
    pub sat: bool,
    /// The resource's dimension of an `_indexable` sample.
    pub tex_type: Option<&'static str>,
    pub a: Vec<Operand>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Input {
    pub name: String,
    pub float: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    /// Input signature by register: semantic with its index (`TEXCOORD10`).
    pub inputs: Vec<(u32, Input)>,
    pub targets: usize,
    /// Output signature by register, upper case with the index (a vertex shader's interpolants);
    /// `None` when two share a register.
    pub outputs: Vec<(u32, String)>,
    pub packed_outputs: bool,
    pub temps: u32,
    pub ins: Vec<Instruction>,
}

const COMP: [char; 4] = ['x', 'y', 'z', 'w'];
const RES_DIM: [&str; 13] = [
    "unknown",
    "buffer",
    "texture1d",
    "texture2d",
    "texture2dms",
    "texture3d",
    "texturecube",
    "texture1darray",
    "texture2darray",
    "texture2dmsarray",
    "texturecubearray",
    "raw_buffer",
    "structured_buffer",
];

fn opcode(code: u32) -> Option<(&'static str, ImmType, u8)> {
    // Flow: 0 none, 1 test (`_z` / `_nz`).
    use ImmType::{Any as A, Float as F, Int as I};
    Some(match code {
        0 => ("add", F, 0),
        1 => ("and", I, 0),
        2 => ("break", I, 0),
        3 => ("breakc", I, 1),
        6 => ("case", I, 0),
        7 => ("continue", I, 0),
        8 => ("continuec", I, 1),
        10 => ("default", I, 0),
        11 => ("deriv_rtx", F, 0),
        12 => ("deriv_rty", F, 0),
        13 => ("discard", I, 1),
        14 => ("div", F, 0),
        15 => ("dp2", F, 0),
        16 => ("dp3", F, 0),
        17 => ("dp4", F, 0),
        18 => ("else", I, 0),
        21 => ("endif", I, 0),
        22 => ("endloop", I, 0),
        23 => ("endswitch", I, 0),
        24 => ("eq", F, 0),
        25 => ("exp", F, 0),
        26 => ("frc", F, 0),
        27 => ("ftoi", F, 0),
        28 => ("ftou", F, 0),
        29 => ("ge", F, 0),
        30 => ("iadd", I, 0),
        31 => ("if", I, 1),
        32 => ("ieq", I, 0),
        33 => ("ige", I, 0),
        34 => ("ilt", I, 0),
        35 => ("imad", I, 0),
        36 => ("imax", I, 0),
        37 => ("imin", I, 0),
        38 => ("imul", I, 0),
        39 => ("ine", I, 0),
        40 => ("ineg", I, 0),
        41 => ("ishl", I, 0),
        42 => ("ishr", I, 0),
        43 => ("itof", I, 0),
        45 => ("ld", I, 0),
        46 => ("ld_ms", I, 0),
        47 => ("log", F, 0),
        48 => ("loop", I, 0),
        49 => ("lt", F, 0),
        50 => ("mad", F, 0),
        51 => ("min", F, 0),
        52 => ("max", F, 0),
        54 => ("mov", A, 0),
        55 => ("movc", A, 0),
        56 => ("mul", F, 0),
        57 => ("ne", F, 0),
        58 => ("nop", I, 0),
        59 => ("not", I, 0),
        60 => ("or", I, 0),
        61 => ("resinfo", I, 0),
        62 => ("ret", I, 0),
        63 => ("retc", I, 1),
        64 => ("round_ne", F, 0),
        65 => ("round_ni", F, 0),
        66 => ("round_pi", F, 0),
        67 => ("round_z", F, 0),
        68 => ("rsq", F, 0),
        69 => ("sample", F, 0),
        70 => ("sample_c", F, 0),
        71 => ("sample_c_lz", F, 0),
        72 => ("sample_l", F, 0),
        73 => ("sample_d", F, 0),
        74 => ("sample_b", F, 0),
        75 => ("sqrt", F, 0),
        76 => ("switch", I, 0),
        77 => ("sincos", F, 0),
        78 => ("udiv", I, 0),
        79 => ("ult", I, 0),
        80 => ("uge", I, 0),
        81 => ("umul", I, 0),
        82 => ("umad", I, 0),
        83 => ("umax", I, 0),
        84 => ("umin", I, 0),
        85 => ("ushr", I, 0),
        86 => ("utof", I, 0),
        87 => ("xor", I, 0),
        108 => ("lod", F, 0),
        109 => ("gather4", F, 0),
        110 => ("samplepos", I, 0),
        111 => ("sampleinfo", I, 0),
        121 => ("bufinfo", I, 0),
        122 => ("deriv_rtx_coarse", F, 0),
        123 => ("deriv_rtx_fine", F, 0),
        124 => ("deriv_rty_coarse", F, 0),
        125 => ("deriv_rty_fine", F, 0),
        126 => ("gather4_c", F, 0),
        127 => ("gather4_po", F, 0),
        128 => ("gather4_po_c", F, 0),
        129 => ("rcp", F, 0),
        130 => ("f32tof16", F, 0),
        131 => ("f16tof32", I, 0),
        132 => ("uaddc", I, 0),
        133 => ("usubb", I, 0),
        134 => ("countbits", I, 0),
        135 => ("firstbit_hi", I, 0),
        136 => ("firstbit_lo", I, 0),
        137 => ("firstbit_shi", I, 0),
        138 => ("ubfe", I, 0),
        139 => ("ibfe", I, 0),
        140 => ("bfi", I, 0),
        141 => ("bfrev", I, 0),
        142 => ("swapc", A, 0),
        _ => return None,
    })
}

fn is_resource_op(code: u32) -> bool {
    matches!(
        code,
        45 | 46 | 61 | 69 | 70 | 71 | 72 | 73 | 74 | 108 | 109 | 126 | 127 | 128 | 110 | 111
    )
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    let s = b.get(at..at + 4).ok_or("past the container")?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn name_at(b: &[u8], at: usize) -> Result<String, String> {
    let rest = b.get(at..).ok_or("name past the container")?;
    let end = rest.iter().position(|c| *c == 0).ok_or("unended name")?;
    Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
}

/// Reads an operand at `p`; returns it and the next place.
fn read_operand(t: &[u32], mut p: usize) -> Result<(Operand, usize), String> {
    let tok = *t.get(p).ok_or("operand past the code")?;
    p += 1;
    let (num_comp, sel_mode) = (tok & 3, (tok >> 2) & 3);
    let (reg, dim) = ((tok >> 12) & 0xff, (tok >> 20) & 3);
    let mut swz = String::new();
    if num_comp == 2 {
        match sel_mode {
            0 => {
                let mask = (tok >> 4) & 0xf;
                swz = (0..4)
                    .filter(|i| mask & (1 << i) != 0)
                    .map(|i| COMP[i])
                    .collect();
            }
            1 => {
                swz = (0..4)
                    .map(|i| COMP[((tok >> (4 + 2 * i)) & 3) as usize])
                    .collect()
            }
            _ => swz.push(COMP[((tok >> 4) & 3) as usize]),
        }
    } else if num_comp == 3 {
        return Err("N-component operand".to_owned());
    }
    let mut more = tok >> 31 != 0;
    let (mut neg, mut abs) = (false, false);
    while more {
        let e = *t.get(p).ok_or("operand past the code")?;
        p += 1;
        more = e >> 31 != 0;
        if e & 0x3f == 1 {
            match (e >> 6) & 0xff {
                1 => neg = true,
                2 => abs = true,
                3 => (neg, abs) = (true, true),
                _ => {}
            }
        }
    }
    let mut o = Operand {
        kind: Kind::Unknown,
        neg,
        abs,
        cb: 0,
        idx: 0,
        swz,
        imm: Vec::new(),
        imm_type: ImmType::Float,
    };
    if reg == 4 || reg == 5 {
        if reg == 5 {
            return Err("64-bit immediate".to_owned());
        }
        let n = match num_comp {
            1 => 1,
            2 => 4,
            _ => 0,
        };
        o.kind = Kind::Literal;
        o.imm = t.get(p..p + n).ok_or("immediate past the code")?.to_vec();
        return Ok((o, p + n));
    }
    let mut indices = Vec::new();
    let mut relative = false;
    for d in 0..dim {
        let rep = (tok >> (22 + 3 * d)) & 7;
        match rep {
            0 => {
                indices.push(*t.get(p).ok_or("index past the code")?);
                p += 1;
            }
            2 => {
                let (_, q) = read_operand(t, p)?;
                p = q;
                relative = true;
                indices.push(0);
            }
            3 => {
                indices.push(*t.get(p).ok_or("index past the code")?);
                p += 1;
                let (_, q) = read_operand(t, p)?;
                p = q;
                relative = true;
            }
            _ => return Err("64-bit index".to_owned()),
        }
    }
    let first = indices.first().copied().unwrap_or(0);
    o.kind = match reg {
        0 => Kind::Temp,
        1 => Kind::Input,
        2 => Kind::Output,
        6 => Kind::Sampler,
        7 => Kind::Texture,
        8 => Kind::Buffer,
        13 => Kind::Null,
        _ => Kind::Unknown,
    };
    if o.kind == Kind::Buffer {
        if relative {
            o.kind = Kind::BufferDynamic;
        } else {
            o.cb = first;
            o.idx = indices.get(1).copied().unwrap_or(0);
        }
    } else {
        if relative {
            o.kind = Kind::Unknown;
        }
        o.idx = first;
    }
    if o.swz.is_empty() {
        o.swz = "xyzw".to_owned();
    }
    Ok((o, p))
}

/// Reads a container.
pub fn read(b: &[u8]) -> Result<Program, String> {
    if b.get(..4) != Some(b"DXBC") {
        return Err("not a DXBC container".to_owned());
    }
    let mut isgn = None;
    let mut osgn = None;
    let mut code = None;
    for k in 0..u32_at(b, 28)? as usize {
        let o = u32_at(b, 32 + k * 4)? as usize;
        let tag = b.get(o..o + 4).ok_or("chunk past the container")?;
        let size = u32_at(b, o + 4)? as usize;
        match tag {
            b"ISGN" => isgn = Some(o + 8),
            b"OSGN" => osgn = Some(o + 8),
            b"SHEX" | b"SHDR" => code = Some((o + 8, size)),
            _ => {}
        }
    }
    let mut program = Program::default();
    let at = isgn.ok_or("no input signature")?;
    let first = at + u32_at(b, at + 4)? as usize;
    for k in 0..u32_at(b, at)? as usize {
        let e = first + k * 24;
        let name = name_at(b, at + u32_at(b, e)? as usize)?;
        let index = u32_at(b, e + 4)?;
        let (fmt, reg) = (u32_at(b, e + 12)?, u32_at(b, e + 16)?);
        if reg == u32::MAX {
            continue;
        }
        program.inputs.retain(|(r, _)| *r != reg);
        program.inputs.push((
            reg,
            Input {
                name: format!("{name}{index}"),
                float: fmt == 3,
            },
        ));
    }
    let at = osgn.ok_or("no output signature")?;
    let first = at + u32_at(b, at + 4)? as usize;
    for k in 0..u32_at(b, at)? as usize {
        let name = name_at(b, at + u32_at(b, first + k * 24)? as usize)?;
        if name.eq_ignore_ascii_case("sv_target") {
            program.targets += 1;
        }
        let (index, reg) = (
            u32_at(b, first + k * 24 + 4)?,
            u32_at(b, first + k * 24 + 16)?,
        );
        if program.outputs.iter().any(|(r, _)| *r == reg) {
            program.packed_outputs = true;
        }
        program
            .outputs
            .push((reg, format!("{}{index}", name.to_ascii_uppercase())));
    }
    let (at, size) = code.ok_or("no shader code")?;
    let mut t = Vec::with_capacity(size / 4);
    for k in 0..size / 4 {
        t.push(u32_at(b, at + k * 4)?);
    }
    let len = *t.get(1).ok_or("no shader length")? as usize;
    let mut p = 2;
    while p < len {
        let tok = t[p];
        let code = tok & 0x7ff;
        let mut ilen = ((tok >> 24) & 0x7f) as usize;
        if code == 53 {
            ilen = *t.get(p + 1).ok_or("custom data past the code")? as usize;
            if tok >> 11 != 3 {
                return Err("custom data".to_owned());
            }
            p += ilen;
            continue;
        }
        if ilen == 0 {
            return Err("a zero-length instruction".to_owned());
        }
        let end = p + ilen;
        let mut q = p + 1;
        let (mut dim, mut off) = (None, false);
        let mut more = tok >> 31 != 0;
        while more {
            let e = *t.get(q).ok_or("past the code")?;
            q += 1;
            more = e >> 31 != 0;
            match e & 0x3f {
                1 => off = true,
                2 => dim = Some(((e >> 6) & 0x1f) as usize),
                _ => {}
            }
        }
        if (88..=106).contains(&code) || (143..=162).contains(&code) {
            if code == 104 {
                program.temps = *t.get(q).ok_or("past the code")?;
            }
            p = end;
            continue;
        }
        let (name, imm, flow) = opcode(code).ok_or_else(|| format!("opcode {code}"))?;
        let mut ops = Vec::new();
        while q < end {
            let (mut o, r) = read_operand(&t, q)?;
            o.imm_type = imm;
            ops.push(o);
            q = r;
        }
        let mut op = name.to_owned();
        if flow == 1 {
            op.push_str(if (tok >> 18) & 1 != 0 { "_nz" } else { "_z" });
        }
        let mut tex_type = None;
        if is_resource_op(code) && (dim.is_some() || off) {
            if off {
                op.push_str("_aoffimmi");
            }
            if let Some(d) = dim {
                op.push_str("_indexable");
                tex_type = RES_DIM.get(d).copied();
            }
        }
        program.ins.push(Instruction {
            op,
            sat: (tok >> 13) & 1 != 0,
            tex_type,
            a: ops,
        });
        p = end;
    }
    Ok(program)
}
