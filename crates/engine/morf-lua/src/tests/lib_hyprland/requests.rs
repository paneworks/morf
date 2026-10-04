//! The request socket: state filled from its answers, and commands and
//! batches sent to it.

use super::*;

#[test]
fn hyprland_library_fills_state_from_the_request_socket() {
    let instance = fake_hyprland("state");
    let mut runtime = Runtime::default();
    start(&mut runtime, &instance);

    wait_for(
        &mut runtime,
        r#"
            local s = H.state
            assert(s.connected)
            assert(s.monitors:len() == 2, "monitors")
            assert(s.workspaces:len() == 3, "workspaces")
            assert(s.clients:len() == 2, "clients")
            assert(s.keyboard_layout == "English (US)", "layout")
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                local s = H.state
                assert(s.focused_monitor == "DP-1")
                assert(s.active_workspace.id == 1 and s.active_workspace.name == "1")
                assert(s.keyboard == "kb")
                assert(s.submap == "")
                assert(s.fullscreen == false)
                -- Sorted by id, whatever order the compositor answered in.
                assert(s.workspaces:get(1).id == 1 and s.workspaces:get(3).id == 3)
                assert(s.workspaces:get(1).active and not s.workspaces:get(2).active)
                assert(s.workspaces:get(3).visible, "shown on the other monitor")
                assert(s.monitors:get(1).name == "eDP-1" and s.monitors:get(1).active_workspace == 3)
                local term = H.client("f00d")
                assert(term and term.floating and term.xwayland and term.width == 800)
                assert(#H.workspace_windows(1) == 2 and #H.workspace_windows(2) == 0)
                assert(H.occupied(1) and not H.occupied(2))
                assert(H.monitor_workspace("eDP-1") == 3 and H.monitor_workspace() == 1)
                assert(H.monitor("DP-1").scale == 1 and H.workspace(3).monitor == "eDP-1")
                assert(#H.snapshot().clients == 2)
            "#,
        )
        .unwrap();
}

#[test]
fn hyprland_library_sends_commands_and_batches() {
    let instance = fake_hyprland("commands");
    let fake = Arc::clone(&instance.fake);
    let mut runtime = Runtime::default();
    start(&mut runtime, &instance);
    wait_for(&mut runtime, "assert(H.state.monitors:len() == 2)");

    runtime
        .execute(
            "check.lua",
            br#"
                answers = {}
                H.dispatch("workspace", 3, function(ok, reply) answers.dispatch = { ok, reply } end)
                H.keyword("general:gaps_out", 8, function(ok) answers.keyword = ok end)
                H.eval("hl.dispatch(hl.dsp.focus({ workspace = 3 }))",
                    function(reply) answers.eval = reply end)
                H.reload(true, function(ok) answers.reload = ok end)
                H.cursor_position(function(x, y) answers.cursor = { x, y } end)
                H.getoption("general:gaps_out", function(option) answers.option = option end)
                H.options({ "general:gaps_out", "nope" }, function(map) answers.options = map end)
                H.version(function(version) answers.version = version end)
                H.json("nonsense", function(value, err) answers.bad = { value, err } end)
                H.batch({ "j/cursorpos", "j/submap" }, function(replies)
                    answers.batch = replies
                end)
                assert(H.batch({ "a;b" }, function(r, err) answers.refused = err end) == false)
            "#,
        )
        .unwrap();
    wait_for(
        &mut runtime,
        r#"
            assert(answers.dispatch and answers.keyword ~= nil and answers.eval)
            assert(answers.reload ~= nil and answers.cursor and answers.option)
            assert(answers.options and answers.version and answers.bad and answers.batch)
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                assert(answers.dispatch[1] == true and answers.dispatch[2] == "ok")
                assert(answers.keyword == true and answers.reload == true)
                assert(answers.eval:find("evaluated hl.dispatch", 1, true))
                assert(answers.cursor[1] == 12 and answers.cursor[2] == 34)
                assert(answers.option.css == "10 10 10 10")
                assert(answers.options["general:gaps_out"].set == true)
                assert(answers.options["nope"] == nil)
                assert(answers.version.version == "0.56.2")
                assert(answers.bad[1] == nil and answers.bad[2] == "unknown request")
                assert(#answers.batch == 2 and answers.batch[2] == '"default"')
                assert(answers.batch[1]:find('"x": 12', 1, true))
                assert(answers.refused == "batched command contains ';'")
            "#,
        )
        .unwrap();
    let requests = fake.requests.lock().unwrap().clone();
    for expected in [
        "/dispatch workspace 3",
        "/keyword general:gaps_out 8",
        "/eval hl.dispatch(hl.dsp.focus({ workspace = 3 }))",
        "/reload config-only",
        "[[BATCH]]j/cursorpos;j/submap",
        "j/nonsense",
    ] {
        assert!(
            requests.iter().any(|seen| seen == expected),
            "{expected} was not sent: {requests:?}"
        );
    }
}
