@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

// 0 copies, 1 encodes linear light as sRGB, 2 decodes sRGB, 3 encodes PQ
// over BT.2020, 4 writes scRGB (`depth::Transfer`).
override MORF_TRANSFER: f32 = 0.0;
// The target holds sRGB-encoded values rather than linear light.
override MORF_SOURCE_SRGB: bool = false;
// SDR white under PQ, in units of 10000 cd/m².
override MORF_WHITE: f32 = 0.0203;

fn encode(linear: vec3<f32>) -> vec3<f32> {
    let c = clamp(linear, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3<f32>(0.0031308));
}

fn decode(encoded: vec3<f32>) -> vec3<f32> {
    let c = clamp(encoded, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

// The sRGB curve past 1.0, for values brighter than SDR white.
fn decode_extended(encoded: vec3<f32>) -> vec3<f32> {
    let c = max(encoded, vec3<f32>(0.0));
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

// Straight linear sRGB light, 1.0 being SDR white.
fn straight_linear(texel: vec4<f32>) -> vec3<f32> {
    if texel.a <= 0.0 {
        return vec3<f32>(0.0);
    }
    let straight = texel.rgb / texel.a;
    if MORF_SOURCE_SRGB {
        return decode_extended(straight);
    }
    return straight;
}

// SMPTE ST 2084 of luminance in units of 10000 cd/m².
fn pq(luminance: vec3<f32>) -> vec3<f32> {
    let p = pow(clamp(luminance, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(0.1593017578125));
    return pow((0.8359375 + 18.8515625 * p) / (1.0 + 18.6875 * p), vec3<f32>(78.84375));
}

// Linear BT.709 to linear BT.2020 (`hdr::SRGB_TO_BT2020`), column major.
const SRGB_TO_BT2020 = mat3x3<f32>(
    vec3<f32>(0.6274039, 0.0690973, 0.0163914),
    vec3<f32>(0.3292830, 0.9195404, 0.0880133),
    vec3<f32>(0.0433131, 0.0113623, 0.8955953),
);

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(source, source_sampler, input.uv);
    if MORF_TRANSFER == 1.0 {
        return vec4<f32>(encode(texel.rgb), texel.a);
    }
    if MORF_TRANSFER == 2.0 {
        return vec4<f32>(decode(texel.rgb), texel.a);
    }
    // HDR: premultiplied again after encoding, as a compositor reads it.
    if MORF_TRANSFER == 3.0 {
        let wide = max(SRGB_TO_BT2020 * straight_linear(texel), vec3<f32>(0.0));
        return vec4<f32>(pq(wide * MORF_WHITE) * texel.a, texel.a);
    }
    if MORF_TRANSFER == 4.0 {
        return vec4<f32>(straight_linear(texel) * texel.a, texel.a);
    }
    return texel;
}
