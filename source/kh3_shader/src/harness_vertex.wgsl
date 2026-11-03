// The vertex stage of a translated KH3 mesh-particle vertex shader: `kh_vertex` is the game's own, which
// does the instance transform, the UV turn, the world position offset and the interpolants; this only
// feeds it and projects its result. The inputs' values are StrawHatPro's harness (MIT).
//KH_IMPORTS

struct KhParams {
    m: array<vec4<f32>, 64>,
    misc: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> kh: KhParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var kh_t0: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var kh_s0: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var kh_t1: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var kh_s1: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var kh_t2: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var kh_s2: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var kh_t3: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var kh_s3: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var kh_t4: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var kh_s4: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var kh_t5: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var kh_s5: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(13) var kh_t6: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(14) var kh_s6: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(15) var kh_t7: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(16) var kh_s7: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(17) var kh_t8: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(18) var kh_s8: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(19) var kh_t9: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(20) var kh_s9: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(21) var kh_twrap: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(22) var kh_swrap: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(23) var kh_tclamp: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(24) var kh_sclamp: sampler;

struct KhVsIn {
    a0: vec4<f32>, a1: vec4<f32>, a2: vec4<f32>, a3: vec4<f32>, a4: vec4<f32>, a5: vec4<f32>, a6: vec4<f32>, a7: vec4<f32>,
    a8: vec4<f32>, a9: vec4<f32>, a10: vec4<f32>, a11: vec4<f32>, a12: vec4<f32>, a13: vec4<f32>, a14: vec4<f32>, a15: vec4<f32>,
    mp0: vec4<f32>, mp1: vec4<f32>, mp2: vec4<f32>, mp3: vec4<f32>, mp4: vec4<f32>, mp5: vec4<f32>,
    p0: vec4<f32>, p1: vec4<f32>, p2: vec4<f32>, p3: vec4<f32>, p8: vec4<f32>, p9: vec4<f32>,
    v8: vec4<f32>, v9: vec4<f32>, v10: vec4<f32>, v11: vec4<f32>, v12: vec4<f32>, v13: vec4<f32>, v14: vec4<f32>, v15: vec4<f32>,
    v32: vec4<f32>, v33: vec4<f32>, v34: vec4<f32>, v35: vec4<f32>, v48: vec4<f32>, v55: vec4<f32>, v56: vec4<f32>, v58: vec4<f32>,
    v128: vec4<f32>, v161: vec4<f32>,
}

struct KhVsOut {
    tc10: vec4<f32>, tc11: vec4<f32>, tc0: vec4<f32>, tc1: vec4<f32>,
    col0: vec4<f32>, col1: vec4<f32>, col2: vec4<f32>, col3: vec4<f32>,
    wpos: vec3<f32>,
}

//KH_GENERATED

struct Vertex {
    // Only for the draw's sort place.
    @location(0) position: vec3<f32>,
    // The mesh's own vertex.
    @location(1) local: vec3<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) vcol: vec4<f32>,
    @location(5) dynamic: vec4<f32>,
    // x: emitter seconds, y: relative time, z: the UV selectors, w: the sub-UV code
    @location(6) imisc: vec4<f32>,
    @location(7) normal: vec3<f32>,
    // w: the sign of the binormal
    @location(8) tangent: vec4<f32>,
    // xyz: the floor's height in x
    @location(9) particle: vec4<f32>,
    @location(10) tc1: vec4<f32>,
    @location(11) row0: vec4<f32>,
    @location(12) row1: vec4<f32>,
    @location(13) row2: vec4<f32>,
}

struct KhVaryings {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) vcol: vec4<f32>,
    @location(4) dynamic: vec4<f32>,
    @location(5) misc: vec4<f32>,
    @location(6) normal: vec3<f32>,
    @location(7) tangent: vec4<f32>,
    @location(8) particle: vec4<f32>,
    @location(9) tc1: vec4<f32>,
}

@vertex
fn vertex(v: Vertex) -> KhVaryings {
    var out: KhVaryings;
    let w = view.world_from_view;
    let camera = w[3].xzy * KH_SCALE;
    let seconds = v.imisc.x;
    var i: KhVsIn;
    i.a0 = vec4<f32>(v.local, 1.0);
    i.a1 = vec4<f32>(v.tangent.xyz * 0.5 + vec3<f32>(0.5), 0.0);
    i.a2 = vec4<f32>(v.normal * 0.5 + vec3<f32>(0.5), v.tangent.w * 0.5 + 0.5);
    i.a3 = v.vcol;
    i.a4 = vec4<f32>(v.uv.xy, 0.0, 0.0);
    i.a5 = vec4<f32>(v.uv.zw, 0.0, 0.0);
    i.a6 = i.a5;
    i.a7 = i.a5;
    i.a8 = v.row0;
    i.a9 = v.row1;
    i.a10 = v.row2;
    i.a12 = vec4<f32>(0.0, v.imisc.y, 0.0, 0.0);
    i.a13 = v.dynamic;
    i.a14 = v.color;
    i.a15 = vec4<f32>(0.0);
    i.mp2 = vec4<f32>(1.0);
    let code = u32(v.imisc.z + 0.5);
    i.mp3 = vec4<f32>(f32(code & 3u), f32((code >> 2u) & 1u), f32((code >> 3u) & 1u), 0.0);
    i.mp5 = vec4<f32>(0.0, seconds, 0.0, 0.0);
    i.p9 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    i.v48 = vec4<f32>(-w[2].xzy, 0.0);
    i.v55 = vec4<f32>(camera, 0.0);
    i.v58 = vec4<f32>(-camera, 0.0);
    i.v128 = vec4<f32>(seconds);
    i.v161 = vec4<f32>(1.0, 1.0, seconds, 0.0);
    let o = kh_vertex(i);
    let world = o.wpos + camera;
    out.world_position = world;
    out.position = view.clip_from_world * vec4<f32>(world.xzy / KH_SCALE, 1.0);
    out.uv = o.tc0;
    out.tc1 = o.tc1;
    out.color = o.col1;
    out.vcol = o.col0;
    out.dynamic = o.col2;
    out.misc = vec4<f32>(o.col3.z, v.imisc.y, o.tc10.w, o.tc11.w);
    out.normal = o.tc11.xyz;
    out.tangent = vec4<f32>(o.tc10.xyz, v.particle.x);
    let origin = vec3<f32>(v.row0.w, v.row1.w, v.row2.w);
    out.particle = vec4<f32>(origin, length(vec3<f32>(v.row0.x, v.row1.x, v.row2.x)));
    return out;
}
