//! What the widget layer leans on, measured on the real VM: a pure Lua
//! loop, a host-to-Lua call, a signal change that re-runs bindings, a table
//! written to a signal, and building nodes with bindings (what a skin
//! does). Ignored by default; run with
//!
//!     cargo test --release -p morf-lua --lib [--features jit] bench_vm -- --ignored --nocapture
//!
//! and `MORF_JIT=off` for the interpreter alone in a jit build.

use std::time::{Duration, Instant};

use super::*;

/// Runs `body` until a second has passed, `poll`ing between runs so the
/// native tier can compile what turned hot, and returns the time per run.
fn per_run(runtime: &mut Runtime, mut body: impl FnMut(&mut Runtime)) -> Duration {
    // Warm up: what is going to be compiled has been by the time we time.
    for _ in 0..200 {
        body(runtime);
        runtime.poll_services();
    }
    let started = Instant::now();
    let mut runs = 0u32;
    while started.elapsed() < Duration::from_secs(1) {
        body(runtime);
        runs += 1;
        if runs % 64 == 0 {
            runtime.poll_services();
        }
    }
    started.elapsed() / runs
}

fn report(what: &str, per: Duration, unit: &str) {
    eprintln!(
        "bench_vm {what:<34} {:>12.3} µs {unit}",
        per.as_secs_f64() * 1e6
    );
}

fn ipc(runtime: &mut Runtime, verb: &str) {
    runtime.call_ipc(verb, &[]).unwrap();
}

#[test]
#[ignore]
fn bench_vm() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "bench.lua",
            br##"
        local morf = require("morf")
        local ui = require("morf.ui")

        -- A pure Lua loop: numbers and a table, no host calls.
        morf.ipc.compute = function()
          local sum, t = 0, {}
          for i = 1, 10000 do sum = sum + i * 0.5 t[i % 64 + 1] = sum end
          return sum
        end

        -- A host-to-Lua call that does nothing.
        morf.ipc.noop = function() return true end

        -- One signal read by 1000 bindings.
        local level = morf.signal("bench.level", 0)
        for i = 1, 1000 do
          ui.Rect { width = 1, height = 1, opacity = function() return (level:get() + i) % 2 end }
        end
        local n = 0
        morf.ipc.bump = function() n = n + 1 level:set(n) return true end

        -- A 64-entry table written to a signal.
        local rows = morf.signal("bench.rows", {})
        local k = 0
        morf.ipc.rows = function()
          k = k + 1
          local t = {}
          for i = 1, 64 do t[i] = { id = i, value = k + i, name = "row" } end
          rows:set(t)
          return true
        end

        -- What a skin does: 200 nodes, three bindings each.
        local hover = morf.signal("bench.hover", false)
        morf.ipc.build = function()
          local root = ui.Item {}
          for i = 1, 200 do
            ui.reparent(ui.Rect {
              x = function() return i * 2 end,
              width = function() return hover:get() and 12 or 10 end,
              color = function() return hover:get() and "#ff0000" or "#00ff00" end,
            }, root)
          end
          ui.destroy(root)
          return true
        end
    "##,
        )
        .unwrap();

    let compute = per_run(&mut runtime, |r| ipc(r, "compute"));
    let noop = per_run(&mut runtime, |r| ipc(r, "noop"));
    let bump = per_run(&mut runtime, |r| ipc(r, "bump"));
    let rows = per_run(&mut runtime, |r| ipc(r, "rows"));
    let build = per_run(&mut runtime, |r| ipc(r, "build"));

    report("pure Lua loop (10k iterations)", compute, "per call");
    report("host-to-Lua call", noop, "per call");
    report("signal re-running 1000 bindings", bump, "per change");
    report("  one binding", bump / 1000, "per binding");
    report("64-row table into a signal", rows, "per set");
    report("200 nodes, 3 bindings each", build, "per build");
    report("  one node", build / 200, "per node");
    eprintln!(
        "bench_vm jit: {}",
        runtime.jit_report().unwrap_or_else(|| "off".into())
    );
}
