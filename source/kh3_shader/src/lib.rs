//! KH3's compiled effect shaders as WGSL. A cooked material's graph has no math left: the full truth
//! is its compiled D3D11 bytecode, which [`dxbc`] reads and [`translate`] turns into a WGSL function.
//! Everything is read from the player's own install at run time; nothing from it is in this
//! repository. Ported from StrawHatPro's `tools/sora_vfx_shaders.js` and `tools/lib/dxbc_disasm.js`
//! (MIT; see NOTICE).

pub mod dxbc;
pub mod translate;

const HARNESS: &str = include_str!("harness.wgsl");
const HARNESS_VERTEX: &str = include_str!("harness_vertex.wgsl");
const BEVY_VIEW: &str = "#import bevy_pbr::mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler}
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils::prepass_depth
#endif";
const BEVY_DEPTH: &str = "fn kh_scene_device(p: vec4<f32>) -> f32 {
#ifdef DEPTH_PREPASS
    return prepass_depth(p, 0u);
#else
    return 0.0;
#endif
}";
const STUB_VIEW: &str = "struct KhView { world_from_view: mat4x4<f32>, clip_from_view: mat4x4<f32>, clip_from_world: mat4x4<f32>, viewport: vec4<f32> }
@group(0) @binding(0) var<uniform> view: KhView;
@group(0) @binding(24) var view_transmission_texture: texture_2d<f32>;
@group(0) @binding(25) var view_transmission_sampler: sampler;";
const STUB_DEPTH: &str = "fn kh_scene_device(p: vec4<f32>) -> f32 { return 0.0; }";

// On the glow camera: only what is past white, as the tone curve would have it; the rest is in the frame.
const GLOW_BLOCK: &str = "#ifdef KH_GLOW_BLENDED
    return vec4<f32>(max(color.xyz * saturate(color.w) - vec3<f32>(1.0), vec3<f32>(0.0)), 0.0);
#endif
#ifdef KH_GLOW_NONE
    return vec4<f32>(0.0);
#endif
#ifdef KH_GLOW
    return vec4<f32>(max(color.xyz - vec3<f32>(1.0), vec3<f32>(0.0)), 0.0);
#endif";

const MESH_BLOCK: &str = "if (in.misc.z < -0.5) {
        let s = kh_mesh_subuv(in.uv.xy, in.misc.z);
        i.tc1 = s.sub0;
        i.tc2 = s.sub1;
        i.tc10.w = 0.0;
    }";

// A lit material's emissive plus its base colour under a key light and an ambient term.
const DEFERRED_BLOCK: &str = "let lit_normal = normalize(o.o1.xyz * 2.0 - vec3<f32>(1.0));
    let lit_light = normalize(vec3<f32>(0.35, 0.25, 0.9));
    let lit = 0.45 + 0.75 * max(dot(lit_normal, lit_light), 0.0);
    color = vec4<f32>(o.o0.xyz + o.o3.xyz * lit, 1.0);";

// The decal's box is drawn; what it shades is the scene point behind each pixel, taken into the box's
// own space (the rows of the world to decal matrix ride in the vertex colour, dynamic and particle).
const DECAL_BLOCK: &str = "if (scene_depth <= 0.0) {
        discard;
    }
    let g = camera + i.twp * (scene_depth / dot(i.twp, forward));
    // KH3's characters take no decals: what stands above the floor keeps its pixels.
    if (g.z > in.tangent.w + 12.0) {
        discard;
    }
    i.decal_pos = vec3<f32>(dot(in.vcol.xyz, g) + in.vcol.w, dot(in.dynamic.xyz, g) + in.dynamic.w, dot(in.particle.xyz, g) + in.particle.w);
    i.dcol = in.color;
    i.dprm = vec4<f32>(in.misc.yzw, 0.0);
    i.dz = vec4<f32>(in.uv.zw, 0.0, 0.0);
    i.d12 = vec4<f32>(1.0);
    i.d23 = vec4<f32>(in.misc.x);";

// The material's distortion pass: the scene behind the surface, read at the offset its shader asks for.
const DISTORT_BLOCK: &str = "let offset = (o.o0.xy - o.o0.zw) * 0.25;
    let scene_uv = clamp(in.position.xy * size.zw + offset, vec2<f32>(0.0), vec2<f32>(1.0));
    return vec4<f32>(textureSampleLevel(view_transmission_texture, view_transmission_sampler, scene_uv, 0.0).xyz, 1.0);";

/// What kind of pass a fragment shader is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Base,
    Decal,
    Distort,
}

/// The parts of a fragment shader.
#[derive(Clone, Copy, Debug, Default)]
pub struct Parts<'a> {
    pub material: &'a str,
    pub mask: Option<&'a str>,
    pub mesh: bool,
    pub deferred: bool,
    pub kind: Kind,
    /// Bevy's lengths to KH3's centimetres.
    pub scale: f32,
}

fn assemble(parts: Parts, view: &str, depth: &str, glow: &str) -> String {
    let mut generated = format!(
        "const KH_SCALE: f32 = {:e};\nfn kh_material(i: KhIn) -> KhOut {{\n{}\n}}\n",
        parts.scale, parts.material
    );
    if let Some(mask) = parts.mask {
        generated.push_str(&format!("fn kh_mask(i: KhIn) {{\n{mask}\n}}\n"));
    }
    HARNESS
        .replace("//KH_VIEW", view)
        .replace("//KH_DEPTH_FN", depth)
        .replace("//KH_GLOW", glow)
        .replace("//KH_GENERATED", &generated)
        .replace("//KH_MESH", if parts.mesh { MESH_BLOCK } else { "" })
        .replace(
            "//KH_MASK_CALL",
            if parts.mask.is_some() {
                "kh_mask(i);"
            } else {
                ""
            },
        )
        .replace(
            "//KH_DEFERRED",
            if parts.deferred { DEFERRED_BLOCK } else { "" },
        )
        .replace(
            "//KH_DECAL",
            if parts.kind == Kind::Decal {
                DECAL_BLOCK
            } else {
                ""
            },
        )
        .replace(
            "//KH_DISTORT",
            if parts.kind == Kind::Distort {
                DISTORT_BLOCK
            } else {
                ""
            },
        )
}

fn assemble_vertex(body: &str, scale: f32, view: &str) -> String {
    let generated = format!(
        "const KH_SCALE: f32 = {scale:e};\nfn kh_vertex(i: KhVsIn) -> KhVsOut {{\n{body}\n}}\n"
    );
    HARNESS_VERTEX
        .replace("//KH_IMPORTS", view)
        .replace("//KH_GENERATED", &generated)
}

/// The shader, as Bevy takes it.
pub fn fragment_source(parts: Parts) -> String {
    assemble(parts, BEVY_VIEW, BEVY_DEPTH, GLOW_BLOCK)
}

/// A translated vertex shader, as Bevy takes it.
pub fn vertex_source(body: &str, scale: f32) -> String {
    assemble_vertex(body, scale, "#import bevy_pbr::mesh_view_bindings::view")
}

fn check(source: &str) -> Result<(), String> {
    let source = source.replace("#{MATERIAL_BIND_GROUP}", "2");
    let module = naga::front::wgsl::parse_str(&source).map_err(|e| e.message().to_owned())?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map(|_| ())
    .map_err(|e| e.into_inner().to_string())
}

/// Whether the shader is valid WGSL: a translated shader that is not stays on the generic path, and
/// never reaches the pipeline cache.
pub fn validate(parts: Parts) -> Result<(), String> {
    check(&assemble(parts, STUB_VIEW, STUB_DEPTH, ""))
}

/// The same for a vertex shader.
pub fn validate_vertex(body: &str, scale: f32) -> Result<(), String> {
    check(&assemble_vertex(body, scale, STUB_VIEW))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_harness_takes_a_translated_body() {
        let parts = Parts {
            material: "    var o0: vec4<f32>;\n    o0 = i.col1;\n    return KhOut(o0, vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));",
            scale: 1.,
            ..Parts::default()
        };
        validate(parts).unwrap();
    }

    #[test]
    fn the_mesh_mask_and_lit_parts_are_valid_too() {
        let parts = Parts {
            material: "    return KhOut(textureSampleLevel(kh_t0, kh_swrap, i.tc0.xy, 0.0), vec4<f32>(0.5), vec4<f32>(0.0), vec4<f32>(1.0));",
            mask: Some("    if (i.col0.x == 0.0) { discard; }"),
            mesh: true,
            deferred: true,
            scale: 2.5,
            ..Parts::default()
        };
        validate(parts).unwrap();
    }

    #[test]
    fn the_decal_and_distortion_parts_are_valid() {
        for kind in [Kind::Decal, Kind::Distort] {
            let parts = Parts {
                material: "    return KhOut(vec4<f32>(i.decal_pos, i.dist12.x), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));",
                scale: 1.,
                kind,
                ..Parts::default()
            };
            validate(parts).unwrap();
        }
    }

    #[test]
    fn the_vertex_harness_takes_a_translated_body() {
        let body = "    var kh_out: KhVsOut;\n    kh_out.tc0 = i.a4;\n    kh_out.wpos = (i.a8 * i.a0).xyz + i.v58.xyz;\n    return kh_out;";
        validate_vertex(body, 2.5).unwrap();
    }

    #[test]
    fn a_body_that_is_not_wgsl_is_refused() {
        let parts = Parts {
            material: "    return nothing;",
            scale: 1.,
            ..Parts::default()
        };
        assert!(validate(parts).is_err());
    }
}
