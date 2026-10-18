use super::*;

// W6 — `continue` and `discard`: control flow WGSL has and Lua spells
// differently.

#[test]
fn continue_is_spelled_the_way_lua_spells_it() {
    // Lua has no `continue`, and the idiom every Lua author already writes is
    // `goto continue` with a `::continue::` label at the end of the body. That
    // is real Lua syntax rather than something invented here, so it is what a
    // shader uses — no new keyword, and nothing to learn.
    let out = emitted(
        "continue",
        "function fragment(uv)
           local total = 0.0
           for i = 1, 8 do
             if i > 4.0 then
               goto continue
             end
             total = total + 0.1
             ::continue::
           end
           return vec4(total, 0.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("continue;"), "{out}");
}

#[test]
fn a_counting_loop_still_advances_after_a_continue() {
    // The bug this had to avoid. A `continue` jumps past the tail of the body,
    // so an increment left there would turn a counting loop into one that never
    // advances — bounded by the guard, but wrong. WGSL's `continuing` block
    // exists for exactly this, and the counter lives in it.
    let out = emitted(
        "counting continue",
        "function fragment(uv)
           local total = 0.0
           for i = 1, 4 do
             goto continue
             ::continue::
           end
           return vec4(total, 0.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("continuing {"), "{out}");
    // The increment is inside it, not before the closing brace of the body.
    let continuing = out.split("continuing {").nth(1).unwrap_or_default();
    assert!(
        continuing.contains("+ 1.0"),
        "the counter advances there: {out}"
    );
}

#[test]
fn a_while_loop_needs_no_continuing_block() {
    // Its condition is already re-checked at the top of the body, so there is
    // nothing that has to happen on the way round. An empty `continuing` would
    // be noise in every generated shader.
    let out = emitted(
        "while continue",
        "function fragment(uv)
           local i = 0.0
           while i < 4.0 do
             i = i + 1.0
           end
           return vec4(i * 0.1, 0.0, 0.0, 1.0)
         end",
    );
    assert!(!out.contains("continuing {"), "{out}");
}

#[test]
fn continue_outside_a_loop_is_refused() {
    let found = errors(
        "function fragment(uv)
           goto continue
         end",
    );
    assert!(mentions(&found, "outside a loop"), "{found:?}");
}

#[test]
fn any_other_goto_still_says_what_is_available() {
    let found = errors(
        "function fragment(uv)
           goto elsewhere
           ::elsewhere::
           return vec4(1.0)
         end",
    );
    assert!(mentions(&found, "not available"), "{found:?}");
    assert!(mentions(&found, "goto continue"), "{found:?}");
}

#[test]
fn discard_throws_the_fragment_away() {
    // Spelled as a call because Lua has no keyword to spare, and it is the one
    // call whose entire point is its effect rather than its value.
    let out = emitted(
        "discard",
        "function fragment(uv)
           if uv.x > 0.9 then
             discard()
           end
           return vec4(1.0, 0.0, 0.0, 1.0)
         end",
    );
    assert!(out.contains("discard;"), "{out}");
}
