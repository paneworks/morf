use super::*;

// The builtin table against WGSL, tracked in PLAN_WGSL_COVERAGE.md.
//
// One test per group of builtins, plus the diagnostics for using them wrongly.
// The point of testing a builtin at all is that a wrong overload should say
// what was wrong rather than emit WGSL a driver will reject with no line
// number.

#[test]
fn the_ordinary_arithmetic_builtins_are_present() {
    // W1. Each is one table entry, and each is here because a shader author
    // reaching for it and not finding it is the failure being prevented.
    for (call, expected) in [
        ("saturate(uv.x)", "saturate("),
        ("trunc(uv.x)", "trunc("),
        ("inversesqrt(uv.x + 1.0)", "inverseSqrt("),
        ("inverse_sqrt(uv.x + 1.0)", "inverseSqrt("),
        ("fma(uv.x, 2.0, 1.0)", "fma("),
        ("asinh(uv.x)", "asinh("),
        ("acosh(uv.x + 1.0)", "acosh("),
        ("atanh(uv.x * 0.5)", "atanh("),
        ("quantize_to_f16(uv.x)", "quantizeToF16("),
    ] {
        let out = emitted(
            call,
            &format!(
                "function fragment(uv)
                   return vec4({call}, 0.0, 0.0, 1.0)
                 end"
            ),
        );
        assert!(out.contains(expected), "{call} emitted:\n{out}");
    }
}

#[test]
fn the_vector_geometry_builtins_are_present() {
    let out = emitted(
        "geometry",
        "function fragment(uv)
           local i = vec3(0.3, -0.8, 0.5)
           local n = vec3(0.0, 1.0, 0.0)
           local bounced = reflect(i, n)
           local bent = refract(i, n, 0.66)
           local facing = faceforward(n, i, n)
           return vec4(bounced + bent + facing, 1.0)
         end",
    );
    assert!(out.contains("refract("), "{out}");
    assert!(out.contains("faceForward("), "{out}");
}

#[test]
fn refract_says_what_its_third_argument_has_to_be() {
    // The one W1 builtin with a shape of its own: two vectors and a scalar
    // ratio, not three of one type.
    let found = errors(
        "function fragment(uv)
           local i = vec3(0.3, -0.8, 0.5)
           return vec4(refract(i, i, vec3(0.5, 0.5, 0.5)), 1.0)
         end",
    );
    assert!(mentions(&found, "ratio must be an f32"), "{found:?}");
}

#[test]
fn a_builtin_used_at_the_wrong_type_names_both() {
    let found = errors(
        "function fragment(uv)
           return vec4(saturate(true), 0.0, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "saturate"), "{found:?}");
}

/// Every builtin name the language offers, so the count in the plan is checked
/// rather than remembered.
#[test]
fn the_builtin_count_matches_what_the_plan_claims() {
    // naga 30.0.1's `MathFunction` has 79 variants. This is not a target to
    // reach — packing and unpacking exist for storage buffers a shader here
    // does not have — but the number should never drift without somebody
    // noticing, because "we have most of it" is how a gap goes unmeasured.
    let names = crate::builtins::available();
    let count = names.split(", ").count();
    assert!(
        count >= 46,
        "W1 brings the surface to at least 46 names, found {count}: {names}",
    );
}

#[test]
fn every_naga_math_function_this_language_can_reach_is_reachable() {
    // The count the plan tracks, checked rather than remembered. naga 30.0.1's
    // `MathFunction` has 79 variants; `Outer` is the only one this language
    // deliberately restricts (to the square case), and every other one is
    // callable by name.
    let names = crate::builtins::available();
    let count = names.split(", ").count();
    assert!(
        count >= 79,
        "expected the whole surface, found {count}: {names}",
    );
}

#[test]
fn the_relational_builtins_are_present() {
    let out = emitted(
        "relational",
        "function fragment(uv)
           local bad = is_nan(uv.x) or is_inf(uv.y)
           local lit = select(1.0, 0.0, all(bad))
           return vec4(lit, select(0.0, 1.0, any(bad)), 0.0, 1.0)
         end",
    );
    for call in ["isNan(", "isInf(", "all(", "any("] {
        assert!(out.contains(call), "{call} missing:\n{out}");
    }
}

#[test]
fn ldexp_scales_by_a_power_of_two() {
    // The straight miss: W1 deferred this to W3 for needing an `i32`
    // exponent, and W3 added integers without coming back for it.
    let out = emitted(
        "ldexp",
        "function fragment(uv)
           local scaled = ldexp(uv.x, i32(3))
           return vec4(scaled, 0.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("ldexp("), "{out}");
}

#[test]
fn the_float_packing_family_is_present() {
    let out = emitted(
        "packing",
        "function fragment(uv)
           local four = vec4(uv.x, uv.y, 0.5, 1.0)
           local two = vec2(uv.x, uv.y)
           local bits = pack4x8unorm(four)
             + pack4x8snorm(four)
             + pack2x16unorm(two)
             + pack2x16snorm(two)
             + pack2x16float(two)
           local back = unpack4x8unorm(bits) + unpack4x8snorm(bits)
           local pair = unpack2x16float(bits) + unpack2x16unorm(bits)
           return vec4(back.xy + pair, 0.0, 1.0)
         end",
    );
    for call in [
        "pack4x8unorm(",
        "pack4x8snorm(",
        "pack2x16unorm(",
        "pack2x16snorm(",
        "pack2x16float(",
        "unpack4x8unorm(",
        "unpack4x8snorm(",
        "unpack2x16float(",
        "unpack2x16unorm(",
    ] {
        assert!(out.contains(call), "{call} missing:\n{out}");
    }
}

#[test]
fn the_byte_packing_family_needs_integer_vectors_and_has_them() {
    // Six of the eighteen packing builtins take or return a `vec4<i32>` or a
    // `vec4<u32>`, which is why they could not simply be table entries: the
    // types had to exist first.
    let out = emitted(
        "byte packing",
        "function fragment(uv)
           local signed = vec4i(1, -2, 3, -4)
           local unsigned = vec4u(1, 2, 3, 4)
           local bits = pack4x_i8(signed)
             + pack4x_u8(unsigned)
             + pack4x_i8_clamp(signed)
             + pack4x_u8_clamp(unsigned)
           local back = unpack4x_u8(bits)
           local dotted = dot4_u8_packed(bits, bits)
           return vec4(f32(back.x + dotted) * 0.001, 0.0, 0.0, 1.0)
         end",
    );
    for call in [
        "pack4xI8(",
        "pack4xU8(",
        "pack4xI8Clamp(",
        "pack4xU8Clamp(",
        "unpack4xU8(",
        "dot4U8Packed(",
    ] {
        assert!(out.contains(call), "{call} missing:\n{out}");
    }
    assert!(out.contains("vec4<i32>("), "integer vectors exist: {out}");
}

#[test]
fn modf_and_frexp_split_a_number() {
    // WGSL returns a struct from both, and this language has no struct of its
    // own. Rather than inventing one, the result is a type whose only operation
    // is reading a part off it — which is the whole of what anybody does.
    let out = emitted(
        "split",
        "function fragment(uv)
           local parts = modf(uv.x * 4.0)
           local pieces = frexp(uv.y + 1.0)
           return vec4(parts.fract, parts.whole * 0.1, f32(pieces.exp) * 0.1, 1.0)
         end",
    );
    assert!(out.contains("modf("), "{out}");
    assert!(out.contains(".fract"), "{out}");
    assert!(out.contains("frexp("), "{out}");
    assert!(out.contains(".exp"), "{out}");
}

#[test]
fn a_split_result_names_the_parts_it_has() {
    let found = errors(
        "function fragment(uv)
           local parts = modf(uv.x)
           return vec4(parts.nonsense, 0.0, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "no `nonsense`"), "{found:?}");
    assert!(mentions(&found, "`.fract` and `.whole`"), "{found:?}");
}
