use super::*;

// Records: a Lua table with named fields, emitted as a WGSL struct whose
// identity is its shape.

#[test]
fn a_lua_record_becomes_a_struct() {
    // The last unchecked compiler-side box. There is nowhere in Lua to declare
    // a struct, so identity comes from the shape: two tables with the same
    // field names and types are the same type.
    let out = emitted(
        "record",
        "function fragment(uv)
           local light = { colour = vec3(1.0, 0.9, 0.8), power = 2.5 }
           return vec4(light.colour * light.power * uv.x, 1.0)
         end",
    );
    assert!(out.contains("struct MorfRecord"), "{out}");
    assert!(out.contains("colour: vec3<f32>,"), "{out}");
    assert!(out.contains("power: f32,"), "{out}");
    assert!(out.contains(".colour"), "and its fields are read: {out}");
}

#[test]
fn two_records_of_the_same_shape_are_one_type() {
    // Structural typing, and the reason the fields are sorted: a Lua table has
    // no order of its own, so two records differing only in the order they were
    // written are the same record.
    let out = emitted(
        "same shape",
        "function fragment(uv)
           local a = { x = 1.0, y = 2.0 }
           local b = { y = 3.0, x = 4.0 }
           return vec4(a.x + b.x, a.y + b.y, 0.0, 1.0)
         end",
    );
    let declarations = out.matches("struct MorfRecord").count();
    assert_eq!(declarations, 1, "one declaration, not two:\n{out}");
}

#[test]
fn a_record_field_that_does_not_exist_is_named() {
    let found = errors(
        "function fragment(uv)
           local light = { power = 2.0 }
           return vec4(light.colour, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "has no `colour`"), "{found:?}");
    assert!(
        mentions(&found, "only the fields it was written with"),
        "{found:?}"
    );
}
