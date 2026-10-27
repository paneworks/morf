use morf_layout::Layout;
use std::fs;
use std::path::PathBuf;

use super::*;

#[test]
fn window_handles_map_item_geometry_after_layout() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "window-map.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                local child = ui.Item { x = 15, y = 7, width = 20, height = 10 }
                local root = ui.Item { child }
                _G.window_handle = window.toplevel {
                  root = root, width = 100, height = 50,
                }
                _G.window_child = child
            "#,
        )
        .unwrap();
    let root = runtime.window_surface_configs()[0].root;
    let layout = Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 100.0,
            height: 50.0,
        },
        &mut NoText,
    )
    .unwrap();
    runtime.observe_layout(&layout);

    runtime
        .execute(
            "window-map-check.lua",
            br#"
                local position = window_handle:item_position(window_child)
                assert(position.x == 15 and position.y == 7)
                local rect = window_handle:item_rect(window_child)
                assert(rect.x == 15 and rect.y == 7)
                assert(rect.width == 20 and rect.height == 10)
                local point = window_handle:map_from_item(window_child, 2, 3)
                assert(point.x == 17 and point.y == 10)
                local mapped = window_handle:map_rect_from_item(window_child, 1, 2, 3, 4)
                assert(mapped.x == 16 and mapped.y == 9)
                assert(mapped.width == 3 and mapped.height == 4)
            "#,
        )
        .unwrap();
}

#[test]
fn core_namespace_exposes_native_process_and_path_data() {
    let mut runtime = Runtime::default();
    runtime.set_shell_root(PathBuf::from("/tmp/morf-shell"));
    runtime
        .execute(
            "morf.lua",
            br#"
                assert(type(morf.process_id) == "number")
                assert(type(morf.version) == "string")
                assert(type(morf.instance_id) == "string")
                assert(type(morf.launch_time_ms) == "number")
                assert(type(morf.app_id) == "string")
                assert(morf.env("PATH") ~= nil)
                assert(morf.env("MORF_VARIABLE_THAT_DOES_NOT_EXIST") == nil)
                assert(morf.paths.shell() == "/tmp/morf-shell")
                assert(string.find(morf.shell_id(), "morf-shell-", 1, true) == 1)
                assert(morf.paths.shell("icons/logo.svg") == "/tmp/morf-shell/icons/logo.svg")
                assert(morf.paths.shell("config.lua") == "/tmp/morf-shell/config.lua")
                assert(morf.paths.state("state.json") == morf.paths.state() .. "/state.json")
                assert(morf.paths.cache("image") == morf.paths.cache() .. "/image")
                assert(not pcall(morf.paths.shell, "/absolute"))
                assert(morf.image.icon("morf-icon-that-does-not-exist") == nil)
                assert(not morf.image.has_icon("morf-icon-that-does-not-exist"))
                assert(not pcall(morf.image.icon, "icon", "hicolor", 0))
                local timer = morf.time.elapsed()
                assert(timer:elapsed() >= 0)
                assert(timer:elapsed_ms() >= 0)
                assert(timer:elapsed_ns() >= 0)
                assert(timer:restart() >= 0)
                assert(timer:restart_ms() >= 0)
                assert(timer:restart_ns() >= 0)
                local curve = morf.easing.curve("in_quad")
                assert(curve:value_at(0.5) == 0.25, "in_quad")
                assert(curve:interpolate(0.5, 10, 20) == 12.5, "interpolate")
                local point = curve:interpolate(0.5, { x = 0, y = 10 }, { x = 20, y = 30 })
                assert(point.x == 5 and point.y == 15)
                local rect = curve:interpolate(0.5,
                    { x = 0, y = 10, width = 20, height = 30 },
                    { x = 20, y = 30, width = 40, height = 50 })
                assert(rect.x == 5 and rect.y == 15 and rect.width == 25 and rect.height == 35)
                local bezier = morf.easing.curve({ x1 = 0.42, y1 = 0, x2 = 1, y2 = 1 })
                assert(bezier:value_at(0) == 0, "bezier start")
                assert(bezier:value_at(1) > 0.999999, "bezier end")
            "#,
        )
        .unwrap();
}

#[test]
fn core_menu_models_nested_entries_and_activation() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "menu.lua",
            br#"
                local triggered = false
                local menu = morf.window.menu {
                    { id = "open", text = "Open", icon = "folder", on_triggered = function()
                        triggered = true
                    end },
                    { id = "separator", separator = true },
                    { id = "flag", text = "Flag", button_type = "checkbox" },
                    { id = "choice", text = "Choice", children = {
                        { id = "one", text = "One", button_type = "radio", radio_group = "choice", checked = true },
                        { id = "two", text = "Two", button_type = "radio", radio_group = "choice" },
                    } },
                }
                assert(#menu:entries() == 4)
                assert(menu:entry("choice").has_children)
                assert(#menu:children("choice") == 2)
                menu:activate("open")
                assert(triggered)
                assert(menu:activate("flag").check_state == "checked")
                menu:activate("two")
                assert(menu:entry("one").checked == false)
                assert(menu:entry("two").checked == true)
                menu:set_enabled("open", false)
                assert(menu:entry("open").enabled == false)
                menu:set_visible("open", false)
                assert(menu:entry("open").visible == false)
                menu:set_checked("flag", "partial")
                assert(menu:entry("flag").check_state == "partial")
            "#,
        )
        .unwrap();
}

#[test]
fn json_codec_preserves_arrays_objects_and_null() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "json.lua",
            br#"
                local json = require("morf.json")
                local value = json.decode('{"array":[],"null":null,"object":{},"values":[1,true,"x"]}')
                assert(value.values[1] == 1)
                assert(value.values[2] == true)
                assert(value.values[3] == "x")
                assert(value.null ~= nil)
                assert(json.encode(value) == '{"array":[],"null":null,"object":{},"values":[1,true,"x"]}')
                assert(json.encode(json.array({})) == '[]')
                assert(json.encode(json.object({})) == '{}')
                assert(json.encode({ empty = json.array({}), missing = json.null }) == '{"empty":[],"missing":null}')
                assert(json.decode(json.encode({ nested = { answer = 42 } })).nested.answer == 42)
            "#,
        )
        .unwrap();
}

#[test]
fn file_view_tracks_state_and_atomic_writes() {
    let path = std::env::temp_dir().join(format!("morf-lua-file-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let source = format!(
        r#"
            local file = morf.fs.view {{ path = {:?}, preload = false }}
            assert(file:path() == {:?})
            assert(not file:preload())
            assert(file:loaded() == false)
            assert(file:exists() == false)
            assert(file:set_text("hello"))
            assert(file:loaded())
            assert(file:exists())
            assert(file:text() == "hello")
            assert(file:data() == "hello")
            assert(file:error() == nil)
            file:set_atomic_writes(false)
            assert(file:atomic_writes() == false)
            assert(file:set_text("world"))
            assert(file:reload())
            assert(file:text() == "world")
            local json = require("morf.json")
            assert(file:set_text(json.encode({{ answer = 42, values = json.array({{ 1, 2 }}) }})))
            local decoded = json.decode(file:text())
            assert(decoded.answer == 42)
            assert(decoded.values[2] == 2)
            assert(file:set_path(""))
            assert(file:path() == "" and not file:loaded())
            file:set_preload(true)
            assert(file:preload())
            assert(file:set_path({:?}))
            assert(file:loaded())
        "#,
        path.to_string_lossy(),
        path.to_string_lossy(),
        path.to_string_lossy(),
    );
    let mut runtime = Runtime::default();
    runtime.execute("file-view.lua", source.as_bytes()).unwrap();
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(written["answer"], 42);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn process_view_applies_launch_context_and_restarts() {
    let directory = std::env::temp_dir();
    let source = format!(
        r#"
            local process = morf.process.view {{
                command = {{ "sh", "-c", "printf '%s:%s' \"$PWD\" \"$MORF_LUA_PROCESS\"" }},
                environment = {{ MORF_LUA_PROCESS = "ok" }},
                working_directory = {:?},
                running = true,
            }}
            assert(process:running())
            assert(type(process:process_id()) == "number")
            local output = ""
            for _ = 1, 20 do
                local event = process:next(1000)
                if event and event.kind == "stdout" then output = output .. event.data end
                if event and event.kind == "exit" then
                    assert(event.success)
                    break
                end
            end
            assert(output == {:?}, "first run stdout: " .. output)
            assert(process:running() == false, "first run should have exited")
            assert(process:command()[1] == "sh")
            assert(process:environment().MORF_LUA_PROCESS == "ok")
            assert(process:working_directory() == {:?})
            assert(not process:clear_environment())
            process:set_command({{ "sh", "-c", "printf '%s' \"$MORF_LUA_PROCESS\"" }})
            process:set_environment({{ MORF_LUA_PROCESS = "changed" }})
            process:set_working_directory(nil)
            process:set_clear_environment(false)
            assert(process:start())
            assert(process:running())
            local restarted = ""
            for _ = 1, 20 do
                local event = process:next(1000)
                if event and event.kind == "stdout" then restarted = restarted .. event.data end
                if event and event.kind == "exit" then break end
            end
            assert(restarted == "changed", "second run stdout: " .. restarted)
            process:set_command({{ "sh", "-c", "sleep 5" }})
            assert(process:start())
            process:set_command({{ "sh", "-c", "printf restarted" }})
            local replaced = ""
            for _ = 1, 20 do
                local event = process:next(1000)
                if event and event.kind == "stdout" then replaced = replaced .. event.data end
                if event and event.kind == "exit" then break end
            end
            assert(replaced == "restarted", "third run stdout: " .. replaced)
        "#,
        directory.to_string_lossy(),
        format!("{}:ok", directory.display()),
        directory.to_string_lossy(),
    );
    let mut runtime = Runtime::default();
    runtime
        .execute("process-view.lua", source.as_bytes())
        .unwrap();
}

#[test]
fn desktop_entries_scan_and_lookup_native_data() {
    let directory = std::env::temp_dir().join(format!("morf-lua-desktop-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("browser.desktop"),
        b"[Desktop Entry]\nType=Application\nName=Browser\nGenericName=Web Browser\nStartupWMClass=browser\nExec=browser --new-window %U\nIcon=browser\nCategories=Network;WebBrowser;\nActions=Private;\n\n[Desktop Action Private]\nName=Private\nExec=browser --private %U\n",
    )
    .unwrap();
    let source = format!(
        r#"
            local entries = morf.apps.entries({{{:?}}})
            local applications = entries:applications()
            assert(#applications == 1)
            assert(applications[1].name == "Browser")
            assert(applications[1].command[2] == "--new-window")
            assert(applications[1].categories[2] == "WebBrowser")
            assert(applications[1].actions[1].id == "Private")
            assert(entries:by_id("browser").icon == "browser")
            assert(entries:heuristic_lookup("BROWSER").id == "browser")
            assert(entries:by_id("missing") == nil)
            assert(not entries:refresh())
            local added = morf.fs.view {{ path = {:?}, preload = false }}
            assert(added:set_text("[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\n"))
            assert(entries:refresh())
            assert(#entries:applications() == 2)
            assert(entries:by_id("editor").name == "Editor")
            assert(not entries:refresh())
        "#,
        directory.to_string_lossy(),
        directory.join("editor.desktop").to_string_lossy(),
    );
    let mut runtime = Runtime::default();
    runtime
        .execute("desktop-entries.lua", source.as_bytes())
        .unwrap();
    fs::remove_dir_all(directory).unwrap();
}
