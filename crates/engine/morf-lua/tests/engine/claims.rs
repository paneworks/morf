//! `ui.claims`: priority, presence as a signal, and the refusals that keep
//! a claimed property its claims' alone.

use std::time::Duration;

use super::*;

/// `try(f, ...)`: whether `f` ran, and what it raised as text.
const TRY: &str = r#"
    local function try(f, ...)
      local ok, error = pcall(f, ...)
      return ok, ok and "" or tostring(error)
    end
"#;

fn runtime(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .set_preference("reduced_motion", IpcValue::Boolean(false))
        .unwrap();
    let source = format!("{TRY}\n{source}");
    runtime.execute("claims.lua", source.as_bytes()).unwrap();
    runtime
}

fn tick(runtime: &mut Runtime, frames: u32) {
    for _ in 0..frames {
        runtime.poll_services();
        runtime.tick_animations(Duration::from_millis(16)).unwrap();
    }
}

/// Ticks until nothing moves, as a loop waiting on its frame callbacks.
fn settle(runtime: &mut Runtime) -> u32 {
    let mut frames = 0;
    while runtime.has_motion() && frames < 600 {
        tick(runtime, 1);
        frames += 1;
    }
    frames
}

fn call(runtime: &mut Runtime, verb: &str) -> Vec<IpcValue> {
    runtime.call_ipc(verb, &[]).unwrap()
}

fn number(runtime: &mut Runtime, verb: &str) -> f64 {
    match call(runtime, verb)[..] {
        [IpcValue::Number(value)] => value,
        [IpcValue::Integer(value)] => value as f64,
        ref other => panic!("unexpected {verb} reply: {other:?}"),
    }
}

/// `(ok, message)` of a verb that answers `pcall`.
fn attempt(runtime: &mut Runtime, verb: &str) -> (bool, String) {
    match call(runtime, verb)[..] {
        [IpcValue::Boolean(ok), IpcValue::String(ref error)] => (ok, error.clone()),
        [IpcValue::Boolean(ok)] => (ok, String::new()),
        ref other => panic!("unexpected {verb} reply: {other:?}"),
    }
}

const PANEL: &str = r#"
    local ui = require("morf.ui")
    local opened = morf.signal("claims.opened", false)
    local panel = ui.Rect { width = 10, height = 10, x = -100 }
    local claims = ui.claims(panel, {
        motion = { duration = 160, easing = "out_cubic" },
        open = { when = opened, class = "state", set = { x = 0 } },
        rest = { set = { x = -100 } },
    })
    local shade = ui.Rect { width = 10, height = 10,
        opacity = function() return claims.open.presence:get() end }
    morf.ipc.open = function() opened:set(true) end
    morf.ipc.close = function() opened:set(false) end
    morf.ipc.x = function() return panel.x end
    morf.ipc.presence = function() return claims.open.presence:get() end
    morf.ipc.shade = function() return shade.opacity end
    morf.ipc.write = function() return try(function() panel.x = 5 end) end
    morf.ipc.bind = function() return try(function() panel.x = function() return 5 end end) end
    morf.ipc.set_presence = function() return try(function() claims.open.presence:set(0.5) end) end
"#;

#[test]
fn the_winning_claim_owns_the_property_and_presence_follows() {
    let mut runtime = runtime(PANEL);
    assert_eq!(number(&mut runtime, "x"), -100.0);
    assert_eq!(number(&mut runtime, "presence"), 0.0);
    call(&mut runtime, "open");
    assert!(runtime.has_motion(), "a changed condition owes a frame");
    settle(&mut runtime);
    assert_eq!(number(&mut runtime, "x"), 0.0);
    assert_eq!(number(&mut runtime, "presence"), 1.0);
    call(&mut runtime, "close");
    settle(&mut runtime);
    assert_eq!(number(&mut runtime, "x"), -100.0);
    assert_eq!(number(&mut runtime, "presence"), 0.0);
}

#[test]
fn presence_eases_and_a_binding_on_it_follows_every_frame() {
    let mut runtime = runtime(PANEL);
    call(&mut runtime, "open");
    tick(&mut runtime, 3);
    let presence = number(&mut runtime, "presence");
    // 48 ms of a 160 ms out-cubic: well past the linear 0.3.
    assert!(
        presence > 0.6 && presence < 0.7,
        "eased presence: {presence}"
    );
    let progress = (number(&mut runtime, "x") + 100.0) / 100.0;
    assert!(
        (presence - progress).abs() < 1e-3,
        "{presence} against {progress}"
    );
    assert_eq!(
        number(&mut runtime, "shade"),
        presence,
        "the binding is current"
    );
    let frames = settle(&mut runtime);
    assert!(frames > 0, "presence is motion");
    assert_eq!(number(&mut runtime, "shade"), 1.0);
}

#[test]
fn a_claimed_property_and_a_presence_refuse_writes() {
    let mut runtime = runtime(PANEL);
    for verb in ["write", "bind"] {
        let (ok, error) = attempt(&mut runtime, verb);
        assert!(!ok && error.contains("claimed"), "{verb}: {error}");
    }
    let (ok, error) = attempt(&mut runtime, "set_presence");
    assert!(!ok && error.contains("read-only"), "{error}");
    assert_eq!(number(&mut runtime, "presence"), 0.0);
}

#[test]
fn conditions_are_read_by_lua_truth() {
    let mut runtime = runtime(
        r#"
        local ui = require("morf.ui")
        local answer = morf.signal("answer", nil)
        local panel = ui.Rect { width = 10, height = 10 }
        ui.claims(panel, {
          on = { when = function() return answer:get() end, set = { x = 1 } },
          off = { set = { x = 0 } },
        })
        morf.ipc.give = function(value) answer:set(value) end
        morf.ipc.x = function() return panel.x end
    "#,
    );
    let cases = [
        (IpcValue::String("yes".into()), 1.0),
        (IpcValue::Boolean(false), 0.0),
        (IpcValue::Integer(0), 1.0),
        (IpcValue::Nil, 0.0),
    ];
    for (value, x) in cases {
        runtime
            .call_ipc("give", std::slice::from_ref(&value))
            .unwrap();
        settle(&mut runtime);
        assert_eq!(number(&mut runtime, "x"), x, "{value:?}");
    }
}

#[test]
fn ties_in_a_class_go_to_the_claim_declared_first() {
    let mut runtime = runtime(
        r#"
        local ui = require("morf.ui")
        local panel = ui.Rect { width = 10, height = 10 }
        ui.claims(panel, { zeta = { set = { x = 1 } }, beta = { set = { y = 1 } } })
        ui.claims(panel, { alpha = { set = { x = 2 } } })
        local other = ui.Rect { width = 10, height = 10 }
        ui.claims(other, { b = { set = { x = 2 } }, a = { set = { x = 1 } } })
        morf.ipc.x = function() return panel.x end
        morf.ipc.other = function() return other.x end
    "#,
    );
    assert_eq!(number(&mut runtime, "x"), 1.0, "the earlier ui.claims wins");
    assert_eq!(number(&mut runtime, "other"), 1.0, "inside one, by name");
}

#[test]
fn a_bad_claim_is_refused_whole_and_nothing_else_stops() {
    let mut runtime = runtime(
        r#"
        local ui = require("morf.ui")
        local panel = ui.Rect { width = 10, height = 10,
          behavior = { y = { duration = 100 } } }
        morf.ipc.bad = function()
          return try(ui.claims, panel, {
            fine = { set = { x = 4 } },
            wrong = { set = { color = "hello" } },
          })
        end
        morf.ipc.bare = function() return try(ui.claims, panel, { b = { set = { color = {} } } }) end
        morf.ipc.slide = function() panel.y = 100 end
        morf.ipc.x = function() panel.x = 3 return panel.x end
        morf.ipc.y = function() return panel.y end
    "#,
    );
    for verb in ["bad", "bare"] {
        let (ok, error) = attempt(&mut runtime, verb);
        assert!(!ok && error.contains("claim"), "{verb}: {error}");
    }
    assert_eq!(number(&mut runtime, "x"), 3.0, "nothing of it was claimed");
    call(&mut runtime, "slide");
    tick(&mut runtime, 3);
    let y = number(&mut runtime, "y");
    assert!(y > 0.0 && y < 100.0, "other motion runs: {y}");
}

#[test]
fn other_writers_of_a_claimed_property_are_refused() {
    let mut runtime = runtime(
        r#"
        local ui = require("morf.ui")
        local bound = ui.Rect { width = 10, height = 10, x = function() return 10 end }
        local looping = ui.Rect { width = 10, height = 10,
          loop = { x = { from = 0, to = 9, duration = 100 } } }
        local stated = ui.Rect { width = 10, height = 10,
          states = { wide = { property_changes = { x = 4 } } } }
        local panel = ui.Rect { width = 10, height = 10 }
        local source = ui.Rect { width = 10, height = 10 }
        ui.claims(panel, { rest = { set = { x = 0 } } })
        local function claim(node) return function() ui.claims(node, { rest = { set = { x = 0 } } }) end end
        morf.ipc.over_binding = function() return try(claim(bound)) end
        morf.ipc.over_loop = function() return try(claim(looping)) end
        morf.ipc.over_states = function() return try(claim(stated)) end
        morf.ipc.play = function()
          return try(function() morf.animation.play { { node = panel, property = "x", to = 200, duration = 50 } } end)
        end
        morf.ipc.fling = function()
          return try(function() morf.animation.fling { node = panel, property = "x", velocity = 900 } end)
        end
        morf.ipc.follow = function() return try(function() ui.follow(panel, "x", { node = source }) end) end
        morf.ipc.x = function() return panel.x end
    "#,
    );
    let refusals = [
        ("over_binding", "binding"),
        ("over_loop", "loop"),
        ("over_states", "states"),
        ("play", "claimed"),
        ("fling", "claimed"),
        ("follow", "claimed"),
    ];
    for (verb, why) in refusals {
        let (ok, error) = attempt(&mut runtime, verb);
        assert!(!ok && error.contains(why), "{verb}: {error}");
    }
    tick(&mut runtime, 4);
    assert_eq!(number(&mut runtime, "x"), 0.0, "nothing else moved it");
}

#[test]
fn a_target_can_be_a_binding_and_moves_its_claim() {
    let mut runtime = runtime(
        r#"
        local ui = require("morf.ui")
        local reach = morf.signal("reach", 40)
        local panel = ui.Rect { width = 10, height = 10 }
        ui.claims(panel, {
          motion = ui.spring { stiffness = 300, damping = 30 },
          out = { set = { x = function() return reach:get() * 2 end, y = reach } },
        })
        morf.ipc.reach = function(value) reach:set(value) end
        morf.ipc.x = function() return panel.x end
        morf.ipc.y = function() return panel.y end
        morf.ipc.target = function() return panel.x_target end
    "#,
    );
    assert_eq!(number(&mut runtime, "x"), 80.0, "first values at once");
    assert_eq!(number(&mut runtime, "y"), 40.0);
    runtime
        .call_ipc("reach", &[IpcValue::Integer(100)])
        .unwrap();
    tick(&mut runtime, 2);
    assert_eq!(number(&mut runtime, "target"), 200.0);
    let x = number(&mut runtime, "x");
    assert!(x > 80.0 && x < 200.0, "travelling by the spring: {x}");
    settle(&mut runtime);
    assert_eq!(number(&mut runtime, "x"), 200.0);
    assert_eq!(number(&mut runtime, "y"), 100.0);
}

#[test]
fn claims_go_with_their_node_and_with_remove() {
    let mut runtime = runtime(
        r#"
        local ui = require("morf.ui")
        local held = {}
        morf.ipc.make = function()
          local node = ui.Rect { width = 10, height = 10 }
          local claims = ui.claims(node, {
            a = { when = function() return true end, set = { x = 1, opacity = 0.5 } },
            b = { set = { x = 2 } },
          })
          held[#held + 1] = { node = node, claims = claims }
        end
        morf.ipc.drop = function()
          for _, h in ipairs(held) do ui.destroy(h.node) end
          held = {}
        end
        morf.ipc.remove = function()
          local h = held[1]
          return h.claims.a.remove(), h.claims.a.remove(), h.node.x,
            try(function() h.node.opacity = 1 end)
        end
        morf.ipc.x = function() return held[1].node.x end
    "#,
    );
    call(&mut runtime, "make");
    tick(&mut runtime, 1);
    let baseline = runtime.resource_stats();
    for _ in 0..20 {
        call(&mut runtime, "make");
    }
    call(&mut runtime, "drop");
    tick(&mut runtime, 1);
    let after = runtime.resource_stats();
    assert!(
        after.graph_signals <= baseline.graph_signals
            && after.graph_effects <= baseline.graph_effects,
        "{baseline:?} -> {after:?}"
    );
    assert!(!runtime.has_motion(), "dead nodes leave nothing to tick");
    call(&mut runtime, "make");
    let reply = call(&mut runtime, "remove");
    assert_eq!(reply[0], IpcValue::Boolean(true));
    assert_eq!(reply[1], IpcValue::Boolean(false), "removed once");
    assert_eq!(reply[3], IpcValue::Boolean(true), "opacity is free again");
    settle(&mut runtime);
    assert_eq!(number(&mut runtime, "x"), 2.0, "b wins without a");
}

#[test]
fn an_exit_owns_the_properties_it_moves() {
    let mut runtime = runtime(
        r#"
        local ui = require("morf.ui")
        local lit = morf.signal("lit", false)
        local root = ui.Item { width = 100, height = 100 }
        local card = ui.Rect { width = 10, height = 10,
          exit = { opacity = 0, duration = 200 } }
        ui.reparent(card, root)
        ui.claims(card, {
          lit = { when = lit, set = { opacity = 0.9 } },
          dim = { set = { opacity = 0.5 } },
        })
        morf.ipc.leave = function() ui.destroy(card) end
        morf.ipc.light = function() lit:set(true) end
        morf.ipc.opacity = function() return card.opacity end
    "#,
    );
    call(&mut runtime, "leave");
    tick(&mut runtime, 4);
    let before = number(&mut runtime, "opacity");
    call(&mut runtime, "light");
    tick(&mut runtime, 2);
    let after = number(&mut runtime, "opacity");
    assert!(after < before, "still fading out: {before} -> {after}");
}
