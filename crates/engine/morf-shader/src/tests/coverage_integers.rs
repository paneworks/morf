use super::*;

// W3 — whole numbers: what type a literal takes, the bitwise operators and
// builtins, and conversions.

#[test]
fn an_integer_literal_still_divides_like_lua() {
    // The rule abstract integers exist to preserve. Nothing in `1 / 2` asks for
    // a whole number, so it stays a float and the answer is a half — as Lua
    // gives, and unlike C.
    let out = emitted(
        "division",
        "function fragment(uv)
           local half = 1 / 2
           return vec4(half, half, half, 1.0)
         end",
    );
    assert!(out.contains("(1.0 / 2.0)"), "{out}");
}

#[test]
fn an_integer_literal_stays_whole_where_something_asks_it_to() {
    // The other half of the same rule: a shift asks for whole numbers, so the
    // literals become them rather than being refused.
    let out = emitted(
        "shift",
        "function fragment(uv)
           local four = 1 << 2
           return vec4(f32(four) * 0.1, 0.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("(1 << 2)"), "{out}");
    assert!(!out.contains("(1.0 << 2.0)"), "and not as floats: {out}");
}

#[test]
fn an_undecided_literal_compared_with_an_integer_stays_an_integer() {
    // `stripe == 0` is a comparison of two `i32`. The literal has no type of
    // its own, so before it took the float default and printed `0.0` — which a
    // driver reads as an `i32` against an abstract float and rejects outright.
    // The shader compiled, and then failed on the GPU, which is the worst place
    // for it to fail.
    let out = compile_material(
        "function fragment(uv, time, resolution, coverage)
           local stripe = i32(floor(uv.x * resolution.x)) % 3
           local tint = vec3(1.0, 1.0, 1.0)
           if stripe == 0 then
             tint = vec3(1.0, 0.0, 0.0)
           end
           return vec4(tint, 1.0)
         end",
    )
    .unwrap()
    .wgsl;
    assert!(out.contains("== 0)"), "{out}");
    assert!(!out.contains("== 0.0)"), "{out}");
}

#[test]
fn an_undecided_literal_compared_with_an_unsigned_takes_that_type() {
    // The same rule, the other way: `u32` needs the `u` suffix, and a bare `0`
    // would be an abstract integer that WGSL will not compare against one.
    let out = compile_material(
        "function fragment(uv, time, resolution, coverage)
           local bits = u32(floor(uv.x * 16.0)) & u32(3)
           local lit = 0.0
           if bits > 1 then
             lit = 1.0
           end
           return vec4(lit, lit, lit, 1.0)
         end",
    )
    .unwrap()
    .wgsl;
    assert!(out.contains("> 1u)"), "{out}");
}

#[test]
fn a_hash_constant_survives_that_would_not_fit_a_float() {
    // Why this step exists at all. `2654435769` needs thirty-two bits and an
    // `f32` has twenty-four of mantissa, so before abstract integers there was
    // no way to write a hash — and therefore no way to write noise that was not
    // the `sin(dot(p, k)) * 43758.5453` trick.
    let out = emitted(
        "hash",
        "function fragment(uv)
           local h = bitcast_u32(uv.x)
           h = h * u32(2654435769)
           h = h ~ (h >> u32(15))
           return vec4(f32(h & u32(255)) / 255.0, 0.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("2654435769u"), "the constant survived: {out}");
    assert!(out.contains("bitcast<u32>("), "{out}");
}

#[test]
fn the_bitwise_operators_are_all_present() {
    let out = emitted(
        "bitwise",
        "function fragment(uv)
           local a = u32(12)
           local b = u32(10)
           local mixed = ((a & b) | (a ~ b)) << u32(1)
           local back = (~mixed) >> u32(2)
           return vec4(f32(back & u32(255)) / 255.0, 0.0, 0.0, 1.0)
         end",
    );
    for operator in ["&", "|", "^", "<<", ">>", "~("] {
        assert!(out.contains(operator), "{operator} missing:\n{out}");
    }
}

#[test]
fn the_bit_counting_builtins_are_present() {
    let out = emitted(
        "bit builtins",
        "function fragment(uv)
           local h = u32(305419896)
           local counted = count_one_bits(h)
             + count_leading_zeros(h)
             + count_trailing_zeros(h)
             + reverse_bits(h)
             + first_leading_bit(h)
             + first_trailing_bit(h)
             + extract_bits(h, u32(4), u32(8))
             + insert_bits(h, u32(3), u32(1), u32(2))
           return vec4(f32(counted & u32(255)) / 255.0, 0.0, 0.0, 1.0)
         end",
    );
    for call in [
        "countOneBits(",
        "countLeadingZeros(",
        "countTrailingZeros(",
        "reverseBits(",
        "firstLeadingBit(",
        "firstTrailingBit(",
        "extractBits(",
        "insertBits(",
    ] {
        assert!(out.contains(call), "{call} missing:\n{out}");
    }
}

#[test]
fn shifting_a_float_says_to_convert_first() {
    let found = errors(
        "function fragment(uv)
           local shifted = uv.x << 2
           return vec4(shifted, 0.0, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "works on whole numbers"), "{found:?}");
    assert!(mentions(&found, "u32(x)"), "{found:?}");
}

#[test]
fn conversions_go_both_ways() {
    let out = emitted(
        "conversions",
        "function fragment(uv)
           local whole = i32(uv.x * 10.0)
           local back = f32(whole) * 0.1
           local unsigned = u32(whole)
           return vec4(back, f32(unsigned) * 0.001, 0.0, 1.0)
         end",
    );
    assert!(out.contains("i32("), "{out}");
    assert!(out.contains("f32("), "{out}");
    assert!(out.contains("u32("), "{out}");
}

#[test]
fn a_bool_has_no_bits_to_reinterpret() {
    let found = errors(
        "function fragment(uv)
           return vec4(f32(bitcast_u32(true)), 0.0, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "no bits to reinterpret"), "{found:?}");
}
