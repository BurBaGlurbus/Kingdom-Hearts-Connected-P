//! The compiled shaders inside a cooked material: `FMaterialShaderMap`, with the uniform expressions
//! that fill the material's constant buffer and the D3D11 bytecode of the shader types asked for.
//! Layout from StrawHatPro's `docs/formats/material.md` and `tools/lib/pkg_classes/material.js`
//! (MIT).
use super::package::{Package, Rd, read_name};
use super::props::export_props;
use super::{Result, bad};

/// A uniform expression, as `FMaterialUniformExpression*` stores it.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Constant([f32; 4]),
    Vector(String, [f32; 4]),
    Scalar(String, f32),
    Texture {
        parameter: Option<String>,
        index: i32,
    },
    Time,
    Sine(Box<Expr>, bool),
    /// `sqrt`, `frac`, `floor`, `ceil`, `abs`, `saturate`, `round`, `sign`, `truncate`.
    Unary(&'static str, Box<Expr>),
    /// `add`, `sub`, `mul`, `div`, `dot`, `min`, `max`, `fmod`.
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Append(Box<Expr>, Box<Expr>, u32),
    Clamp(Box<Expr>, Box<Expr>, Box<Expr>),
    Swizzle(Box<Expr>, [i8; 4], i8),
    /// 0 sin, 1 cos, 2 tan, 3 asin, 4 acos, 5 atan, 6 atan2.
    Trig(Box<Expr>, Box<Expr>, u8),
    /// A kind no file is known to use the fields of: evaluates to 0.
    Unknown,
}

impl Expr {
    /// Whether its value changes with the clock.
    pub fn uses_time(&self) -> bool {
        match self {
            Expr::Time => true,
            Expr::Sine(a, _) | Expr::Unary(_, a) | Expr::Swizzle(a, ..) => a.uses_time(),
            Expr::Binary(_, a, b) | Expr::Append(a, b, _) | Expr::Trig(a, b, _) => {
                a.uses_time() || b.uses_time()
            }
            Expr::Clamp(a, b, c) => a.uses_time() || b.uses_time() || c.uses_time(),
            _ => false,
        }
    }

    /// Its value, with the material's parameters.
    pub fn eval(
        &self,
        time: f32,
        scalar: &dyn Fn(&str) -> Option<f32>,
        vector: &dyn Fn(&str) -> Option<[f32; 4]>,
    ) -> [f32; 4] {
        let e = |x: &Expr| x.eval(time, scalar, vector);
        let map = |v: [f32; 4], f: &dyn Fn(f32) -> f32| v.map(f);
        match self {
            Expr::Constant(v) => *v,
            Expr::Vector(name, default) => vector(name).unwrap_or(*default),
            Expr::Scalar(name, default) => [scalar(name).unwrap_or(*default); 4],
            Expr::Time => [time; 4],
            Expr::Texture { .. } | Expr::Unknown => [0.; 4],
            Expr::Sine(x, cosine) => map(e(x), if *cosine { &f32::cos } else { &f32::sin }),
            Expr::Unary(name, x) => {
                let v = e(x);
                match *name {
                    "sqrt" => map(v, &|a| a.max(0.).sqrt()),
                    "frac" => map(v, &|a| a - a.floor()),
                    "floor" => map(v, &f32::floor),
                    "ceil" => map(v, &f32::ceil),
                    "abs" => map(v, &f32::abs),
                    "saturate" => map(v, &|a| a.clamp(0., 1.)),
                    "round" => map(v, &f32::round),
                    "sign" => map(v, &|a| if a == 0. { 0. } else { a.signum() }),
                    "truncate" => map(v, &f32::trunc),
                    _ => [0.; 4],
                }
            }
            Expr::Binary(name, a, b) => {
                let (a, b) = (e(a), e(b));
                let zip = |f: &dyn Fn(f32, f32) -> f32| [0, 1, 2, 3].map(|i| f(a[i], b[i]));
                match *name {
                    "add" => zip(&|x, y| x + y),
                    "sub" => zip(&|x, y| x - y),
                    "mul" => zip(&|x, y| x * y),
                    "div" => zip(&|x, y| if y.abs() < 1e-5 { 0. } else { x / y }),
                    "dot" => [a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]; 4],
                    "min" => zip(&f32::min),
                    "max" => zip(&f32::max),
                    "fmod" => zip(&|x, y| if y == 0. { 0. } else { x % y }),
                    _ => [0.; 4],
                }
            }
            Expr::Append(a, b, n) => {
                let (a, b) = (e(a), e(b));
                let n = *n as usize;
                [0, 1, 2, 3].map(|i| {
                    if i < n {
                        a[i]
                    } else {
                        b.get(i - n).copied().unwrap_or(0.)
                    }
                })
            }
            Expr::Clamp(x, lo, hi) => {
                let (x, lo, hi) = (e(x), e(lo), e(hi));
                [0, 1, 2, 3].map(|i| x[i].max(lo[i]).min(hi[i]))
            }
            Expr::Swizzle(x, index, n) => {
                let v = e(x);
                [0i8, 1, 2, 3].map(|i| {
                    let at = index[i as usize];
                    if i < *n && (0..4).contains(&at) {
                        v[at as usize]
                    } else {
                        0.
                    }
                })
            }
            Expr::Trig(x, y, op) => {
                let (x, y) = (e(x), e(y));
                let f: &dyn Fn(f32) -> f32 = match op {
                    0 => &f32::sin,
                    1 => &f32::cos,
                    2 => &f32::tan,
                    3 => &f32::asin,
                    4 => &f32::acos,
                    _ => &f32::atan,
                };
                if *op == 6 {
                    [0, 1, 2, 3].map(|i| x[i].atan2(y[i]))
                } else {
                    x.map(|a| {
                        let v = f(a);
                        if v.is_finite() { v } else { 0. }
                    })
                }
            }
        }
    }
}

/// One compiled shader.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shader {
    pub kind: String,
    pub dxbc: Vec<u8>,
    /// The uniform buffers' names by constant-buffer slot.
    pub buffers: Vec<String>,
    pub sampler_map: Vec<u32>,
    pub texture_map: Vec<u32>,
}

/// One vertex factory's shaders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshShaders {
    pub factory: String,
    pub shaders: Vec<Shader>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaderMap {
    pub description: String,
    pub vectors: Vec<Expr>,
    pub scalars: Vec<Expr>,
    pub textures_2d: Vec<Expr>,
    pub textures_cube: usize,
    /// The map's own shaders (a decal's).
    pub top: Vec<Shader>,
    pub mesh: Vec<MeshShaders>,
}

fn skip_guid(r: &mut Rd) -> Result<()> {
    r.skip(16)
}

fn skip_guids(r: &mut Rd) -> Result<()> {
    for _ in 0..r.count(16)? {
        skip_guid(r)?;
    }
    Ok(())
}

fn skip_parameter_set(r: &mut Rd) -> Result<()> {
    for size in [8 + 4 + 4 + 16, 8 + 16 + 4 + 16, 8 + 4 + 4 + 16] {
        let n = r.count(size)?;
        r.skip(n * size)?;
    }
    Ok(())
}

fn colour(r: &mut Rd) -> Result<[f32; 4]> {
    Ok([r.f32()?, r.f32()?, r.f32()?, r.f32()?])
}

fn expression(r: &mut Rd, names: &[String]) -> Result<Expr> {
    let kind = read_name(r, names)?;
    let kind = kind
        .strip_prefix("FMaterialUniformExpression")
        .unwrap_or(&kind);
    let sub = |r: &mut Rd| -> Result<Box<Expr>> { Ok(Box::new(expression(r, names)?)) };
    Ok(match kind {
        "Constant" => {
            let v = colour(r)?;
            r.u8()?;
            Expr::Constant(v)
        }
        "VectorParameter" => {
            let name = read_name(r, names)?;
            Expr::Vector(name, colour(r)?)
        }
        "ScalarParameter" => {
            let name = read_name(r, names)?;
            Expr::Scalar(name, r.f32()?)
        }
        "Texture" | "FlipBookTextureParameter" => {
            let index = r.i32()?;
            r.i32()?;
            Expr::Texture {
                parameter: None,
                index,
            }
        }
        "TextureParameter" => {
            let name = read_name(r, names)?;
            let index = r.i32()?;
            r.i32()?;
            Expr::Texture {
                parameter: Some(name),
                index,
            }
        }
        "Time" | "RealTime" => Expr::Time,
        "Sine" => {
            let x = sub(r)?;
            Expr::Sine(x, r.u32()? != 0)
        }
        "SquareRoot" => Expr::Unary("sqrt", sub(r)?),
        "Period" | "Periodic" | "Frac" => Expr::Unary("frac", sub(r)?),
        "Floor" => Expr::Unary("floor", sub(r)?),
        "Ceil" => Expr::Unary("ceil", sub(r)?),
        "Abs" => Expr::Unary("abs", sub(r)?),
        "Saturate" => Expr::Unary("saturate", sub(r)?),
        "Round" => Expr::Unary("round", sub(r)?),
        "Sign" => Expr::Unary("sign", sub(r)?),
        "Truncate" => Expr::Unary("truncate", sub(r)?),
        "Logarithm2" | "Logarithm10" => {
            sub(r)?;
            Expr::Unknown
        }
        "TrigMath" => {
            let (x, y) = (sub(r)?, sub(r)?);
            Expr::Trig(x, y, r.u8()?)
        }
        "FoldedMath" => {
            let (a, b) = (sub(r)?, sub(r)?);
            let op = r.u8()?;
            r.u32()?;
            let name = ["add", "sub", "mul", "div", "dot"]
                .get(usize::from(op))
                .copied()
                .ok_or_else(|| bad("uniform expression op"))?;
            Expr::Binary(name, a, b)
        }
        "AppendVector" => {
            let (a, b) = (sub(r)?, sub(r)?);
            Expr::Append(a, b, r.u32()?)
        }
        "Min" => Expr::Binary("min", sub(r)?, sub(r)?),
        "Max" => Expr::Binary("max", sub(r)?, sub(r)?),
        "Fmod" => Expr::Binary("fmod", sub(r)?, sub(r)?),
        "Clamp" => Expr::Clamp(sub(r)?, sub(r)?, sub(r)?),
        "ComponentSwizzle" => {
            let x = sub(r)?;
            let mut index = [0i8; 4];
            for i in &mut index {
                *i = r.u8()? as i8;
            }
            Expr::Swizzle(x, index, r.u8()? as i8)
        }
        other => return Err(bad(format!("uniform expression {other}"))),
    })
}

fn expressions(r: &mut Rd, names: &[String]) -> Result<Vec<Expr>> {
    let n = r.count(8)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(expression(r, names)?);
    }
    Ok(out)
}

fn u32s(r: &mut Rd) -> Result<Vec<u32>> {
    let n = r.count(4)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(r.u32()?);
    }
    Ok(out)
}

/// The D3D11 RHI's code: the resource table, the DXBC container, its optional data.
fn shader_code(code: &[u8], out: &mut Shader) -> Result<()> {
    let mut r = Rd::new(code);
    r.u32()?;
    let _views = u32s(&mut r)?;
    out.sampler_map = u32s(&mut r)?;
    let _unordered = u32s(&mut r)?;
    let _hashes = u32s(&mut r)?;
    out.texture_map = u32s(&mut r)?;
    let at = code.len() - r.left();
    let head = code.get(at..at + 32).ok_or_else(|| bad("shader code"))?;
    if &head[..4] != b"DXBC" {
        return Err(bad("shader code is not DXBC"));
    }
    let size = u32::from_le_bytes([head[24], head[25], head[26], head[27]]) as usize;
    out.dxbc = code
        .get(at..at + size)
        .ok_or_else(|| bad("DXBC past the code"))?
        .to_vec();
    r.skip(size)?;
    while r.left() > 4 {
        let key = r.u8()?;
        let len = usize::try_from(r.i32()?).map_err(|_| bad("optional data size"))?;
        if key == b'u' {
            let bytes = r.take(len)?;
            let mut u = Rd::new(bytes);
            let n = u.count(4)?;
            for _ in 0..n {
                out.buffers.push(u.fstring()?);
            }
        } else {
            r.skip(len)?;
        }
    }
    Ok(())
}

/// The fields of `FShader::SerializeBase` after the shader class's own bindings, to the record's
/// end; `Err` when `at` is not their start.
fn shader_tail(bytes: &[u8], at: usize, names: &[String], out: &mut Shader) -> Result<()> {
    let mut r = Rd::new(bytes);
    r.skip(at)?;
    r.skip(40)?;
    read_name(&mut r, names)?;
    read_name(&mut r, names)?;
    r.skip(20)?;
    read_name(&mut r, names)?;
    r.skip(20 + 8)?;
    for _ in 0..r.count(10)? {
        r.fstring()?;
        r.skip(2 + 4)?;
    }
    read_name(&mut r, names)?;
    r.skip(8 + 20 + 4 + 4 + 4)?;
    if r.u32()? != 0 {
        return Err(bad("shared shader code"));
    }
    let n = usize::try_from(r.i32()?).map_err(|_| bad("shader code size"))?;
    let code = r.take(n)?;
    if r.left() != 0 {
        return Err(bad("the tail does not end the record"));
    }
    shader_code(code, out)
}

/// One record: its end offset, the class's bindings (of no known length), the common tail, whose
/// start is the first place from which it parses to the record's end with the record's own type.
fn shader_record(
    r: &mut Rd,
    bytes: &[u8],
    base: i64,
    names: &[String],
    kind: (&str, i32, i32),
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Option<Shader>> {
    let end = i64::from(r.i32()?) - base;
    let start = bytes.len() - r.left();
    let end = usize::try_from(end)
        .ok()
        .filter(|e| *e > start && *e <= bytes.len())
        .ok_or_else(|| bad("shader end offset"))?;
    r.skip(end - start)?;
    if !wanted(kind.0) {
        return Ok(None);
    }
    let record = &bytes[..end];
    let (index, number) = (kind.1.to_le_bytes(), kind.2.to_le_bytes());
    for p in start..end.saturating_sub(104) {
        if record[p + 76..p + 80] != index || record[p + 80..p + 84] != number {
            continue;
        }
        let mut shader = Shader {
            kind: kind.0.to_owned(),
            ..Shader::default()
        };
        if shader_tail(record, p, names, &mut shader).is_ok() {
            return Ok(Some(shader));
        }
    }
    Err(bad(format!("{}: the shader's tail not found", kind.0)))
}

fn raw_name(r: &mut Rd, names: &[String]) -> Result<(String, i32, i32)> {
    let at = names;
    let (index, number) = (r.i32()?, r.i32()?);
    let name = usize::try_from(index)
        .ok()
        .and_then(|i| at.get(i))
        .ok_or_else(|| bad("bad name index"))?;
    Ok((name.clone(), index, number))
}

fn shader_list(
    r: &mut Rd,
    bytes: &[u8],
    base: i64,
    names: &[String],
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Vec<Shader>> {
    let mut out = Vec::new();
    for _ in 0..r.count(12)? {
        let (name, index, number) = raw_name(r, names)?;
        out.extend(shader_record(
            r,
            bytes,
            base,
            names,
            (&name, index, number),
            wanted,
        )?);
    }
    Ok(out)
}

fn shaders_and_pipelines(
    r: &mut Rd,
    bytes: &[u8],
    base: i64,
    names: &[String],
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Vec<Shader>> {
    let shaders = shader_list(r, bytes, base, names, wanted)?;
    for _ in 0..r.count(12)? {
        read_name(r, names)?;
        shader_list(r, bytes, base, names, &|_| false)?;
    }
    Ok(shaders)
}

fn read_map(
    r: &mut Rd,
    bytes: &[u8],
    base: i64,
    names: &[String],
    factory: &dyn Fn(&str) -> bool,
    wanted: &dyn Fn(&str) -> bool,
) -> Result<ShaderMap> {
    // FMaterialShaderMapId.
    r.u32()?;
    skip_guid(r)?;
    r.skip(8)?;
    skip_parameter_set(r)?;
    skip_guids(r)?;
    skip_guids(r)?;
    for _ in 0..3 {
        let n = r.count(28)?;
        r.skip(n * 28)?;
    }
    r.skip(40)?;
    r.i32()?;
    let _friendly = r.fstring()?;
    // FUniformExpressionSet.
    let vectors = expressions(r, names)?;
    let scalars = expressions(r, names)?;
    let textures_2d = expressions(r, names)?;
    let textures_cube = expressions(r, names)?.len();
    expressions(r, names)?;
    skip_guids(r)?;
    for _ in 0..4 {
        expressions(r, names)?;
    }
    r.skip(9 * 4)?;
    let description = r.fstring()?;
    let top = shaders_and_pipelines(r, bytes, base, names, wanted)?;
    let mut mesh = Vec::new();
    for _ in 0..r.count(12)? {
        let name = read_name(r, names)?;
        let list = if factory(&name) {
            wanted
        } else {
            &|_: &str| false
        };
        let shaders = shaders_and_pipelines(r, bytes, base, names, list)?;
        if !shaders.is_empty() {
            mesh.push(MeshShaders {
                factory: name,
                shaders,
            });
        }
    }
    Ok(ShaderMap {
        description,
        vectors,
        scalars,
        textures_2d,
        textures_cube,
        top,
        mesh,
    })
}

/// The first shader map of a `Material` or `MaterialInstanceConstant` export, if it owns one; only
/// the vertex factories and shader types asked for keep their bytecode.
pub fn read(
    p: &Package,
    i: usize,
    factory: &dyn Fn(&str) -> bool,
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Option<ShaderMap>> {
    let bytes = p.export_bytes(i)?;
    let (_, end) = export_props(p, i)?;
    let mut r = Rd::new(bytes);
    r.skip(end)?;
    // The object's guid flag.
    if r.i32()? != 0 {
        skip_guid(&mut r)?;
    }
    if p.class_of(i) != "Material" {
        if r.left() == 0 {
            return Ok(None);
        }
        skip_parameter_set(&mut r)?;
    }
    if r.left() < 4 {
        return Ok(None);
    }
    let base = p.exports[i].offset;
    for _ in 0..r.count(8)? {
        let (_cooked, valid) = (r.u32()?, r.u32()?);
        if valid == 0 {
            continue;
        }
        return read_map(&mut r, bytes, base, &p.names, factory, wanted).map(Some);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<f32> {
        None
    }

    fn none4(_: &str) -> Option<[f32; 4]> {
        None
    }

    #[test]
    fn expressions_evaluate_with_parameters_and_the_clock() {
        let a = Box::new(Expr::Scalar("speed".to_owned(), 2.));
        let e = Expr::Binary("mul", a, Box::new(Expr::Time));
        assert!(e.uses_time());
        assert_eq!(e.eval(3., &none, &none4), [6.; 4]);
        assert_eq!(e.eval(3., &|_| Some(1.), &none4), [3.; 4]);
    }

    #[test]
    fn a_swizzle_and_an_append_take_their_components() {
        let c = Box::new(Expr::Constant([1., 2., 3., 4.]));
        let s = Expr::Swizzle(c.clone(), [2, 1, -1, -1], 2);
        assert_eq!(s.eval(0., &none, &none4), [3., 2., 0., 0.]);
        let a = Expr::Append(c.clone(), c, 1);
        assert_eq!(a.eval(0., &none, &none4), [1., 1., 2., 3.]);
    }

    #[test]
    fn a_division_by_nothing_is_nothing() {
        let e = Expr::Binary(
            "div",
            Box::new(Expr::Constant([1.; 4])),
            Box::new(Expr::Constant([0.; 4])),
        );
        assert_eq!(e.eval(0., &none, &none4), [0.; 4]);
    }
}
