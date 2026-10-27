//! What a call made after load carries of its origin: the Lua frames on
//! its stack, of which a gated native an imported module hands out leaves
//! none.
//!
//! The probe below is that attribution -- what a call-time gate would read:
//! it reports whether any frame of the module `leaky` is on the stack.
//! A closure of `leaky` is seen. The same call made as a tail call from
//! that closure, through a native `leaky` exported, or through a
//! configuration helper that runs what `leaky` handed it later, is not --
//! with `leaky` choosing both the function and its arguments.

use luna::{Callback, CallbackReturn};
use std::cell::RefCell;
use std::rc::Rc;

use super::*;

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("morf-leak-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for folder in ["config", "site", "out"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
    }
    std::fs::canonicalize(root).unwrap()
}

#[test]
fn closures_carry_their_module_but_a_native_it_hands_out_does_not() {
    let root = scratch("probe");
    let mark = root.join("out/spawned");
    std::fs::write(
        root.join("site/leaky.lua"),
        format!(
            r#"
            local M = {{ native = probe, item = {{ "chosen by leaky" }} }}
            function M.closure(item) local seen = probe(item) return seen end
            function M.tail(item) return probe(item) end
            M.spawn = morf.process.spawn
            M.command = {{ command = {{ "touch", "{}" }} }}
            require("defer").later(probe, M.item)
            return M
            "#,
            mark.display()
        ),
    )
    .unwrap();
    std::fs::write(
        root.join("config/defer.lua"),
        "local M = {} function M.later(f, arg) morf.ipc.fire = function() return f(arg) end end return M",
    )
    .unwrap();
    let mut runtime = Runtime::default();
    runtime.set_engine_library(None);
    runtime.set_approvals_path(root.join("approvals.json"));
    runtime.set_module_roots(vec![root.join("config"), root.join("site")]);
    runtime.set_config_file(&root.join("config/shell.lua"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&seen);
    runtime.lua.enter(|ctx| {
        let probe = Callback::from_fn(&ctx, move |ctx, exec, mut stack| {
            let mut level = 0;
            let mut leaky = false;
            while let Some(frame) = exec.lua_frame_at(level) {
                leaky |= frame.chunk_name.display_lossy().to_string() == "leaky";
                level += 1;
            }
            record.borrow_mut().push(leaky);
            stack.replace(ctx, true);
            Ok(CallbackReturn::Return)
        });
        ctx.set_global("probe", probe);
    });
    let shell = root.join("config/shell.lua");
    runtime
        .execute(
            &shell.to_string_lossy(),
            b"LEAKY = require('leaky')\n\
              LEAKY.closure(LEAKY.item)\n\
              LEAKY.tail(LEAKY.item)\n\
              LEAKY.native(LEAKY.item)",
        )
        .unwrap();
    runtime.call_ipc("fire", &[]).unwrap();
    assert_eq!(
        *seen.borrow(),
        [true, false, false, false],
        "seen through its closure; unseen through a tail call, its native and a helper"
    );

    // The same hole with a gated native: leaky chose the program, the
    // configuration's frame is the only one on the stack.
    runtime
        .execute(&shell.to_string_lossy(), b"LEAKY.spawn(LEAKY.command)")
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !mark.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        mark.exists(),
        "after load an exported native runs what it was handed"
    );
    std::fs::remove_dir_all(root).unwrap();
}
