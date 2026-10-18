use super::*;

// W2 — matrices: building them, multiplying by them, their builtins, and how
// a matrix parameter is packed.

#[test]
fn a_rotation_matrix_can_be_written_and_applied() {
    // The thing W2 exists for. Before it, this could only be written by
    // expanding the arithmetic by hand, which is why every Shadertoy port that
    // turns anything wanted it.
    let out = emitted(
        "rotate",
        "function fragment(uv, time)
           local c = cos(time)
           local s = sin(time)
           local turn = mat2(vec2(c, s), vec2(0.0 - s, c))
           local p = turn * (uv - vec2(0.5, 0.5))
           return vec4(p + vec2(0.5, 0.5), 0.0, 1.0)
         end",
    );
    assert!(out.contains("mat2x2<f32>("), "{out}");
    assert!(out.contains(" * "), "and it is applied");
}

#[test]
fn a_matrix_can_be_built_from_columns_or_from_numbers() {
    // WGSL accepts both, and so does this: columns are how a rotation is
    // usually written, and the flat form is how one gets pasted out of
    // somebody else's shader.
    for source in [
        "mat3(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0))",
        "mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)",
    ] {
        let out = emitted(
            source,
            &format!(
                "function fragment(uv)
                   local m = {source}
                   local v = m * vec3(uv.x, uv.y, 1.0)
                   return vec4(v, 1.0)
                 end"
            ),
        );
        assert!(out.contains("mat3x3<f32>("), "{source}: {out}");
    }
}

#[test]
fn a_matrix_multiplies_vectors_matrices_and_scalars() {
    let out = emitted(
        "products",
        "function fragment(uv)
           local m = mat2(vec2(2.0, 0.0), vec2(0.0, 2.0))
           local scaled = m * 0.5
           local composed = m * scaled
           local applied = composed * uv
           local row = uv * composed
           return vec4(applied + row, 0.0, 1.0)
         end",
    );
    // A scalar never widens into a matrix: `mat2x2<f32>(0.5)` is not legal.
    assert!(!out.contains("mat2x2<f32>(0.5)"), "{out}");
}

#[test]
fn the_matrix_builtins_are_present() {
    let out = emitted(
        "matrix builtins",
        "function fragment(uv)
           local m = mat3(vec3(2.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), vec3(0.0, 0.0, 1.0))
           local d = determinant(m)
           local back = inverse(transpose(m))
           local v = back * vec3(uv.x, uv.y, 1.0)
           return vec4(v * d, 1.0)
         end",
    );
    for call in ["determinant(", "inverse(", "transpose("] {
        assert!(out.contains(call), "{call} missing:\n{out}");
    }
}

#[test]
fn outer_products_build_a_matrix() {
    // WGSL has no `outer`. naga carries one for its GLSL frontend and the WGSL
    // grammar does not name it, so calling it emits WGSL no driver will accept
    // — which is what the GPU test found. It is a matrix of scaled copies of
    // one vector, so it is emitted as exactly that.
    let out = emitted(
        "outer",
        "function fragment(uv)
           local m = outer(vec3(1.0, 2.0, 3.0), vec3(0.5, 0.5, 0.5))
           return vec4(m * vec3(uv.x, uv.y, 1.0), 1.0)
         end",
    );
    assert!(!out.contains("outer("), "not as a call: {out}");
    assert!(out.contains("mat3x3<f32>("), "as a matrix: {out}");
    // Three columns, each the first vector scaled by a component of the second.
    for component in [".x", ".y", ".z"] {
        assert!(
            out.contains(component),
            "column {component} missing:\n{out}"
        );
    }
}

#[test]
fn an_outer_product_that_is_not_square_says_so() {
    // This language has only square matrices, and naming the limit is better
    // than emitting a type it cannot spell.
    let found = errors(
        "function fragment(uv)
           local m = outer(vec3(1.0, 2.0, 3.0), vec2(0.5, 0.5))
           return vec4(1.0)
         end",
    );
    assert!(mentions(&found, "not square"), "{found:?}");
}

#[test]
fn adding_a_matrix_to_something_is_refused() {
    // `m * v` is a linear map applied to a vector, not a componentwise
    // multiply, and `m + v` has no meaning worth guessing at.
    let found = errors(
        "function fragment(uv)
           local m = mat2(vec2(1.0, 0.0), vec2(0.0, 1.0))
           return vec4(m + uv, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "mat2"), "{found:?}");
}

#[test]
fn a_matrix_of_the_wrong_shape_says_what_it_wanted() {
    let found = errors(
        "function fragment(uv)
           local m = mat3(vec2(1.0, 0.0), vec2(0.0, 1.0))
           return vec4(m * vec3(1.0, 1.0, 1.0), 1.0)
         end",
    );
    assert!(mentions(&found, "cannot be built from"), "{found:?}");
    assert!(mentions(&found, "3 vec3 columns"), "{found:?}");
}

#[test]
fn a_mismatched_matrix_product_is_refused() {
    let found = errors(
        "function fragment(uv)
           local m = mat3(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0))
           return vec4(m * uv, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "mat3"), "{found:?}");
    assert!(mentions(&found, "vec2"), "{found:?}");
}

#[test]
fn matrix_parameters_are_packed_at_wgsl_alignment() {
    // A `mat3x3` is three sixteen-byte columns — forty-eight bytes, not
    // thirty-six. The rule that surprises everybody, and the reason the packer
    // computes rather than assumes.
    let params = vec![
        Binding {
            name: "level".to_owned(),
            ty: Type::F32,
        },
        Binding {
            name: "turn".to_owned(),
            ty: Type::Mat3,
        },
    ];
    let compiled = compile_with(
        "function fragment(uv, time, resolution, coverage, level, turn)
           return vec4(turn * vec3(uv.x, uv.y, level), 1.0)
         end",
        ShaderKind::Material,
        params,
    )
    .expect("this compiles");
    assert_eq!(compiled.params[0].offset, HEADER_BYTES);
    // The matrix aligns to sixteen, so it cannot simply follow the f32.
    assert_eq!(compiled.params[1].offset, HEADER_BYTES + 16);
    assert_eq!(compiled.uniform_size, HEADER_BYTES + 16 + 48);
}
