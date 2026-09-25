// Subpixel (LCD) text: a glyph's distance field read three times across the
// pixel, once at each subpixel's centre, so each colour channel gets the
// coverage of its own third of the pixel.
//
// Dual-source blending carries the three coverages to the blend unit: the
// first output is the colour times each channel's coverage, the second the
// coverages themselves, and the blend is `src + dst * (1 - src1)` channel by
// channel. That is only right over something opaque -- over a transparent
// pixel the fringes have nothing to mix with -- so the host draws only text
// that lies on an opaque rectangle of the same surface this way; the rest goes
// through the greyscale glyph pipeline. Keep the taps in step with `lcd.rs`,
// the CPU reference the tests hold this against.

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

struct LcdOutput {
    @location(0) @blend_src(0) color: vec4<f32>,
    @location(0) @blend_src(1) coverage: vec4<f32>,
}

/// As in glyph.wgsl: whether the target is gamma-blended.
override MORF_GAMMA_BLEND: bool = false;
/// Blue first rather than red: the three readings are mirrored.
override MORF_LCD_BGR: bool = false;
/// The width, in pixels, each subpixel's reading is smoothed over: the LCD
/// filter. A third is none; one is FreeType's light filter, a little more its
/// default one.
override MORF_LCD_SPREAD: f32 = 1.0;

/// A straight colour as the target stores it: linear for a linear target,
/// encoded for a gamma one.
fn morf_lcd_encode(straight: vec3<f32>) -> vec3<f32> {
    if !MORF_GAMMA_BLEND {
        return straight;
    }
    let clamped = clamp(straight, vec3<f32>(0.0), vec3<f32>(1.0));
    let low = clamped * 12.92;
    let high = 1.055 * pow(clamped, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, clamped <= vec3<f32>(0.0031308));
}

@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;

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
fn fs_main(input: VertexOutput) -> LcdOutput {
    var mask_coverage = 1.0;
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
        mask_coverage = smoothstep(edge, -edge, distance);
    }

    let edge = input.field.x;
    const FIELD_STEP: f32 = 1.0 / 255.0;
    let feather = max(input.ramp * 0.5 * MORF_LCD_SPREAD, FIELD_STEP) + input.field.y;
    let across = dpdx(input.uv);
    let down = dpdy(input.uv);
    // Each subpixel's centre, left to right; two readings about it, a
    // twelfth of a pixel either side and a quarter above and below, so a
    // horizontal edge is smoothed as a vertical one is.
    let direction = select(1.0, -1.0, MORF_LCD_BGR);
    var coverage = vec3<f32>(0.0);
    for (var channel = 0; channel < 3; channel = channel + 1) {
        let centre = (f32(channel) - 1.0) / 3.0 * direction;
        var sum = 0.0;
        for (var tap = 0; tap < 2; tap = tap + 1) {
            let side = select(-1.0, 1.0, tap == 1);
            let shift = across * (centre + side / 12.0) + down * (side * 0.25);
            let here = textureSample(atlas, atlas_sampler, input.uv + shift).r;
            sum = sum + (1.0 - smoothstep(edge - feather, edge + feather, here));
        }
        coverage[channel] = sum * 0.5;
    }
    let body = mix(input.color.rgb, input.color_overlay.rgb, input.color_overlay.a);
    let alpha = coverage * (input.color.a * mask_coverage);
    let mean = (alpha.r + alpha.g + alpha.b) / 3.0;
    var output: LcdOutput;
    output.color = vec4<f32>(morf_lcd_encode(body) * alpha, mean);
    output.coverage = vec4<f32>(alpha, mean);
    return output;
}
