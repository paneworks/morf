//! `ui.claims`: priority, presence as a signal, and the claimed-write
//! refusal (PLAN item 5).

use std::time::Duration;

use super::*;

fn claims_runtime() -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "claims.lua",
            br#"
                local ui = require("morf.ui")
                local opened = morf.signal("claims.opened", false)
                local panel = ui.Rect { width = 10, height = 10, x = -100 }
                local claims = ui.claims(panel, {
                    open = { when = opened, class = "state", duration = 100,
                             set = { x = 0 } },
                    rest = { class = "ambient", duration = 100,
                             set = { x = -100 } },
                })
                morf.ipc.open = function() opened:set(true) end
                morf.ipc.close = function() opened:set(false) end
                morf.ipc.x = function() return panel.x end
                morf.ipc.presence = function() return claims.open.presence:get() end
                morf.ipc.try_write = function()
                    local ok, error = pcall(function() panel.x = 5 end)
                    return ok, ok and "" or tostring(error)
                end
            "#,
        )
        .unwrap();
    runtime
}

fn tick(runtime: &mut Runtime, frames: u32) {
    for _ in 0..frames {
        runtime.poll_services();
        runtime.tick_animations(Duration::from_millis(16)).unwrap();
    }
}

fn number(runtime: &mut Runtime, verb: &str) -> f64 {
    match runtime.call_ipc(verb, &[]).unwrap()[..] {
        [IpcValue::Number(value)] => value,
        [IpcValue::Integer(value)] => value as f64,
        ref other => panic!("unexpected {verb} reply: {other:?}"),
    }
}

#[test]
fn the_winning_claim_owns_the_property_and_presence_follows() {
    let mut runtime = claims_runtime();
    tick(&mut runtime, 30);
    // At rest the ambient claim owns x, and it starts there.
    assert_eq!(number(&mut runtime, "x"), -100.0);
    assert_eq!(number(&mut runtime, "presence"), 0.0);

    runtime.call_ipc("open", &[]).unwrap();
    tick(&mut runtime, 30);
    // The state claim pre-empts: x travelled to its target, and the
    // open claim's presence arrived with it.
    assert_eq!(number(&mut runtime, "x"), 0.0);
    assert_eq!(number(&mut runtime, "presence"), 1.0);

    runtime.call_ipc("close", &[]).unwrap();
    tick(&mut runtime, 30);
    assert_eq!(number(&mut runtime, "x"), -100.0);
    assert_eq!(number(&mut runtime, "presence"), 0.0);
}

#[test]
fn writing_a_claimed_property_directly_is_refused() {
    let mut runtime = claims_runtime();
    let (ok, error) = match runtime.call_ipc("try_write", &[]).unwrap()[..] {
        [IpcValue::Boolean(ok), IpcValue::String(ref error)] => (ok, error.clone()),
        ref other => panic!("unexpected try_write reply: {other:?}"),
    };
    assert!(!ok);
    assert!(error.contains("claimed"), "{error}");
}
