@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

// 0 copies, 1 encodes linear light as sRGB, 2 decodes sRGB (`depth::transfer`).
override MORF_TRANSFER: f32 = 0.0;

fn encode(linear: vec3<f32>) -> vec3<f32> {
    let c = clamp(linear, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3<f32>(0.0031308));
}

fn decode(encoded: vec3<f32>) -> vec3<f32> {
    let c = clamp(encoded, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(source, source_sampler, input.uv);
    if MORF_TRANSFER == 1.0 {
        return vec4<f32>(encode(texel.rgb), texel.a);
    }
    if MORF_TRANSFER == 2.0 {
        return vec4<f32>(decode(texel.rgb), texel.a);
    }
    return texel;
}
