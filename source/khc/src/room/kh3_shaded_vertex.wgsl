// The vertex stage of the translated KH3 materials: everything is baked on the CPU, this only places it.
#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::view_transformations::position_world_to_clip

struct Vertex {
    @builtin(instance_index) instance: u32,
    @location(0) position: vec3<f32>,
    @location(1) game: vec3<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) vcol: vec4<f32>,
    @location(5) dynamic: vec4<f32>,
    @location(6) misc: vec4<f32>,
    @location(7) normal: vec3<f32>,
    @location(8) tangent: vec4<f32>,
    @location(9) particle: vec4<f32>,
    @location(10) tc1: vec4<f32>,
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
    let world = get_world_from_local(v.instance) * vec4<f32>(v.position, 1.0);
    out.position = position_world_to_clip(world.xyz);
    out.world_position = v.game;
    out.uv = v.uv;
    out.color = v.color;
    out.vcol = v.vcol;
    out.dynamic = v.dynamic;
    out.misc = v.misc;
    out.normal = v.normal;
    out.tangent = v.tangent;
    out.particle = v.particle;
    out.tc1 = v.tc1;
    return out;
}
