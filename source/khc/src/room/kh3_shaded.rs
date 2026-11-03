//! KH3's effect materials as the game shades them [KHC-009]: each material's compiled base-pass pixel
//! shader, read from the player's paks and translated (`kh3_shader`), with its constant buffer from
//! the uniform expressions and its textures by slot. A material no shader can be translated for stays
//! on the old generic path in `kh3_fx`. Follows StrawHatPro's `tools/sora_vfx_shaders.js` (MIT).

use bevy::asset::{AssetPath, RenderAssetUsages, embedded_path};
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef, VertexFormat};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
    RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureDimension,
    TextureFormat,
};
use bevy::shader::{Shader, ShaderRef};
use kh3_data::Paks;
use kh3_data::material::{Blend as MatBlend, MaterialParams};
use kh3_data::package::Package;
use kh3_data::props::Value;
use kh3_data::shader_map::{self, Expr, ShaderMap};
use kh3_data::texture::Texture;
use kh3_shader::dxbc;
use kh3_shader::translate::{self, Meta};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, RwLock};

pub const ATTR_GAME: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Game", 3_940_000_001, VertexFormat::Float32x3);
pub const ATTR_UV: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Uv", 3_940_000_002, VertexFormat::Float32x4);
pub const ATTR_COLOR: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Color", 3_940_000_003, VertexFormat::Float32x4);
pub const ATTR_VCOL: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Vcol", 3_940_000_004, VertexFormat::Float32x4);
pub const ATTR_DYNAMIC: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Dynamic", 3_940_000_005, VertexFormat::Float32x4);
pub const ATTR_MISC: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Misc", 3_940_000_006, VertexFormat::Float32x4);
pub const ATTR_NORMAL: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Normal", 3_940_000_007, VertexFormat::Float32x3);
pub const ATTR_TANGENT: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Tangent", 3_940_000_008, VertexFormat::Float32x4);
pub const ATTR_PARTICLE: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Particle", 3_940_000_009, VertexFormat::Float32x4);
pub const ATTR_TC1: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Tc1", 3_940_000_010, VertexFormat::Float32x4);
pub const ATTR_ROW0: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Row0", 3_940_000_011, VertexFormat::Float32x4);
pub const ATTR_ROW1: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Row1", 3_940_000_012, VertexFormat::Float32x4);
pub const ATTR_ROW2: MeshVertexAttribute =
    MeshVertexAttribute::new("Kh3Row2", 3_940_000_013, VertexFormat::Float32x4);

/// The translated shaders' handles by variant: a material key holds only the number.
static SHADERS: RwLock<Vec<Handle<Shader>>> = RwLock::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct ShadedKey {
    pub variant: u32,
    /// 0 translucent, 1 additive, 2 opaque, 3 modulate, 4 distortion, 5 decal over, 6 decal added.
    pub pass: u32,
    /// The translated vertex shader's variant + 1; 0 the shared one.
    pub vertex: u32,
}

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct KhParams {
    pub m: [Vec4; 64],
    pub misc: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
#[bind_group_data(ShadedKey)]
pub struct ShadedMaterial {
    #[uniform(0)]
    pub params: KhParams,
    #[texture(1)]
    #[sampler(2)]
    pub t0: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub t1: Option<Handle<Image>>,
    #[texture(5)]
    #[sampler(6)]
    pub t2: Option<Handle<Image>>,
    #[texture(7)]
    #[sampler(8)]
    pub t3: Option<Handle<Image>>,
    #[texture(9)]
    #[sampler(10)]
    pub t4: Option<Handle<Image>>,
    #[texture(11)]
    #[sampler(12)]
    pub t5: Option<Handle<Image>>,
    #[texture(13)]
    #[sampler(14)]
    pub t6: Option<Handle<Image>>,
    #[texture(15)]
    #[sampler(16)]
    pub t7: Option<Handle<Image>>,
    #[texture(17)]
    #[sampler(18)]
    pub t8: Option<Handle<Image>>,
    #[texture(19)]
    #[sampler(20)]
    pub t9: Option<Handle<Image>>,
    #[texture(21)]
    #[sampler(22)]
    pub wrap: Option<Handle<Image>>,
    #[texture(23)]
    #[sampler(24)]
    pub clamp: Option<Handle<Image>>,
    pub key: ShadedKey,
}

impl From<&ShadedMaterial> for ShadedKey {
    fn from(m: &ShadedMaterial) -> Self {
        m.key
    }
}

impl Material for ShadedMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("kh3_shaded_vertex.wgsl"))
                .with_source("embedded"),
        )
    }

    fn alpha_mode(&self) -> AlphaMode {
        match self.key.pass {
            1 | 6 => AlphaMode::Add,
            2 | 4 => AlphaMode::Opaque,
            _ => AlphaMode::Blend,
        }
    }

    fn reads_view_transmission_texture(&self) -> bool {
        self.key.pass == 4
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let data = key.bind_group_data;
        let decal = data.pass == 5 || data.pass == 6;
        // A decal draws one side of its box whatever is in front (the shader finds the scene point).
        descriptor.primitive.cull_mode =
            decal.then_some(bevy::render::render_resource::Face::Front);
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            if decal {
                depth.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always);
            }
            depth.depth_write_enabled = Some(data.pass == 2);
        }
        let mut attributes = vec![
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTR_GAME.at_shader_location(1),
            ATTR_UV.at_shader_location(2),
            ATTR_COLOR.at_shader_location(3),
            ATTR_VCOL.at_shader_location(4),
            ATTR_DYNAMIC.at_shader_location(5),
            ATTR_MISC.at_shader_location(6),
            ATTR_NORMAL.at_shader_location(7),
            ATTR_TANGENT.at_shader_location(8),
            ATTR_PARTICLE.at_shader_location(9),
            ATTR_TC1.at_shader_location(10),
        ];
        if data.vertex > 0 {
            attributes.push(ATTR_ROW0.at_shader_location(11));
            attributes.push(ATTR_ROW1.at_shader_location(12));
            attributes.push(ATTR_ROW2.at_shader_location(13));
            if let Ok(table) = SHADERS.read()
                && let Some(shader) = table.get(data.vertex as usize - 1)
            {
                descriptor.vertex.shader = shader.clone();
            }
        }
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        let keep_alpha = visual::glow::keeps_alpha(key.mesh_key);
        // The glow camera's draw: light past white, added.
        let glow = matches!(
            key.mesh_key.target_format(),
            TextureFormat::Rgba16Float | TextureFormat::Rg11b10Ufloat
        );
        if glow && let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if let Ok(table) = SHADERS.read()
                && let Some(shader) = table.get(data.variant as usize)
            {
                fragment.shader = shader.clone();
            }
            if glow {
                fragment.shader_defs.push(match data.pass {
                    0 => "KH_GLOW_BLENDED".into(),
                    3 => "KH_GLOW_NONE".into(),
                    _ => "KH_GLOW".into(),
                });
            }
            let color = match data.pass {
                _ if glow => Some((BlendFactor::One, BlendFactor::One)),
                1 | 6 => Some((BlendFactor::One, BlendFactor::OneMinusSrcAlpha)),
                2 | 4 => None,
                3 => Some((BlendFactor::Dst, BlendFactor::OneMinusSrcAlpha)),
                _ => Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)),
            };
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = color.map(|(src, dst)| BlendState {
                    color: BlendComponent {
                        src_factor: src,
                        dst_factor: dst,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::Zero,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                });
            }
        }
        if keep_alpha {
            visual::glow::keep_alpha(descriptor);
        }
        Ok(())
    }
}

/// One translated pixel shader.
#[derive(Clone, Debug)]
pub struct Variant {
    pub source: String,
    pub textures: BTreeSet<u32>,
}

/// A shader map's material-independent half.
#[derive(Debug, Default)]
struct Owner {
    map: ShaderMap,
    sprite: Option<usize>,
    mesh: Option<usize>,
    mesh_vertex: Option<usize>,
    decal: Option<usize>,
    sprite_distort: Option<usize>,
    mesh_distort: Option<usize>,
}

/// A material's record: the variants it runs and what fills their inputs.
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub sprite: Option<usize>,
    pub mesh: Option<usize>,
    /// A mesh's own vertex shader, where it moves the vertices.
    pub mesh_vertex: Option<usize>,
    pub decal: Option<usize>,
    /// The decal adds its colour instead of blending it.
    pub decal_adds: bool,
    pub sprite_distort: Option<usize>,
    pub mesh_distort: Option<usize>,
    pub cb: Vec<[f32; 4]>,
    /// Constant-buffer places that follow the clock: float index, count, expression.
    pub timed: Vec<(usize, usize, Expr)>,
    pub scalars: HashMap<String, f32>,
    pub vectors: HashMap<String, [f32; 4]>,
    pub textures: Vec<Option<String>>,
    pub clip: f32,
    pub blend: MatBlend,
}

const SPRITE_FACTORIES: [&str; 4] = [
    "FParticleSpriteVertexFactory",
    "FSQEXSquarePolygonVertexFactory",
    "FSQEXTrianglePolygonVertexFactory",
    "FParticleBeamTrailVertexFactory",
];
const MESH_FACTORIES: [&str; 2] = ["FMeshParticleVertexFactory", "FLocalVertexFactory"];
const BASE_PASS: &str = "TBasePassPSFNoLightMapPolicy";
const DEPTH_ONLY: &str = "FDepthOnlyPS";
const DECAL: &str = "FDeferredDecalPS";
const VERTEX: &str = "TBasePassVSFNoLightMapPolicy";
const DISTORT: &str = "TDistortionMeshPS";

/// Material parameters a decal emitter writes every frame: read from the harness's inputs.
fn decal_input(name: &str) -> Option<&'static str> {
    Some(match name {
        "Decal_Color" => "i.dcol",
        "Decal_Opacity" => "i.dcol.w",
        "Decal_CustomParam0" => "i.dprm.x",
        "Decal_CustomParam1" => "i.dprm.y",
        "Decal_CustomParam2" => "i.dprm.z",
        "Decal_ZFadePow" => "i.dz.x",
        "Decal_ZFadeSegment" => "i.dz.y",
        _ => return None,
    })
}

/// A scalar expression over what the emitter drives, in WGSL.
fn decal_scalar(e: &Expr) -> Option<String> {
    match e {
        Expr::Scalar(name, _) => decal_input(name).map(str::to_owned),
        Expr::Constant(v) => Some(format!("{:?}", v[0])),
        Expr::Binary(op, a, b) => {
            let sign = match *op {
                "add" => "+",
                "sub" => "-",
                "mul" => "*",
                "div" => "/",
                _ => return None,
            };
            Some(format!(
                "({} {sign} {})",
                decal_scalar(a)?,
                decal_scalar(b)?
            ))
        }
        _ => None,
    }
}

/// The material buffer's rows a decal emitter drives.
fn decal_rows(map: &ShaderMap) -> HashMap<u32, String> {
    let mut rows = HashMap::new();
    let vectors = map.vectors.len();
    for (i, e) in map.vectors.iter().enumerate() {
        let name = match e {
            Expr::Vector(name, _) => Some(name),
            Expr::Swizzle(x, index, _) if index[..3] == [0, 1, 2] => match &**x {
                Expr::Vector(name, _) => Some(name),
                _ => None,
            },
            _ => None,
        };
        if let Some(input) = name.and_then(|n| decal_input(n)) {
            rows.insert(i as u32, format!("vec4<f32>({input}.xyz, kh.m[{i}].w)"));
        }
    }
    for (r, chunk) in map.scalars.chunks(4).enumerate() {
        let row = vectors + r;
        let parts: Vec<(bool, String)> = (0..4)
            .map(|c| {
                let driven = chunk
                    .get(c)
                    .and_then(|e| decal_scalar(e).filter(|t| t.contains("i.")));
                match driven {
                    Some(t) => (true, t),
                    None => (false, format!("kh.m[{row}].{}", ['x', 'y', 'z', 'w'][c])),
                }
            })
            .collect();
        if parts.iter().any(|p| p.0) {
            let text: Vec<String> = parts.into_iter().map(|p| p.1).collect();
            rows.insert(row as u32, format!("vec4<f32>({})", text.join(", ")));
        }
    }
    rows
}

/// Reads the shaders of the materials.
#[derive(Default)]
pub struct Reader {
    owners: HashMap<String, Option<Arc<Owner>>>,
    pub variants: Vec<Variant>,
    /// Why shaders could not be translated, with how many.
    pub refused: HashMap<String, usize>,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Base,
    Mask,
    Decal,
    Distort,
    Vertex,
}

fn semantics_of(factory: &str) -> HashMap<String, &'static str> {
    if factory != "FParticleBeamTrailVertexFactory" {
        return HashMap::new();
    }
    [
        ("TEXCOORD2", "tc3"),
        ("TEXCOORD3", "tc2"),
        ("TEXCOORD4", "tc1"),
    ]
    .map(|(s, f)| (s.to_owned(), f))
    .into_iter()
    .collect()
}

impl Reader {
    pub fn new(scale: f32) -> Reader {
        Reader {
            scale,
            ..Reader::default()
        }
    }

    fn translate_one(
        map: &ShaderMap,
        factory: &str,
        shader: &shader_map::Shader,
        mode: Mode,
    ) -> Result<translate::Translated, String> {
        let program = dxbc::read(&shader.dxbc)?;
        let (textures, samplers) = translate::resource_tables(
            &shader.buffers,
            &shader.texture_map,
            &shader.sampler_map,
            map.textures_2d.len() as u32,
            map.textures_cube as u32,
        );
        translate::pixel(
            &program,
            &Meta {
                buffers: shader.buffers.clone(),
                textures,
                samplers,
                semantics: semantics_of(factory),
                mask_only: mode == Mode::Mask,
                decal: mode == Mode::Decal,
                distort: mode == Mode::Distort,
                vertex: mode == Mode::Vertex,
                rows: if mode == Mode::Decal {
                    decal_rows(map)
                } else {
                    HashMap::new()
                },
            },
        )
    }

    fn refuse(&mut self, why: String) {
        *self.refused.entry(why).or_default() += 1;
    }

    fn push(&mut self, source: String, textures: BTreeSet<u32>) -> usize {
        self.variants.push(Variant { source, textures });
        self.variants.len() - 1
    }

    /// Translates a class's shader; `None` with the reason counted.
    fn variant(&mut self, map: &ShaderMap, factories: &[&str], mesh: bool) -> Option<usize> {
        let set = factories.iter().find_map(|f| {
            map.mesh
                .iter()
                .find(|m| m.factory == *f && m.shaders.iter().any(|s| s.kind == BASE_PASS))
        })?;
        let shader = set.shaders.iter().find(|s| s.kind == BASE_PASS)?;
        let t = match Self::translate_one(map, &set.factory, shader, Mode::Base) {
            Ok(t) => t,
            Err(why) => {
                self.refuse(why);
                return None;
            }
        };
        let mut textures = t.textures.clone();
        // A masked material clips in its depth-only pass, which the base pass relies on.
        let mask = map
            .description
            .contains("BlendMode=BLEND_Masked")
            .then(|| set.shaders.iter().find(|s| s.kind == DEPTH_ONLY))
            .flatten()
            .and_then(|s| Self::translate_one(map, &set.factory, s, Mode::Mask).ok())
            .filter(|m| m.wgsl.contains("discard"));
        if let Some(m) = &mask {
            textures.extend(m.textures.iter().copied());
        }
        let parts = kh3_shader::Parts {
            material: &t.wgsl,
            mask: mask.as_ref().map(|m| m.wgsl.as_str()),
            mesh,
            deferred: t.outputs >= 4,
            kind: kh3_shader::Kind::Base,
            scale: self.scale,
        };
        if let Err(why) = kh3_shader::validate(parts) {
            let first = why.lines().next().unwrap_or("").to_owned();
            self.refuse(format!("invalid: {first}"));
            return None;
        }
        Some(self.push(kh3_shader::fragment_source(parts), textures))
    }

    /// The decal's shader: a fragment shader of its own over the box.
    fn decal_variant(&mut self, map: &ShaderMap) -> Option<usize> {
        let shader = map.top.iter().find(|s| s.kind == DECAL)?;
        let t = match Self::translate_one(map, "", shader, Mode::Decal) {
            Ok(t) => t,
            Err(why) => {
                self.refuse(format!("decal: {why}"));
                return None;
            }
        };
        let parts = kh3_shader::Parts {
            material: &t.wgsl,
            kind: kh3_shader::Kind::Decal,
            scale: self.scale,
            ..kh3_shader::Parts::default()
        };
        if let Err(why) = kh3_shader::validate(parts) {
            let first = why.lines().next().unwrap_or("").to_owned();
            self.refuse(format!("decal invalid: {first}"));
            return None;
        }
        Some(self.push(kh3_shader::fragment_source(parts), t.textures))
    }

    fn distort_variant(
        &mut self,
        map: &ShaderMap,
        factories: &[&str],
        mesh: bool,
    ) -> Option<usize> {
        if !map.description.contains("Distorted=1") {
            return None;
        }
        let set = factories.iter().find_map(|f| {
            map.mesh
                .iter()
                .find(|m| m.factory == *f && m.shaders.iter().any(|s| s.kind.starts_with(DISTORT)))
        })?;
        let shader = set.shaders.iter().find(|s| s.kind.starts_with(DISTORT))?;
        let t = match Self::translate_one(map, &set.factory, shader, Mode::Distort) {
            Ok(t) => t,
            Err(why) => {
                self.refuse(format!("distortion: {why}"));
                return None;
            }
        };
        let parts = kh3_shader::Parts {
            material: &t.wgsl,
            mesh,
            kind: kh3_shader::Kind::Distort,
            scale: self.scale,
            ..kh3_shader::Parts::default()
        };
        if let Err(why) = kh3_shader::validate(parts) {
            let first = why.lines().next().unwrap_or("").to_owned();
            self.refuse(format!("distortion invalid: {first}"));
            return None;
        }
        Some(self.push(kh3_shader::fragment_source(parts), t.textures))
    }

    /// The mesh particle's own vertex shader, for a material whose shader moves the vertices.
    fn vertex_variant(&mut self, map: &ShaderMap) -> Option<usize> {
        if !map.description.contains("ModifiesMeshPosition=1") {
            return None;
        }
        let set = map
            .mesh
            .iter()
            .find(|m| m.factory == "FMeshParticleVertexFactory")?;
        let shader = set.shaders.iter().find(|s| s.kind == VERTEX)?;
        let t = match Self::translate_one(map, &set.factory, shader, Mode::Vertex) {
            Ok(t) => t,
            Err(why) => {
                self.refuse(format!("vertex: {why}"));
                return None;
            }
        };
        if let Err(why) = kh3_shader::validate_vertex(&t.wgsl, self.scale) {
            let first = why.lines().next().unwrap_or("").to_owned();
            self.refuse(format!("vertex invalid: {first}"));
            return None;
        }
        Some(self.push(kh3_shader::vertex_source(&t.wgsl, self.scale), t.textures))
    }

    /// The nearest package of a material's chain that owns a shader map.
    fn owner(&mut self, paks: &Paks, path: &str) -> (Option<Arc<Owner>>, Option<Arc<Package>>) {
        let mut at = path.to_owned();
        let mut owner = None;
        let mut last = None;
        for _ in 0..16 {
            let Ok(Some(p)) = paks.game_package(&at) else {
                break;
            };
            let p = Arc::new(p);
            let Some(i) = p.export_of_classes(&["MaterialInstanceConstant", "Material"]) else {
                break;
            };
            if owner.is_none() {
                owner = match self.owners.get(&at) {
                    Some(known) => known.clone(),
                    None => {
                        let read = shader_map::read(
                            &p,
                            i,
                            &|f| SPRITE_FACTORIES.contains(&f) || MESH_FACTORIES.contains(&f),
                            &|k| {
                                k == BASE_PASS
                                    || k == DEPTH_ONLY
                                    || k == DECAL
                                    || k == VERTEX
                                    || k.starts_with(DISTORT)
                            },
                        );
                        let made = match read {
                            Ok(Some(map)) => {
                                let sprite = self.variant(&map, &SPRITE_FACTORIES, false);
                                let mesh = self.variant(&map, &MESH_FACTORIES, true);
                                let mesh_vertex = self.vertex_variant(&map);
                                let decal = self.decal_variant(&map);
                                let sprite_distort =
                                    self.distort_variant(&map, &SPRITE_FACTORIES, false);
                                let mesh_distort =
                                    self.distort_variant(&map, &MESH_FACTORIES, true);
                                Some(Arc::new(Owner {
                                    map,
                                    sprite,
                                    mesh,
                                    mesh_vertex,
                                    decal,
                                    sprite_distort,
                                    mesh_distort,
                                }))
                            }
                            Ok(None) => None,
                            Err(why) => {
                                *self.refused.entry(format!("map: {why}")).or_default() += 1;
                                None
                            }
                        };
                        self.owners.insert(at.clone(), made.clone());
                        made
                    }
                };
            }
            let parent = kh3_data::props::export_props(&p, i)
                .ok()
                .and_then(|(props, _)| props.object("Parent"))
                .and_then(|o| kh3_data::material::import_path(&p, o));
            last = Some(p);
            match parent {
                Some(next) => at = next,
                None => break,
            }
        }
        (owner, last)
    }

    /// A material's record, if a shader map owns it and a shader of it translated.
    pub fn record(&mut self, paks: &Paks, path: &str, m: &MaterialParams) -> Option<Record> {
        let (owner, root) = self.owner(paks, path);
        let owner = owner?;
        if owner.sprite.is_none() && owner.mesh.is_none() && owner.decal.is_none() {
            return None;
        }
        let refs = root
            .as_ref()
            .map_or_else(Vec::new, |r| expression_textures(paks, r));
        let decal_adds = root.as_ref().is_some_and(|r| {
            r.export_of_class("Material")
                .and_then(|i| kh3_data::props::export_props(r, i).ok())
                .is_some_and(|(p, _)| {
                    p.str("DecalBlendMode")
                        .is_some_and(|b| b.contains("DBM_Emissive"))
                })
        });
        let scalars: HashMap<String, f32> =
            m.scalars.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let vectors: HashMap<String, [f32; 4]> =
            m.vectors.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let mut record = Record {
            sprite: owner.sprite,
            mesh: owner.mesh,
            mesh_vertex: owner.mesh_vertex,
            decal: owner.decal,
            decal_adds,
            sprite_distort: owner.sprite_distort,
            mesh_distort: owner.mesh_distort,
            scalars,
            vectors,
            blend: m.blend,
            clip: m
                .scalars
                .get("OpacityMaskClipValue")
                .copied()
                .unwrap_or(0.333),
            ..Record::default()
        };
        let mut flat: Vec<f32> = Vec::new();
        for e in &owner.map.vectors {
            if e.uses_time() {
                record.timed.push((flat.len(), 4, e.clone()));
            }
            flat.extend(eval(e, 0., &record));
        }
        for e in &owner.map.scalars {
            if e.uses_time() {
                record.timed.push((flat.len(), 1, e.clone()));
            }
            flat.push(eval(e, 0., &record)[0]);
        }
        while !flat.len().is_multiple_of(4) {
            flat.push(0.);
        }
        record.cb = flat.as_chunks::<4>().0.iter().take(64).copied().collect();
        record.textures = owner
            .map
            .textures_2d
            .iter()
            .map(|e| {
                let Expr::Texture { parameter, index } = e else {
                    return None;
                };
                parameter
                    .as_ref()
                    .and_then(|n| m.textures.get(n).cloned())
                    .or_else(|| {
                        usize::try_from(*index)
                            .ok()
                            .and_then(|i| refs.get(i))
                            .cloned()
                            .flatten()
                    })
            })
            .collect();
        Some(record)
    }
}

impl Record {
    /// Every shader it runs.
    pub fn variants(&self) -> impl Iterator<Item = usize> {
        [
            self.sprite,
            self.mesh,
            self.mesh_vertex,
            self.decal,
            self.sprite_distort,
            self.mesh_distort,
        ]
        .into_iter()
        .flatten()
    }
}

/// An expression with a material's parameters.
pub fn eval(e: &Expr, time: f32, r: &Record) -> [f32; 4] {
    e.eval(time, &|n| r.scalars.get(n).copied(), &|n| {
        r.vectors.get(n).copied()
    })
}

fn object_of(v: &Value) -> Option<i32> {
    v.as_object()
}

fn class_takes_default(class: &str) -> bool {
    matches!(
        class,
        "MaterialExpressionTextureObject"
            | "MaterialExpressionTextureObjectParameter"
            | "MaterialExpressionTextureSampleParameter2D"
    )
}

/// A list's textures, in the order the engine builds them: each new one once, and a slot for the
/// engine's default where an expression names none.
fn add_textures(p: &Package, list: &[Value], out: &mut Vec<Option<String>>) {
    for v in list {
        let Some(i) = object_of(v).and_then(|o| usize::try_from(o - 1).ok()) else {
            continue;
        };
        if i >= p.exports.len() || p.class_of(i).ends_with("FunctionCall") {
            continue;
        }
        let Ok((node, _)) = kh3_data::props::export_props(p, i) else {
            continue;
        };
        match node
            .object("Texture")
            .filter(|t| *t < 0)
            .and_then(|t| kh3_data::material::import_path(p, t))
        {
            Some(path) => {
                if !out.iter().flatten().any(|x| *x == path) {
                    out.push(Some(path));
                }
            }
            None => {
                if node.object("Texture").is_none()
                    && class_takes_default(p.class_of(i))
                    && !out.iter().any(Option::is_none)
                {
                    out.push(None);
                }
            }
        }
    }
}

fn function_of(paks: &Paks, path: &str) -> Option<(Package, usize)> {
    let p = paks.game_package(path).ok()??;
    let i = p.export_of_class("MaterialFunction")?;
    Some((p, i))
}

/// The functions a function calls, nested first.
fn dependents(paks: &Paks, path: &str, list: &mut Vec<String>, depth: u8) {
    let Some((p, i)) = function_of(paks, path).filter(|_| depth < 8) else {
        return;
    };
    let Ok((props, _)) = kh3_data::props::export_props(&p, i) else {
        return;
    };
    for v in props.array("FunctionExpressions").unwrap_or(&[]) {
        let Some(e) = object_of(v).and_then(|o| usize::try_from(o - 1).ok()) else {
            continue;
        };
        if e >= p.exports.len() || !p.class_of(e).ends_with("FunctionCall") {
            continue;
        }
        let Some(next) = kh3_data::props::export_props(&p, e)
            .ok()
            .and_then(|(n, _)| n.object("MaterialFunction"))
            .and_then(|o| kh3_data::material::import_path(&p, o))
        else {
            continue;
        };
        dependents(paks, &next, list, depth + 1);
        if !list.contains(&next) {
            list.push(next);
        }
    }
}

/// `UMaterial::RebuildExpressionTextureReferences`: a uniform texture expression's index is a place
/// in this list.
fn expression_textures(paks: &Paks, root: &Package) -> Vec<Option<String>> {
    let mut out = Vec::new();
    let Some(mat) = root.export_of_class("Material") else {
        return out;
    };
    let Ok((props, _)) = kh3_data::props::export_props(root, mat) else {
        return out;
    };
    for v in props.array("Expressions").unwrap_or(&[]) {
        let Some(e) = object_of(v).and_then(|o| usize::try_from(o - 1).ok()) else {
            continue;
        };
        if e >= root.exports.len() {
            continue;
        }
        if root.class_of(e).ends_with("FunctionCall") {
            let Some(path) = kh3_data::props::export_props(root, e)
                .ok()
                .and_then(|(n, _)| n.object("MaterialFunction"))
                .and_then(|o| kh3_data::material::import_path(root, o))
            else {
                continue;
            };
            let mut list = vec![path.clone()];
            dependents(paks, &path, &mut list, 0);
            for f in list {
                if let Some((p, i)) = function_of(paks, &f)
                    && let Ok((n, _)) = kh3_data::props::export_props(&p, i)
                {
                    add_textures(&p, n.array("FunctionExpressions").unwrap_or(&[]), &mut out);
                }
            }
        } else {
            add_textures(root, std::slice::from_ref(v), &mut out);
        }
    }
    out
}

fn is_srgb(t: &Texture) -> bool {
    !matches!(t.format.as_str(), "PF_BC4" | "PF_BC5" | "PF_G8")
        && t.props.bool("SRGB").unwrap_or(true)
}

fn address(t: &Texture, name: &str) -> ImageAddressMode {
    match t.props.str(name) {
        Some(mode) if mode.ends_with("TA_Clamp") => ImageAddressMode::ClampToEdge,
        Some(mode) if mode.ends_with("TA_Mirror") => ImageAddressMode::MirrorRepeat,
        _ => ImageAddressMode::Repeat,
    }
}

/// A texture as the shaders read it: colour decoded from sRGB, its top mip (they always read mip 0).
pub fn image_of(t: &Texture) -> Option<Image> {
    let mip = t
        .mips
        .iter()
        .find(|m| !m.bytes.is_empty() && m.width.max(m.height) <= 1024)
        .or(t.mips.iter().find(|m| !m.bytes.is_empty()))?;
    let srgb = is_srgb(t);
    let aligned = mip.width % 4 == 0 && mip.height % 4 == 0;
    let stored = match t.format.as_str() {
        "PF_DXT1" if aligned => Some(if srgb {
            TextureFormat::Bc1RgbaUnormSrgb
        } else {
            TextureFormat::Bc1RgbaUnorm
        }),
        "PF_DXT5" if aligned => Some(if srgb {
            TextureFormat::Bc3RgbaUnormSrgb
        } else {
            TextureFormat::Bc3RgbaUnorm
        }),
        "PF_BC7" if aligned => Some(if srgb {
            TextureFormat::Bc7RgbaUnormSrgb
        } else {
            TextureFormat::Bc7RgbaUnorm
        }),
        "PF_BC5" if aligned => Some(TextureFormat::Bc5RgUnorm),
        "PF_BC4" if aligned => Some(TextureFormat::Bc4RUnorm),
        "PF_G8" => Some(TextureFormat::R8Unorm),
        "PF_B8G8R8A8" => Some(if srgb {
            TextureFormat::Bgra8UnormSrgb
        } else {
            TextureFormat::Bgra8Unorm
        }),
        _ => None,
    };
    let size = Extent3d {
        width: mip.width,
        height: mip.height,
        depth_or_array_layers: 1,
    };
    let mut image = match stored {
        Some(format) => Image::new(
            size,
            TextureDimension::D2,
            mip.bytes.clone(),
            format,
            RenderAssetUsages::RENDER_WORLD,
        ),
        None => Image::new(
            size,
            TextureDimension::D2,
            kh3_ui::gfx::decode_texture(&t.format, mip.width, mip.height, &mip.bytes)?,
            if srgb {
                TextureFormat::Rgba8UnormSrgb
            } else {
                TextureFormat::Rgba8Unorm
            },
            RenderAssetUsages::RENDER_WORLD,
        ),
    };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address(t, "AddressX"),
        address_mode_v: address(t, "AddressY"),
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    Some(image)
}

/// A one-texel image that only carries a shared sampler.
fn sampler_image(mode: ImageAddressMode) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![255; 4],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// The translated shaders, handed to the pipeline cache.
pub fn register_variants(variants: &[Variant], shaders: &mut Assets<Shader>) {
    if let Ok(mut table) = SHADERS.write() {
        table.clear();
        for (i, v) in variants.iter().enumerate() {
            table.push(shaders.add(Shader::from_wgsl(
                v.source.clone(),
                format!("kh3_shaded/{i}.wgsl"),
            )));
        }
    }
}

/// A material's asset for a draw.
#[allow(clippy::too_many_arguments)]
pub fn make_material(
    record: &Record,
    variant: usize,
    vertex: Option<usize>,
    pass: Option<u32>,
    used: &BTreeSet<u32>,
    images: &HashMap<String, Handle<Image>>,
    shared: &(Handle<Image>, Handle<Image>),
    additive_override: bool,
    reverse_override: bool,
) -> ShadedMaterial {
    let mut m = [Vec4::ZERO; 64];
    for (row, v) in m.iter_mut().zip(&record.cb) {
        *row = Vec4::from_array(*v);
    }
    let tex = |slot: u32| -> Option<Handle<Image>> {
        used.contains(&slot)
            .then(|| record.textures.get(slot as usize).cloned().flatten())
            .flatten()
            .and_then(|path| images.get(&path).cloned())
    };
    let pass = pass.unwrap_or(match (additive_override, reverse_override, record.blend) {
        (true, ..) => 1,
        (_, true, _) => 0,
        (_, _, MatBlend::Additive) => 1,
        (_, _, MatBlend::Opaque | MatBlend::Masked) => 2,
        (_, _, MatBlend::Modulate) => 3,
        _ => 0,
    });
    ShadedMaterial {
        params: KhParams {
            m,
            misc: Vec4::new(
                record.clip,
                0.,
                f32::from(u8::from(additive_override)),
                f32::from(u8::from(reverse_override)),
            ),
        },
        t0: tex(0),
        t1: tex(1),
        t2: tex(2),
        t3: tex(3),
        t4: tex(4),
        t5: tex(5),
        t6: tex(6),
        t7: tex(7),
        t8: tex(8),
        t9: tex(9),
        wrap: Some(shared.0.clone()),
        clamp: Some(shared.1.clone()),
        key: ShadedKey {
            variant: variant as u32,
            pass,
            vertex: vertex.map_or(0, |v| v as u32 + 1),
        },
    }
}

/// The shared wrap and clamp samplers' images.
pub fn shared_samplers(images: &mut Assets<Image>) -> (Handle<Image>, Handle<Image>) {
    (
        images.add(sampler_image(ImageAddressMode::Repeat)),
        images.add(sampler_image(ImageAddressMode::ClampToEdge)),
    )
}

/// The plugin's half: the vertex stage's file and the material's pipeline.
pub struct ShadedPlugin;

impl Plugin for ShadedPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "kh3_shaded_vertex.wgsl");
        app.add_plugins(bevy::pbr::MaterialPlugin::<ShadedMaterial>::default());
    }
}

/// One vertex of a shaded draw.
#[derive(Default, Clone)]
pub struct ShadedVerts {
    pub position: Vec<[f32; 3]>,
    pub game: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 4]>,
    pub color: Vec<[f32; 4]>,
    pub vcol: Vec<[f32; 4]>,
    pub dynamic: Vec<[f32; 4]>,
    pub misc: Vec<[f32; 4]>,
    pub normal: Vec<[f32; 3]>,
    pub tangent: Vec<[f32; 4]>,
    pub particle: Vec<[f32; 4]>,
    pub tc1: Vec<[f32; 4]>,
    /// A translated vertex shader's instance rows.
    pub rows: [Vec<[f32; 4]>; 3],
    pub indices: Vec<u32>,
}

impl ShadedVerts {
    /// Puts the vertices into a mesh, about its middle (the draw's sort place), and returns that.
    pub fn store(self, mesh: &mut Mesh) -> Vec3 {
        let Self {
            mut position,
            game,
            uv,
            color,
            vcol,
            dynamic,
            misc,
            normal,
            tangent,
            particle,
            tc1,
            rows,
            indices,
        } = self;
        let n = position.len().max(1) as f32;
        let mut middle = Vec3::ZERO;
        for p in &position {
            middle += Vec3::from_array(*p);
        }
        middle /= n;
        for p in &mut position {
            *p = (Vec3::from_array(*p) - middle).to_array();
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, position);
        mesh.insert_attribute(ATTR_GAME, game);
        mesh.insert_attribute(ATTR_UV, uv);
        mesh.insert_attribute(ATTR_COLOR, color);
        mesh.insert_attribute(ATTR_VCOL, vcol);
        mesh.insert_attribute(ATTR_DYNAMIC, dynamic);
        mesh.insert_attribute(ATTR_MISC, misc);
        mesh.insert_attribute(ATTR_NORMAL, normal);
        mesh.insert_attribute(ATTR_TANGENT, tangent);
        mesh.insert_attribute(ATTR_PARTICLE, particle);
        mesh.insert_attribute(ATTR_TC1, tc1);
        let [row0, row1, row2] = rows;
        if !row0.is_empty() {
            mesh.insert_attribute(ATTR_ROW0, row0);
            mesh.insert_attribute(ATTR_ROW1, row1);
            mesh.insert_attribute(ATTR_ROW2, row2);
        }
        mesh.insert_indices(bevy::mesh::Indices::U32(indices));
        middle
    }
}
