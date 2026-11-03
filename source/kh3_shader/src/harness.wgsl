// What a translated KH3 pixel shader runs in: `kh_material(KhIn) -> KhOut` is the game's own compiled
// base-pass shader, instruction by instruction (translate.rs), spliced in at the marker below; `kh_mask`
// is the opacity mask of a masked material. Everything is in GAME space (KH3's: cm, X forward, Y right,
// Z up). The inputs' values are StrawHatPro's harness (crates/kh-er-render/shaders/er_vfx.wgsl, MIT).
//KH_VIEW

struct KhParams {
    m: array<vec4<f32>, 64>,
    // x: opacity-mask clip value, z: 1 = the emitter draws additive although the shader is translucent,
    // w: 1 = the reverse
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

struct KhIn {
    tc0: vec4<f32>, tc1: vec4<f32>, tc2: vec4<f32>, tc3: vec4<f32>, tc7: vec4<f32>, tc10: vec4<f32>, tc11: vec4<f32>,
    col0: vec4<f32>, col1: vec4<f32>, col2: vec4<f32>, col3: vec4<f32>,
    ptime: vec4<f32>, ppos: vec4<f32>, psubuv: vec4<f32>, psize: vec4<f32>,
    svpos: vec4<f32>, front: vec4<f32>,
    ptr0: vec4<f32>, ptr1: vec4<f32>, ptr2: vec4<f32>,
    // Translated world position (relative to the camera), game space.
    twp: vec3<f32>,
    // 1 / (scene depth behind this pixel, cm); 0 when nothing is there.
    inv_scene_depth: f32,
    v8: vec4<f32>, v9: vec4<f32>, v10: vec4<f32>, v53: vec4<f32>, v54: vec4<f32>, v55: vec4<f32>, v58: vec4<f32>,
    v117: vec4<f32>, v118: vec4<f32>, v119: vec4<f32>, v122: vec4<f32>, v123: vec4<f32>, v124: vec4<f32>, v125: vec4<f32>,
    v126: vec4<f32>, v127: vec4<f32>, v128: vec4<f32>, v130: vec4<f32>, v137: vec4<f32>, v161: vec4<f32>,
    p8: vec4<f32>, p9: vec4<f32>, p11: vec4<f32>, p17: vec4<f32>, p18: vec4<f32>,
    s10: vec4<f32>, l5: vec4<f32>,
    decal_pos: vec3<f32>, dcol: vec4<f32>, dprm: vec4<f32>, dz: vec4<f32>, d12: vec4<f32>, d23: vec4<f32>,
    dist12: vec4<f32>,
}

struct KhOut {
    o0: vec4<f32>,
    o1: vec4<f32>,
    o2: vec4<f32>,
    o3: vec4<f32>,
}

struct KhSubUv {
    sub0: vec4<f32>,
    sub1: vec4<f32>,
}

// The mesh sub-UV coordinates of UE's FMeshParticleVertexFactory from the particle's sub image:
// -(1 + lerp7 | image8 << 7 | (h - 1) << 15 | (v - 1) << 19).
fn kh_mesh_subuv(uv: vec2<f32>, code: f32) -> KhSubUv {
    let c = u32(round(-code)) - 1u;
    let lerp = f32(c & 127u) / 127.0;
    let f0 = (c >> 7u) & 255u;
    let h = ((c >> 15u) & 15u) + 1u;
    let v = ((c >> 19u) & 15u) + 1u;
    let f1 = min(f0 + 1u, h * v - 1u);
    let size = vec2<f32>(1.0 / f32(h), 1.0 / f32(v));
    let s0 = (uv + vec2<f32>(f32(f0 % h), f32(f0 / h))) * size;
    let s1 = (uv + vec2<f32>(f32(f1 % h), f32(f1 / h))) * size;
    return KhSubUv(vec4<f32>(s0, uv), vec4<f32>(s1, lerp, 0.0));
}

//KH_GENERATED

struct KhVaryings {
    @builtin(position) position: vec4<f32>,
    // GAME space, cm
    @location(0) world_position: vec3<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) vcol: vec4<f32>,
    @location(4) dynamic: vec4<f32>,
    // x: emitter seconds, y: the particle's relative time, z: sub-image blend (mesh: sub-UV code), w: tangent sign
    @location(5) misc: vec4<f32>,
    @location(6) normal: vec3<f32>,
    // w: the height of the floor under the camera, cm
    @location(7) tangent: vec4<f32>,
    @location(8) particle: vec4<f32>,
    @location(9) tc1: vec4<f32>,
}

// KH3's scene is HDR under a filmic curve; the room's frame blends stored sRGB values, so each
// fragment goes through the curve on its own: the fitted ACES curve, sRGB encoded.
//KH_DEPTH_FN

fn kh_tone(c: vec3<f32>) -> vec3<f32> {
    let m_in = mat3x3<f32>(vec3<f32>(0.59719, 0.07600, 0.02840), vec3<f32>(0.35458, 0.90834, 0.13383), vec3<f32>(0.04823, 0.01566, 0.83777));
    let m_out = mat3x3<f32>(vec3<f32>(1.60475, -0.10208, -0.00327), vec3<f32>(-0.53108, 1.10813, -0.07276), vec3<f32>(-0.07367, -0.00605, 1.07602));
    let v = m_in * min(max(c, vec3<f32>(0.0)), vec3<f32>(1000.0));
    let a = v * (v + vec3<f32>(0.0245786)) - vec3<f32>(0.000090537);
    let b = v * (0.983729 * v + vec3<f32>(0.4329510)) + vec3<f32>(0.238081);
    let d = saturate(m_out * (a / b));
    let lo = d * 12.92;
    let hi = 1.055 * pow(d, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, d <= vec3<f32>(0.0031308));
}

@fragment
fn fragment(in: KhVaryings) -> @location(0) vec4<f32> {
    var i: KhIn;
    let time = in.misc.x;
    // Bevy's axes to KH3's: Y and Z change places.
    let w = view.world_from_view;
    let right = w[0].xzy;
    let up = w[1].xzy;
    let forward = -w[2].xzy;
    let camera = w[3].xzy * KH_SCALE;
    let size = vec4<f32>(view.viewport.zw, 1.0 / view.viewport.zw);
    i.twp = in.world_position - camera;
    i.svpos = vec4<f32>(in.position.xy, in.position.z, dot(i.twp, forward));
    i.front = vec4<f32>(select(0.0, 1.0, dot(in.normal, -i.twp) >= 0.0));
    // The scene behind this pixel, from the depth prepass and the floor at Sora's feet (a room's floor
    // need not be in the prepass).
    var scene_depth = 0.0;
    let device = kh_scene_device(in.position);
    if (device > 0.0) {
        scene_depth = view.clip_from_view[3][2] / device * KH_SCALE;
    }
    if (i.twp.z < -1.0e-3 && camera.z > in.tangent.w) {
        let floor_depth = dot(i.twp * ((camera.z - in.tangent.w) / -i.twp.z), forward);
        scene_depth = select(floor_depth, min(scene_depth, floor_depth), scene_depth > 0.0);
    }
    i.inv_scene_depth = 0.0;
    if (scene_depth > 0.0) {
        i.inv_scene_depth = 1.0 / max(scene_depth, 1.0);
    }
    //KH_DECAL
    let n = normalize(in.normal);
    let t = normalize(in.tangent.xyz);
    i.tc10 = vec4<f32>(t, in.misc.z);
    i.tc11 = vec4<f32>(n, in.misc.w);
    i.tc7 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    i.ptime = vec4<f32>(in.misc.y);
    i.ppos = in.particle;
    i.psubuv = in.uv;
    i.psize = vec4<f32>(time);
    i.col0 = in.vcol;
    i.col3 = vec4<f32>(0.0, 0.0, time, 0.0);
    i.tc0 = in.uv;
    // (a mesh: UV set 1; a sprite: the dynamic parameter)
    i.tc1 = in.tc1;
    i.tc2 = in.color;
    //KH_MESH
    i.tc3 = in.uv;
    i.col1 = in.color;
    i.col2 = in.dynamic;
    // ---- View (slots identified by how the compiled shaders use them)
    i.v8 = vec4<f32>(right.x, up.x, forward.x, 0.0);
    i.v9 = vec4<f32>(right.y, up.y, forward.y, 0.0);
    i.v10 = vec4<f32>(right.z, up.z, forward.z, 0.0);
    i.v53 = vec4<f32>(0.0, 0.0, 1.0, 0.0);
    i.v54 = vec4<f32>(0.5, -0.5, 0.5, 0.5);
    i.v55 = vec4<f32>(camera, 0.0);
    i.v58 = vec4<f32>(-camera, 0.0);
    i.v117 = vec4<f32>(0.0);
    i.v118 = size;
    i.v119 = size;
    i.v122 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    i.v123 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    i.v124 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    i.v125 = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    i.v126 = vec4<f32>(0.0);
    i.v127 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    i.v128 = vec4<f32>(time);
    i.v130 = vec4<f32>(0.0);
    i.v137 = vec4<f32>(0.0);
    i.v161 = vec4<f32>(1.0, 1.0, time, 0.0);
    i.p8 = vec4<f32>(0.0);
    i.p9 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    i.p11 = vec4<f32>(0.0);
    i.p17 = vec4<f32>(0.0);
    i.p18 = vec4<f32>(0.0);
    i.s10 = vec4<f32>(0.0, time, 0.0, 0.0);
    i.l5 = vec4<f32>(1.0);
    i.ptr0 = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    i.ptr1 = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    i.ptr2 = vec4<f32>(0.0, 0.0, 1.0, 0.0);
    let aspect = size.x / size.y;
    i.dist12 = vec4<f32>(view.clip_from_view[0][0], aspect, 1920.0, 1920.0 / aspect);
    //KH_MASK_CALL
    let o = kh_material(i);
    //KH_DISTORT
    var color = o.o0;
    //KH_DEFERRED
    // The emitter's blend type against the shader's own.
    if (kh.misc.z != 0.0) {
        color = vec4<f32>(color.xyz * color.w, 0.0);
    }
    if (kh.misc.w != 0.0) {
        color = vec4<f32>(color.xyz, saturate(max(color.x, max(color.y, color.z))));
    }
    //KH_GLOW
    return vec4<f32>(kh_tone(color.xyz), max(color.w, 0.0));
}
