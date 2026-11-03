// Menu 2D quads as the PC exe draws SEQD sprites: texture x vertex colour
// (vertex colours arrive as PS2 bytes / 128, so 0x80 = 1.0, matching the
// exe's x1.992 vertex scale), clamped, in gamma space.
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var sheet: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var sheet_sampler: sampler;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var c = textureSample(sheet, sheet_sampler, mesh.uv);
#ifdef SRGB_SOURCE
    // An sRGB render target samples as linear: back to its stored values.
    let lo = c.rgb * 12.92;
    let hi = 1.055 * pow(c.rgb, vec3(1.0 / 2.4)) - 0.055;
    c = vec4(select(hi, lo, c.rgb <= vec3(0.0031308)), c.a);
#endif
#ifdef VERTEX_COLORS
    c = c * mesh.color;
#endif
    c = clamp(c, vec4(0.0), vec4(1.0));
#ifndef SRGB_OUTPUT
    // Linear compositing: hand the blender linear values.
    c = vec4(pow(c.rgb, vec3(2.2)), c.a);
#endif
    return c;
}
