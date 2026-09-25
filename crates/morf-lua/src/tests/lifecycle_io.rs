use morf_scene::Element;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::Duration;

use super::*;

#[test]
fn native_timer_callbacks_recompute_lua_bindings() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "timer.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local count = morf.signal("timer.count", 0)
                morf.timer(1, function() count:set(count:get() + 1) end, false)
                ui.Text { text = function() return "" .. count:get() end }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    poll_until(&mut runtime, |runtime| {
        runtime.scene().string_value(root, "text").ok() == Some("1")
    });

    assert_eq!(runtime.scene().string_value(root, "text").unwrap(), "1");
}

#[test]
fn loader_and_timer_build_native_scene_objects() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "scene-objects.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local count = morf.signal("scene.timer.count", 0)
                ui.Item {
                  ui.Loader {
                    source = function() return ui.Text { text = "loaded" } end,
                  },
                  ui.Timer {
                    interval = 1,
                    running = true,
                    on_triggered = function() count:set(count:get() + 1) end,
                  },
                  ui.Text { text = function() return "" .. count:get() end },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();
    let loader_children = runtime.scene().children(children[0]).unwrap().to_vec();

    assert_eq!(
        runtime.scene().element(children[0]).unwrap(),
        Element::Loader
    );
    assert_eq!(
        runtime.scene().element(children[1]).unwrap(),
        Element::Timer
    );
    assert_eq!(
        runtime
            .scene()
            .string_value(loader_children[0], "text")
            .unwrap(),
        "loaded"
    );

    poll_until(&mut runtime, |runtime| {
        runtime.scene().string_value(children[2], "text").ok() == Some("1")
    });

    assert_eq!(
        runtime.scene().string_value(children[2], "text").unwrap(),
        "1"
    );
    assert!(!runtime.scene().bool_value(children[1], "running").unwrap());
}

#[test]
fn loader_and_timer_follow_dynamic_properties() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "dynamic-loader-timer.lua",
            br#"
                local ui = require("morf.ui")
                local active = morf.signal("loader.active", false)
                local running = morf.signal("timer.running", false)
                local loader = ui.Loader {
                  active = function() return active:get() end,
                  source = function() return ui.Text { text = "loaded" } end,
                }
                local timer = ui.Timer {
                  interval = 1,
                  ["repeat"] = false,
                  running = function() return running:get() end,
                  on_triggered = function() end,
                }
                morf.ipc["dynamic.start"] = function()
                  active:set(true)
                  running:set(true)
                end
                morf.ipc["dynamic.stop"] = function() active:set(false) end
                ui.Item { loader, timer }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();
    let loader = children[0];
    let timer = children[1];
    assert!(runtime.scene().children(loader).unwrap().is_empty());

    runtime.call_ipc("dynamic.start", &[]).unwrap();
    assert!(runtime.poll_services());
    assert_eq!(runtime.scene().children(loader).unwrap().len(), 1);
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while runtime.scene().bool_value(timer, "running").unwrap()
        && std::time::Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(1));
        runtime.poll_services();
    }
    assert!(!runtime.scene().bool_value(timer, "running").unwrap());

    runtime.call_ipc("dynamic.stop", &[]).unwrap();
    assert!(runtime.poll_services());
    assert!(runtime.scene().children(loader).unwrap().is_empty());
}

#[test]
fn lazy_loader_defers_requests_and_exposes_its_item() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lazy-loader.lua",
            br#"
                local ui = require("morf.ui")
                local loader = ui.Loader {
                    active = false,
                    loading = true,
                    source = function() return ui.Text { text = "deferred" } end,
                }
                assert(loader.active == false)
                assert(loader.loading == true)
                assert(loader.item == nil)
                morf.ipc["loader.state"] = function()
                    return loader.active, loader.loading, loader.active_async,
                        loader.item and loader.item.text or "missing"
                end
                morf.ipc["loader.close"] = function() loader.active = false end
                morf.ipc["loader.open_async"] = function() loader.active_async = true end
                ui.Item { loader }
            "#,
        )
        .unwrap();

    assert!(runtime.poll_services());
    assert_eq!(
        runtime.call_ipc("loader.state", &[]).unwrap(),
        [
            IpcValue::Boolean(true),
            IpcValue::Boolean(false),
            IpcValue::Boolean(true),
            IpcValue::String("deferred".into()),
        ]
    );
    runtime.call_ipc("loader.close", &[]).unwrap();
    assert!(runtime.poll_services());
    assert_eq!(
        runtime.call_ipc("loader.state", &[]).unwrap(),
        [
            IpcValue::Boolean(false),
            IpcValue::Boolean(false),
            IpcValue::Boolean(false),
            IpcValue::String("missing".into()),
        ]
    );
    runtime.call_ipc("loader.open_async", &[]).unwrap();
    assert!(runtime.poll_services());
    assert_eq!(
        runtime.call_ipc("loader.state", &[]).unwrap()[0],
        IpcValue::Boolean(true)
    );
}

#[test]
fn retain_locks_delay_loader_item_destruction() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "retainable-loader.lua",
            br#"
                local core = require("morf.core")
                local ui = require("morf.ui")
                local dropped = false
                local destroying = false
                local retained
                local lock
                local loader = ui.Loader {
                    source = function()
                        local item = ui.Text { text = "leaving" }
                        retained = core.retainable(item, {
                            on_dropped = function() dropped = true end,
                            on_about_to_destroy = function() destroying = true end,
                        })
                        lock = core.retain_lock(retained, true)
                        return item
                    end,
                }
                morf.ipc["retain.drop"] = function() loader.active = false end
                morf.ipc["retain.release"] = function() lock:set_locked(false) end
                morf.ipc["retain.state"] = function()
                    return dropped, destroying, retained:retained(),
                        lock:locked(), loader.item and loader.item.text or "missing"
                end
                ui.Item { loader }
            "#,
        )
        .unwrap();

    runtime.call_ipc("retain.drop", &[]).unwrap();
    assert!(runtime.poll_services());
    assert_eq!(
        runtime.call_ipc("retain.state", &[]).unwrap(),
        [
            IpcValue::Boolean(true),
            IpcValue::Boolean(false),
            IpcValue::Boolean(true),
            IpcValue::Boolean(true),
            IpcValue::String("leaving".into()),
        ]
    );
    runtime.call_ipc("retain.release", &[]).unwrap();
    assert_eq!(
        runtime.call_ipc("retain.state", &[]).unwrap(),
        [
            IpcValue::Boolean(true),
            IpcValue::Boolean(true),
            IpcValue::Boolean(false),
            IpcValue::Boolean(false),
            IpcValue::String("missing".into()),
        ]
    );
}

#[test]
fn lua_io_primitives_stream_processes_and_bound_files() {
    let path = std::env::temp_dir().join(format!("morf-lua-bound-file-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, "old").unwrap();
    let source = format!(
        r#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local output = morf.signal("process.output", "pending")
            local file = morf.file("{}")
            assert(file:read() == "old")
            file:write("new")
            assert(file:read() == "new")
            local process = morf.process("sh", {{ "-c", "printf streamed" }})
            morf.timer(1, function()
                local event = process:next()
                if event and event.kind == "stdout" then output:set(event.data) end
            end)
            ui.Text {{ text = function() return output:get() end }}
        "#,
        path.display()
    );
    let mut runtime = Runtime::default();
    runtime.execute("io.lua", source.as_bytes()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let root = runtime.scene().roots()[0];
    while runtime.scene().string_value(root, "text").unwrap() != "streamed"
        && std::time::Instant::now() < deadline
    {
        runtime.poll_services();
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "streamed"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn lua_socket_uses_bounded_timeout_reads() {
    let path = std::env::temp_dir().join(format!("morf-lua-socket-{}", std::process::id()));
    let next = std::env::temp_dir().join(format!("morf-lua-socket-next-{}", std::process::id()));
    let listener = UnixListener::bind(&path).unwrap();
    let next_listener = UnixListener::bind(&next).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"ping");
        stream.write_all(b"pong").unwrap();
        let (mut stream, _) = next_listener.accept().unwrap();
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"next");
        stream.write_all(b"done").unwrap();
    });
    let source = format!(
        r#"
            local morf = require("morf")
            local socket = morf.socket({:?})
            assert(socket:connected())
            assert(socket:path() == {:?})
            assert(not socket:set_path({:?}))
            socket:send("ping")
            socket:flush()
            assert(socket:receive(4, 500) == "pong")
            socket:close()
            assert(not socket:connected())
            assert(socket:receive(4, 1) == nil)
            assert(socket:set_path({:?}))
            assert(socket:set_connected(true))
            assert(socket:connected())
            socket:send("next")
            assert(socket:receive(4, 500) == "done")
            assert(not socket:set_connected(false))
        "#,
        path.to_string_lossy(),
        path.to_string_lossy(),
        next.to_string_lossy(),
        next.to_string_lossy(),
    );
    let mut runtime = Runtime::default();

    runtime.execute("socket.lua", source.as_bytes()).unwrap();

    server.join().unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(next).unwrap();
}

#[test]
fn lua_exposes_stream_parsers_and_socket_servers() {
    let path = std::env::temp_dir().join(format!("morf-lua-server-{}", std::process::id()));
    let next = std::env::temp_dir().join(format!("morf-lua-server-next-{}", std::process::id()));
    let source = format!(
        r#"
            local morf = require("morf")
            local lines = morf.split_parser("\n")
            local first = lines:push("one\ntw")
            assert(#first == 1 and first[1] == "one")
            local second = lines:push("o\r\nlast")
            assert(#second == 1 and second[1] == "two")
            assert(lines:finish() == "last")

            local split = morf.split_parser("--")
            local parts = split:push("a-b--c--tail")
            assert(#parts == 2 and parts[1] == "a-b" and parts[2] == "c")
            assert(split:finish() == "tail")
            assert(split:delimiter() == "--")
            assert(#split:push("left|ri") == 0)
            local replaced = split:set_delimiter("|")
            assert(#replaced == 1 and replaced[1] == "left")
            assert(split:push("ght|")[1] == "right")
            assert(#split:set_delimiter("") == 0)
            assert(split:push("raw")[1] == "raw")

            local collector = morf.stream_collector {{
                maximum_bytes = 16,
                wait_for_end = true,
            }}
            assert(collector:push("one") == false)
            assert(collector:text() == "")
            assert(collector:finish() == true)
            assert(collector:text() == "one")
            assert(collector:finished())
            collector:reset()
            collector:set_wait_for_end(false)
            assert(collector:push("two") == true)
            assert(collector:data() == "two")

            local server = morf.socket_server({:?})
            assert(server:active())
            assert(server:path() == {:?})
            assert(not server:set_path({:?}))
            assert(server:accept() == nil)
            server:close()
            assert(not server:active())
            assert(server:set_path({:?}))
            assert(server:set_active(true))
            assert(server:active())
            assert(not server:set_active(false))
        "#,
        path.to_string_lossy(),
        path.to_string_lossy(),
        next.to_string_lossy(),
        next.to_string_lossy(),
    );
    let mut runtime = Runtime::default();

    runtime
        .execute("io-surfaces.lua", source.as_bytes())
        .unwrap();

    assert!(!path.exists());
    assert!(!next.exists());
}

#[test]
fn a_timer_handle_stops_it() {
    // A repeating timer with no handle could not be stopped, which is how a
    // helper polled every twenty milliseconds went on being polled after the
    // helper was gone. The handle names the timer to whoever holds it.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "timer-cancel.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local fired = morf.signal("timer.fired", 0)
                local tick
                tick = morf.timer(1, function()
                    fired:set(fired:get() + 1)
                    assert(tick:active(), "still there from inside its own callback")
                    assert(tick:cancel(), "and there was something to cancel")
                    assert(not tick:cancel(), "a second cancel finds nothing")
                    assert(not tick:active())
                end, true)
                ui.Text { text = function() return "" .. fired:get() end }
            "#,
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_millis(200);
    while std::time::Instant::now() < deadline {
        runtime.poll_services();
        std::thread::sleep(Duration::from_millis(2));
    }
    let root = runtime.scene().roots()[0];
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "1",
        "once: a cancelled repeating timer does not fire again"
    );
}

/// A `ui.Loader` with the given extra fields, inactive until `loader.open`
/// and again after `loader.close`, counting how often its source runs.
fn switched_loader(fields: &str) -> (Runtime, NodeHandle) {
    let mut runtime = Runtime::default();
    let source = format!(
        r#"
            local ui = require("morf.ui")
            local active = morf.signal("loader.active", false)
            builds = 0
            local loader = ui.Loader {{
              {fields}
              active = function() return active:get() end,
              source = function()
                builds = builds + 1
                return ui.Text {{ text = "panel " .. builds }}
              end,
            }}
            morf.ipc["loader.open"] = function() active:set(true) end
            morf.ipc["loader.close"] = function() active:set(false) end
            morf.ipc["loader.builds"] = function() return builds end
            ui.Item {{ loader }}
        "#
    );
    runtime.execute("switched.lua", source.as_bytes()).unwrap();
    let root = runtime.scene().roots()[0];
    let loader = runtime.scene().children(root).unwrap()[0];
    (runtime, loader)
}

fn builds(runtime: &mut Runtime) -> i64 {
    match runtime.call_ipc("loader.builds", &[]).unwrap().as_slice() {
        [IpcValue::Integer(count)] => *count,
        [IpcValue::Number(count)] => *count as i64,
        other => panic!("builds: {other:?}"),
    }
}

/// The loader's item, as its text and whether it is shown.
fn item(runtime: &Runtime, loader: NodeHandle) -> Option<(String, bool)> {
    let scene = runtime.scene();
    let child = *scene.children(loader).unwrap().first()?;
    Some((
        scene.string_value(child, "text").unwrap().to_owned(),
        scene.bool_value(child, "visible").unwrap(),
    ))
}

#[test]
fn a_preloading_loader_builds_ahead_hidden_and_shows_without_building() {
    let (mut runtime, loader) = switched_loader("preload = true,");
    assert_eq!(item(&runtime, loader), None);
    // Built once, and held: turns that follow leave it be.
    for _ in 0..4 {
        runtime.poll_services();
    }
    // Built while nothing moves, hidden, and not made active by it.
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), false)));
    assert!(!runtime.scene().bool_value(loader, "active").unwrap());
    assert_eq!(builds(&mut runtime), 1);

    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), true)));
    assert_eq!(builds(&mut runtime), 1, "opening built nothing");

    // Closed, it is let go as any Loader's item is, and a fresh one is
    // built ahead of the next open.
    runtime.call_ipc("loader.close", &[]).unwrap();
    for _ in 0..4 {
        runtime.poll_services();
    }
    assert_eq!(item(&runtime, loader), Some(("panel 2".into(), false)));
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 2".into(), true)));
    assert_eq!(builds(&mut runtime), 2);
}

#[test]
fn a_kept_loader_hides_its_item_and_shows_the_same_one_again() {
    let (mut runtime, loader) = switched_loader("keep = true,");
    runtime.poll_services();
    assert_eq!(
        item(&runtime, loader),
        None,
        "nothing built before it is asked"
    );
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    let shown = runtime.scene().children(loader).unwrap()[0];
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), true)));

    runtime.call_ipc("loader.close", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), false)));
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(runtime.scene().children(loader).unwrap()[0], shown);
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), true)));
    assert_eq!(builds(&mut runtime), 1);
}

#[test]
fn a_preload_waits_for_the_scene_to_stop_moving() {
    let (mut runtime, loader) =
        switched_loader("preload = true, behavior = { x = { duration = 200 } },");
    runtime.poll_services();
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    runtime.call_ipc("loader.close", &[]).unwrap();
    // Something moving when the next one would be built.
    runtime.scene_mut().assign(loader, "x", 100.0).unwrap();
    assert!(runtime.has_motion());
    runtime.poll_services();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), None, "not while anything moves");
    assert!(matches!(
        runtime.next_deadline(),
        Some((_, crate::DeadlineCause::Preload))
    ));
    for _ in 0..4 {
        runtime.tick_animations(Duration::from_millis(100)).unwrap();
    }
    assert!(!runtime.has_motion(), "the motion is over");
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 2".into(), false)));
}
