//! The session's own Hyprland, read but never written, by hand only.

use super::*;

/// Reads the session's own Hyprland, never writing to it: `j/` queries and
/// the event stream only. Run by hand with `--ignored` under Hyprland to check
/// the parsing against real answers.
#[test]
#[ignore]
fn hyprland_library_reads_the_live_compositor() {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_err() {
        return;
    }
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &examples_script(),
            br#"
                H = require("lib.integrations.hyprland")
                assert(H.start { poll_ms = 10 })
                live = {}
                H.version(function(v) live.version = v end)
                H.layers(function(v) live.layers = v end)
                H.binds(function(v) live.binds = v end)
                H.cursor_position(function(x, y) live.cursor = { x, y } end)
                H.getoption("general:gaps_out", function(v) live.gaps = v end)
            "#,
        )
        .unwrap();
    wait_for(
        &mut runtime,
        r#"
            local s = H.state
            assert(s.monitors:len() > 0 and s.workspaces:len() > 0)
            assert(s.focused_monitor ~= "" and s.active_workspace.id ~= 0)
            assert(s.keyboard_layout ~= "")
            assert(live.version and live.layers and live.binds and live.cursor and live.gaps)
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                local s = H.state
                local lines = { "version " .. tostring(live.version.version),
                    "focused " .. s.focused_monitor .. " ws " .. s.active_workspace.id
                        .. " (" .. s.active_workspace.name .. ")",
                    "layout " .. s.keyboard .. ": " .. s.keyboard_layout,
                    "active " .. s.active_window.address .. " " .. s.active_window.class,
                    "cursor " .. tostring(live.cursor[1]) .. "," .. tostring(live.cursor[2]),
                    "binds " .. #live.binds }
                for i = 1, s.monitors:len() do
                    local m = s.monitors:get(i)
                    lines[#lines + 1] = "monitor " .. m.name .. " shows " .. m.active_workspace
                end
                for i = 1, s.workspaces:len() do
                    local w = s.workspaces:get(i)
                    lines[#lines + 1] = "workspace " .. w.id .. " " .. w.name .. " on "
                        .. w.monitor .. " windows " .. w.windows
                        .. " listed " .. #H.workspace_windows(w.id)
                end
                report = table.concat(lines, "\n")
            "#,
        )
        .unwrap();
    // Surfaced as an error only because an error is what reaches the test.
    let report = runtime
        .execute("report.lua", b"H.stop(); error(report, 0)")
        .unwrap_err();
    eprintln!("{report}");
}
