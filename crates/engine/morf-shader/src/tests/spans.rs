use super::*;

// Checks over the whole program still point at the line that caused them.

#[test]
fn a_parse_error_points_at_its_line() {
    let found = errors(
        "function fragment(uv)
           local x = 1.0
           local y = = 2.0
           return vec4(x)
         end",
    );
    assert_eq!(found[0].line, 3, "{found:?}");
}

#[test]
fn a_wrongly_typed_return_points_at_the_return() {
    let found = errors(
        "function fragment(uv)
           local x = 1.0
           if x > 0.5 then
             return vec4(1.0)
           end
           return uv
         end",
    );
    assert!(mentions(&found, "returns a vec4"), "{found:?}");
    assert_eq!(found[0].line, 6, "{found:?}");
}

#[test]
fn a_missing_return_points_at_the_entry_point() {
    let found = errors(
        "local function helper(t)
           return t
         end
         function fragment(uv)
           if uv.x > 0.5 then
             return vec4(1.0)
           end
         end",
    );
    assert!(mentions(&found, "without returning"), "{found:?}");
    assert_eq!(found[0].line, 4, "{found:?}");
}

#[test]
fn a_derivative_in_a_branch_points_at_its_line() {
    let found = errors(
        "function fragment(uv)
           local total = 0.0
           if uv.x > 0.5 then
             total = total + 1.0
             total = total + dpdx(uv.x)
           end
           return vec4(total)
         end",
    );
    assert!(mentions(&found, "dpdx"), "{found:?}");
    assert_eq!(found[0].line, 5, "{found:?}");
}

#[test]
fn a_misnamed_argument_points_at_the_entry_point() {
    let found = errors(
        "
         function fragment(pos)
           return vec4(1.0)
         end",
    );
    assert!(mentions(&found, "argument 1"), "{found:?}");
    assert_eq!(found[0].line, 2, "{found:?}");
}
