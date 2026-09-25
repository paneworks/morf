// A layer composited through an alpha mask: the node's `mask`.
//
// The mask was rendered, like the layer, into a target of its own over exactly
// the same region of the surface, so one interpolated coordinate finds both:
// `uv` in the layer's texture and `morph_uv` in the mask's. What the layer
// wrote is premultiplied, so scaling it by the mask's alpha scales colour and
// coverage together. Everything else is the plain layer composite of
// glyph.wgsl: opacity from `color.a`, the rounded clip from `mode.y`.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) color_overlay: vec4<f32>,
    @location(3) mode: vec4<f32>,
    @location(4) surface_point: vec2<f32>,
    @location(5) mask_bounds: vec4<f32>,
    @location(6) mask_inverse_0: vec4<f32>,
    @location(7) mask_inverse_1: vec4<f32>,
    @location(8) mask_radii: vec4<f32>,
    @location(9) field: vec4<f32>,
    @location(10) outline_color: vec4<f32>,
    @location(11) morph_uv: vec2<f32>,
    @location(12) morph_size: vec2<f32>,
    @location(13) ramp: f32,
}

@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;
@group(1) @binding(0) var mask_texture: texture_2d<f32>;
@group(1) @binding(1) var mask_sampler: sampler;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex: u32,
    @location(0) origin: vec2<f32>,
    @location(1) axes: vec4<f32>,
    @location(2) uv_bounds: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) color_overlay: vec4<f32>,
    @location(5) mode: vec4<f32>,
    @location(6) surface: vec4<f32>,
    @location(7) mask_bounds: vec4<f32>,
    @location(8) mask_inverse_0: vec4<f32>,
    @location(9) mask_inverse_1: vec4<f32>,
    @location(10) mask_radii: vec4<f32>,
    @location(11) field: vec4<f32>,
    @location(12) outline_color: vec4<f32>,
    @location(13) morph_bounds: vec4<f32>,
    @location(14) ramp: f32,
) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vertex];
    var output: VertexOutput;
    output.position = vec4<f32>(origin + corner.x * axes.xy + corner.y * axes.zw, 0.0, 1.0);
    output.uv = uv_bounds.xy + corner * uv_bounds.zw;
    output.color = color;
    output.color_overlay = color_overlay;
    output.mode = mode;
    output.surface_point = surface.xy + corner * surface.zw;
    output.mask_bounds = mask_bounds;
    output.mask_inverse_0 = mask_inverse_0;
    output.mask_inverse_1 = mask_inverse_1;
    output.mask_radii = mask_radii;
    output.field = field;
    output.outline_color = outline_color;
    output.morph_uv = morph_bounds.xy + corner * morph_bounds.zw;
    output.morph_size = morph_bounds.zw;
    output.ramp = ramp;
    return output;
}


@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(atlas, atlas_sampler, input.uv);
    var coverage = 1.0;
    if input.mode.y > 0.5 {
        let local = vec2<f32>(
            dot(input.mask_inverse_0.xyz, vec3<f32>(input.surface_point, 1.0)),
            dot(input.mask_inverse_1.xyz, vec3<f32>(input.surface_point, 1.0)),
        );
        let distance = rounded_distance(
            local - input.mask_bounds.xy,
            input.mask_bounds.zw,
            input.mask_radii,
        );
        let edge = max(fwidth(distance), 0.0001);
        coverage = smoothstep(edge, -edge, distance);
    }
    // `field.x` inverts: what the mask covers is cut out instead of kept.
    let mask = textureSample(mask_texture, mask_sampler, input.morph_uv).a;
    coverage = coverage * select(mask, 1.0 - mask, input.field.x > 0.5);
    return sampled * (coverage * input.color.a);
}
