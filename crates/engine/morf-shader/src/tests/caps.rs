use super::*;

// Every cap produces its named diagnostic and never a panic. A shader that
// exceeds one is a configuration mistake, and the blast radius of getting this
// wrong is the user's whole session, so each is tested rather than trusted.

#[test]
fn an_oversized_source_is_refused_by_length() {
    let body = format!(
        "function fragment(uv)\n{}\n  return vec4(1.0)\nend",
        "  -- padding\n".repeat(MAX_SOURCE_BYTES / 12 + 16)
    );
    let found = match compile_material(&body) {
        Ok(_) => panic!("expected the length cap to fire"),
        Err(diagnostics) => diagnostics,
    };
    assert!(mentions(&found, "over the"), "{found:?}");
}

#[test]
fn too_many_parameters_are_refused() {
    let params: Vec<Binding> = (0..MAX_PARAMS + 1)
        .map(|index| Binding {
            name: format!("p{index}"),
            ty: Type::F32,
        })
        .collect();
    let found = compile_with(
        "function fragment(uv)
           return vec4(1.0)
         end",
        ShaderKind::Material,
        params,
    )
    .expect_err("expected the parameter cap to fire");
    assert!(mentions(&found, "parameters"), "{found:?}");
}

#[test]
fn loops_nested_past_the_limit_are_refused() {
    let mut body = String::from("function fragment(uv)\n  local t = 0.0\n");
    for _ in 0..MAX_LOOP_NESTING + 1 {
        body.push_str("  while t < 1.0 do\n");
    }
    body.push_str("  t = t + 1.0\n");
    for _ in 0..MAX_LOOP_NESTING + 1 {
        body.push_str("  end\n");
    }
    body.push_str("  return vec4(t)\nend");
    let found = match compile_material(&body) {
        Ok(_) => panic!("expected the nesting cap to fire"),
        Err(diagnostics) => diagnostics,
    };
    assert!(mentions(&found, "nested deeper"), "{found:?}");
}

#[test]
fn a_shader_at_the_edge_of_the_nesting_limit_still_compiles() {
    // The cap has to reject one more than it allows, not one fewer: an
    // off-by-one here would refuse a shader somebody legitimately wrote.
    let mut body = String::from("function fragment(uv)\n  local t = 0.0\n");
    for _ in 0..MAX_LOOP_NESTING {
        body.push_str("  while t < 1.0 do\n");
    }
    body.push_str("  t = t + 1.0\n");
    for _ in 0..MAX_LOOP_NESTING {
        body.push_str("  end\n");
    }
    body.push_str("  return vec4(t)\nend");
    assert!(compile_material(&body).is_ok());
}

#[test]
fn nested_loops_share_one_budget_per_invocation() {
    // Two unbounded loops would multiply their guards to 16M turns; the shared
    // budget stops both once the product reaches the cap.
    let out = wgsl(
        "function fragment(uv)
           local t = 0.0
           while t < 1.0 do
             while t < 2.0 do
               t = t + 0.5
             end
           end
           return vec4(t)
         end",
    );
    assert!(out.contains("var<private> morf_work: u32 = 0u;"), "{out}");
    assert!(out.contains("    morf_work = 0u;"), "{out}");
    let budget = format!("if (morf_work >= {MAX_LOOP_WORK}u) {{ break; }}");
    assert_eq!(out.matches(&budget).count(), 2, "{out}");
    assert_eq!(out.matches("morf_work = morf_work + 1u;").count(), 2);
}

#[test]
fn a_shader_without_loops_carries_no_budget() {
    let out = wgsl(
        "function fragment(uv)
           return vec4(uv, 0.0, 1.0)
         end",
    );
    assert!(!out.contains("morf_work"), "{out}");
}

#[test]
fn constant_nests_over_the_budget_are_refused() {
    let found = compile_material(
        "function fragment(uv)
           local t = 0.0
           for j = 1, 300 do
             for i = 1, 300 do
               t = t + 0.1
             end
           end
           return vec4(t)
         end",
    )
    .expect_err("expected the loop work cap to fire");
    assert!(mentions(&found, "nested loops run"), "{found:?}");
    assert_eq!(found[0].line, 4, "{found:?}");
    let found = compile_material(
        "function fragment(uv)
           local t = 0.0
           for j = 1, 64 do
             for i = 1, 64 do
               t = t + 0.1
             end
           end
           return vec4(t)
         end",
    );
    assert!(found.is_ok(), "{found:?}");
}

#[test]
fn a_loop_inside_a_helper_counts_toward_its_caller() {
    let found = compile_material(
        "local function spin(t)
           for i = 1, 300 do
             t = t + 1.0
           end
           return t
         end
         function fragment(uv)
           local t = 0.0
           for j = 1, 300 do
             t = spin(t)
           end
           return vec4(t)
         end",
    )
    .expect_err("expected the helper's loop to count");
    assert!(mentions(&found, "nested loops run"), "{found:?}");
    assert_eq!(found[0].line, 2, "{found:?}");
}

#[test]
fn a_counter_the_body_assigns_is_not_counted() {
    // Writing the counter makes the trip count unknowable, so the loop keeps
    // the full guard rather than one that would cut it short.
    let out = wgsl(
        "function fragment(uv)
           local t = 0.0
           for i = 1, 4 do
             i = i - 0.5
             t = t + 1.0
           end
           return vec4(t)
         end",
    );
    assert!(
        out.contains(&format!("morf_guard0 >= {MAX_ITERATIONS}u")),
        "{out}"
    );
}
