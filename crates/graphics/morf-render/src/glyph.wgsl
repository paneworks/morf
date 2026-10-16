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

/// Whether this pipeline writes into a gamma-blended target.
///
/// Set per pipeline by the host. Off, the target is sRGB and the hardware
/// encodes what is written, so blending happens in linear light. On, the
/// target is plain and the value written is already encoded, so the blend
/// unit mixes sRGB values the way a browser or Qt does.
override MORF_GAMMA_BLEND: bool = false;

/// A premultiplied linear colour as the target wants it: unchanged for a
/// linear target, and for a gamma one encoded as straight colour and
/// premultiplied again, so an opaque colour lands on the same bytes either
/// way and only the mixing differs.
fn morf_blend_output(color: vec4<f32>) -> vec4<f32> {
    if !MORF_GAMMA_BLEND || color.a <= 0.0 {
        return color;
    }
    let straight = clamp(color.rgb / color.a, vec3<f32>(0.0), vec3<f32>(1.0));
    let low = straight * 12.92;
    let high = 1.055 * pow(straight, vec3<f32>(1.0 / 2.4)) - 0.055;
    let encoded = select(high, low, straight <= vec3<f32>(0.0031308));
    return vec4<f32>(encoded * color.a, color.a);
}

/// A straight linear colour in the space this pipeline's target holds:
/// unchanged for a linear target, encoded on the sRGB curve for a gamma one.
fn morf_target_rgb(linear: vec3<f32>) -> vec3<f32> {
    if !MORF_GAMMA_BLEND {
        return linear;
    }
    let straight = clamp(linear, vec3<f32>(0.0), vec3<f32>(1.0));
    let low = straight * 12.92;
    let high = 1.055 * pow(straight, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, straight <= vec3<f32>(0.0031308));
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


/// What a configuration's effect shader reads when it samples underneath.
///
/// The contract a compiled shader is built against: it calls `morf_sample` and
/// this pass decides what that means. For a composited layer it is the layer's
/// own rendered content, which is what makes a distortion or a custom blur
/// possible at all.
fn morf_sample(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(atlas, atlas_sampler, uv);
}

/// Where a configuration's effect shader gets to rework what was rendered.
///
/// The default hands the sample straight back, so an ordinary layer composite
/// costs nothing. Attaching an effect shader replaces this body with a call
/// into the compiled one.
fn morf_effect_hook(uv: vec2<f32>, sampled: vec4<f32>) -> vec4<f32> {
    return sampled; // morf-hook: effect
}

/// How far a glass's lens reaches in from its rim, as a share of its shorter
/// side.
const GLASS_BAND: f32 = 0.25;
/// How far in from the edge its rim's light fades, in logical pixels.
const GLASS_RIM_WIDTH: f32 = 1.5;
/// The variance of the backdrop against its blurred mean at which automatic
/// frost begins, and at which it is complete.
const GLASS_QUIET: f32 = 0.0005;
const GLASS_BUSY: f32 = 0.01;

/// Glass over what this surface drew beneath it. Mip 0 of the backdrop is
/// that picture as drawn and mip 1 its blur: the lens moves where both are
/// read, the frost mixes them, and saturation, tint and the rim's light go
/// over the mix. `field` holds saturation, frost (negative: by what is
/// behind), the lens's bend and the rim; `outline_color.xy` the backdrop's
/// logical size, inverted.
fn morf_glass(input: VertexOutput) -> vec4<f32> {
    let shaped = input.mode.y > 0.5;
    let local = vec2<f32>(
        dot(input.mask_inverse_0.xyz, vec3<f32>(input.surface_point, 1.0)),
        dot(input.mask_inverse_1.xyz, vec3<f32>(input.surface_point, 1.0)),
    ) - input.mask_bounds.xy;
    let size = input.mask_bounds.zw;
    let distance = select(-1.0e4, rounded_distance(local, size, input.mask_radii), shaped);
    let edge = max(fwidth(distance), 0.0001);
    let coverage = smoothstep(edge, -edge, distance);

    // The lens is a slab with a rounded rim, its corners rounded at least as
    // far as the rim reaches so the slope turns smoothly round them. It bends
    // most at the edge and not at all past the rim.
    let band = max(min(size.x, size.y) * GLASS_BAND, 1.0);
    let lens = max(input.mask_radii, vec4<f32>(band));
    let step = vec2<f32>(0.5, 0.0);
    let slope = vec2<f32>(
        rounded_distance(local + step.xy, size, lens) - rounded_distance(local - step.xy, size, lens),
        rounded_distance(local + step.yx, size, lens) - rounded_distance(local - step.yx, size, lens),
    );
    let normal = select(vec2<f32>(0.0), normalize(slope), shaped && dot(slope, slope) > 1.0e-8);
    let inside = 1.0 - clamp(-distance / band, 0.0, 1.0);
    let bend = -normal * input.field.z * inside * inside;
    // From the shape's own space back to the surface's.
    let a = input.mask_inverse_0.x;
    let c = input.mask_inverse_0.y;
    let b = input.mask_inverse_1.x;
    let d = input.mask_inverse_1.y;
    let determinant = a * d - b * c;
    var moved = vec2<f32>(0.0);
    if abs(determinant) > 1.0e-6 {
        moved = vec2<f32>(d * bend.x - c * bend.y, a * bend.y - b * bend.x) / determinant;
    }
    let uv = input.uv + moved * input.outline_color.xy;
    let sharp = textureSampleLevel(atlas, atlas_sampler, uv, 0.0);
    let blurred = textureSampleLevel(atlas, atlas_sampler, uv, 1.0);

    // Automatic frost: how busy what is behind is, as the variance of this
    // point against the mean its blur gives.
    var frost = input.field.y;
    if frost < 0.0 {
        let apart = sharp - blurred;
        frost = smoothstep(GLASS_QUIET, GLASS_BUSY, dot(apart, apart) * 0.25);
    }
    let color = mix(sharp, blurred, frost);
    let grey = dot(color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    var rgb = clamp(mix(vec3<f32>(grey), color.rgb, input.field.x), vec3<f32>(0.0), vec3<f32>(color.a));
    rgb = mix(rgb, morf_target_rgb(input.color_overlay.rgb) * color.a, input.color_overlay.a);
    var lit = vec4<f32>(rgb, color.a);
    if input.field.w > 0.0 {
        // Light from the upper left, brightest on the rim that faces it.
        let facing = 0.6 + 0.4 * dot(normal, vec2<f32>(-0.6, -0.8));
        let fade = exp(min(distance, 0.0) / GLASS_RIM_WIDTH);
        let glow = clamp(input.field.w * facing * fade, 0.0, 1.0);
        lit = lit * (1.0 - glow) + vec4<f32>(glow);
    }
    return lit * (input.color.a * coverage);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.mode.x > 1.5 {
        return morf_glass(input);
    }
    var sampled = morf_effect_hook(input.uv, morf_sample(input.uv));

    // A layer mask trims the quad. For a distance field the sampled red channel
    // carries distance rather than coverage, so the mask is applied to the final
    // alpha instead of to the sample it is derived from.
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

    if input.mode.w > 0.5 {
        // The field runs from inside to outside across the encoded spread, so
        // the edge is wherever the requested weight sits and a wider outline is
        // simply a second threshold further out.
        //
        // A morphing glyph arrives as two neighbouring frames of the journey
        // rather than as its two end letters. The correspondence between the
        // letters — which stroke becomes which — was solved in the outline
        // before either was measured, so these two shapes differ by a twelfth
        // of the way across and averaging them has almost nothing to get wrong.
        let edge = input.field.x;
        // Half the field's change across one device pixel. Taken from the size
        // the glyph is drawn at rather than measured from the sampled field:
        // `fwidth` of a minified texture varies from pixel to pixel, and an
        // antialiasing width that wobbles is what a hard, unsettled edge looks
        // like. Floored at the field's own precision, because a byte cannot
        // resolve an edge finer than one step in two hundred and fifty-five.
        const FIELD_STEP: f32 = 1.0 / 255.0;
        let feather = max(input.ramp * 0.5, FIELD_STEP) + input.field.y;

        // Four readings inside the pixel rather than one at its centre.
        //
        // A single reading thresholded over one pixel is the right *coverage*
        // and still looks stepped: it gives one intermediate value per pixel,
        // so along a shallow curve — where the edge moves a fraction of a pixel
        // per column — whole runs of pixels come out the same and the eye reads
        // the run as a stair. Four readings on a rotated grid, averaged,
        // approximate how much of the pixel the letter actually covers, which
        // is what a rasterizer computes exactly and what the stair was missing.
        //
        // The offsets are in pixels, turned into atlas coordinates by how far a
        // pixel reaches across the atlas here — which the derivatives already
        // know, and which is right whatever size the glyph is drawn at.
        let across = dpdx(input.uv);
        let down = dpdy(input.uv);
        let morph_across = dpdx(input.morph_uv);
        let morph_down = dpdy(input.morph_uv);
        let morphing = input.morph_size.x > 0.0 && input.morph_size.y > 0.0;
        // A rotated grid, so neither a horizontal nor a vertical edge has two
        // readings land on the same line as each other.
        var taps = array<vec2<f32>, 4>(
            vec2<f32>(-0.125, -0.375),
            vec2<f32>(0.375, -0.125),
            vec2<f32>(-0.375, 0.125),
            vec2<f32>(0.125, 0.375),
        );
        var fill = 0.0;
        var outer = 0.0;
        for (var index = 0; index < 4; index = index + 1) {
            let tap = taps[index];
            let shift = across * tap.x + down * tap.y;
            var here = textureSample(atlas, atlas_sampler, input.uv + shift).r;
            if morphing {
                let partner = textureSample(
                    atlas,
                    atlas_sampler,
                    input.morph_uv + morph_across * tap.x + morph_down * tap.y,
                ).r;
                here = mix(here, partner, input.field.w);
            }
            fill = fill + (1.0 - smoothstep(edge - feather, edge + feather, here));
            outer = outer + (1.0 - smoothstep(
                edge + input.field.z - feather,
                edge + input.field.z + feather,
                here,
            ));
        }
        fill = fill * 0.25;
        outer = outer * 0.25;
        let body = mix(input.color.rgb, input.color_overlay.rgb, input.color_overlay.a);
        let body_alpha = fill * input.color.a;
        // The outline occupies only the band the fill does not, and sits under
        // it, so a translucent fill does not double-darken over its own outline.
        let outline_alpha = max(outer - fill, 0.0) * input.outline_color.a;
        let alpha = (body_alpha + outline_alpha * (1.0 - body_alpha)) * mask_coverage;
        let premultiplied = (body * body_alpha
            + input.outline_color.rgb * outline_alpha * (1.0 - body_alpha)) * mask_coverage;
        return morf_blend_output(vec4<f32>(premultiplied, alpha));
    }

    sampled *= mask_coverage;
    // A solid quad — a decoration line — takes no coverage from the atlas.
    let sampled_alpha = select(
        select(sampled.a, sampled.r, input.mode.z > 0.5),
        mask_coverage,
        input.mode.z > 1.5,
    );
    let alpha = sampled_alpha * input.color.a;
    if input.mode.x > 0.5 {
        // A layer composited back: its target holds what its subtree wrote,
        // already in the space this pipeline writes, so it is not encoded
        // again.
        return vec4<f32>(sampled.rgb * input.color.a, alpha);
    }
    let color = select(sampled.rgb * input.color.rgb, input.color.rgb, input.mode.z > 0.5);
    return morf_blend_output(
        vec4<f32>(mix(color, input.color_overlay.rgb, input.color_overlay.a) * alpha, alpha),
    );
}
