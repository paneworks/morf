use super::*;

// W4 — derivatives: both axes and their precision variants, and WGSL's
// uniformity rule for where they may be taken.

#[test]
fn a_shader_can_antialias_its_own_edge() {
    // The asymmetry W4 closes. `field.wgsl` softens its own edges with
    // `fwidth`, and before this a configuration's shader could not — so a
    // shader that drew its own shape had no way to soften it at the resolution
    // it was actually being drawn at.
    let out = emitted(
        "antialias",
        "function fragment(uv)
           local d = length(uv - vec2(0.5, 0.5)) - 0.3
           local edge = fwidth(d)
           local coverage = 1.0 - smoothstep(0.0 - edge, edge, d)
           return vec4(coverage, coverage, coverage, 1.0)
         end",
    );
    assert!(out.contains("fwidth("), "{out}");
}

#[test]
fn both_derivative_axes_are_available() {
    let out = emitted(
        "derivatives",
        "function fragment(uv)
           local gx = dpdx(uv.x)
           local gy = dpdy(uv.y)
           return vec4(abs(gx) * 50.0, abs(gy) * 50.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("dpdx("), "{out}");
    assert!(out.contains("dpdy("), "{out}");
}

#[test]
fn the_coarse_and_fine_derivatives_are_available() {
    // Deferred in W4 as "a precision hint nobody writes". They are still that,
    // and they are also six table entries, which is less than the argument for
    // leaving them out was worth.
    let out = emitted(
        "derivative variants",
        "function fragment(uv)
           local a = dpdx_coarse(uv.x) + dpdx_fine(uv.x)
           local b = dpdy_coarse(uv.y) + dpdy_fine(uv.y)
           local c = fwidth_coarse(uv.x) + fwidth_fine(uv.y)
           return vec4(a * 10.0, b * 10.0, c * 10.0, 1.0)
         end",
    );
    for call in [
        "dpdxCoarse(",
        "dpdxFine(",
        "dpdyCoarse(",
        "dpdyFine(",
        "fwidthCoarse(",
        "fwidthFine(",
    ] {
        assert!(out.contains(call), "{call} missing:\n{out}");
    }
}

#[test]
fn a_derivative_inside_a_branch_is_refused_by_name() {
    // WGSL's uniformity rule, caught here rather than by naga — whose message
    // for it is close to unreadable and carries no line number. One check
    // covers `texture` and all three derivatives, because it is one rule.
    let found = errors(
        "function fragment(uv)
           local d = 0.0
           if uv.x > 0.5 then
             d = fwidth(uv.y)
           end
           return vec4(d, 0.0, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "fwidth"), "{found:?}");
    assert!(mentions(&found, "inside an `if`"), "{found:?}");
    assert!(mentions(&found, "neighbouring pixels"), "{found:?}");
}

#[test]
fn a_derivative_inside_a_loop_is_refused_too() {
    let found = errors(
        "function fragment(uv)
           local total = 0.0
           local i = 0.0
           while i < 4.0 do
             i = i + 1.0
             total = total + dpdx(uv.x)
           end
           return vec4(total, 0.0, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "dpdx"), "{found:?}");
}

#[test]
fn a_coarse_derivative_obeys_the_same_uniformity_rule() {
    // Six more calls under one rule, and the rule already knew how to name
    // whichever it found.
    let found = errors(
        "function fragment(uv)
           local d = 0.0
           if uv.x > 0.5 then
             d = fwidth_fine(uv.y)
           end
           return vec4(d, 0.0, 0.0, 1.0)
         end",
    );
    assert!(mentions(&found, "fwidthFine"), "{found:?}");
}

#[test]
fn a_derivative_outside_control_flow_is_fine() {
    // The rule is about *where*, not about the call. Computing it first and
    // branching on the result is exactly what the diagnostic suggests, so it
    // had better work.
    let out = emitted(
        "hoisted",
        "function fragment(uv)
           local edge = fwidth(uv.x)
           local shade = 0.0
           if uv.x > 0.5 then
             shade = edge * 40.0
           end
           return vec4(shade, 0.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("fwidth("), "{out}");
}
