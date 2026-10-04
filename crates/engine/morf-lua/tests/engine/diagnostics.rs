//! What the engine says about a configuration, unprompted.

use super::*;

#[test]
fn a_container_that_laid_out_to_nothing_is_reported_once() {
    // The commonest way a configuration draws nothing and says nothing: a
    // container whose size resolved to zero with a whole subtree inside it.
    // Only the layout knows -- a zero in the scene may mean "auto" -- so the
    // lint reads the resolved geometry, and says it once per node rather than
    // sixty times a second.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lint.lua",
            br#"
                local ui = require("morf.ui")
                -- Nested, because the root is sized to the surface whatever it
                -- asks for; it is a child that can resolve to nothing.
                ui.Rect { ui.Rect { width = 0, height = 0, ui.Text { text = "unseen" } } }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let layout = morf_layout::Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 100.0,
            height: 100.0,
        },
        &mut NoText,
    )
    .unwrap();
    runtime.lint_layout(&layout, root);
    runtime.poll_services();
    runtime.lint_layout(&layout, root);
    runtime.poll_services();
    let logs = runtime.take_logs();
    assert_eq!(logs.len(), 1, "once, not once per layout: {logs:?}");
    assert!(
        logs[0].message.contains("laid out to nothing"),
        "{}",
        logs[0].message
    );
    assert_eq!(logs[0].level, LogLevel::Warn);
}

#[test]
fn a_container_growing_from_nothing_is_not_reported() {
    // A panel that morphs open from zero lays out to nothing on its first
    // frames with its whole subtree inside. That is the animation, not a
    // forgotten size, so neither it nor anything inside it is reported while
    // the size moves.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "grow.lua",
            br#"
                local ui = require("morf.ui")
                _G.panel = ui.Rect {
                    width = 0, height = 0,
                    behavior = { width = { duration = 200 }, height = { duration = 200 } },
                    ui.Column { ui.Text { text = "inside" } },
                }
                ui.Rect { _G.panel }
                _G.panel.width = 120
                _G.panel.height = 40
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let layout = morf_layout::Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 100.0,
            height: 100.0,
        },
        &mut NoText,
    )
    .unwrap();
    runtime.lint_layout(&layout, root);
    runtime.poll_services();
    let logs = runtime.take_logs();
    assert!(
        logs.iter()
            .all(|log| !log.message.contains("laid out to nothing")),
        "an opening panel is not a mistake: {logs:?}"
    );
}

#[test]
fn a_container_of_hidden_rows_is_not_reported() {
    // A list of rows each shown only while it has something to show -- the
    // desk's task list with no tasks -- lays out to nothing, and so does the
    // column holding it. Nothing in either would be seen at any size, so
    // neither is a mistake; a hidden subtree is passed over altogether.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "hidden.lua",
            br#"
                local ui = require("morf.ui")
                ui.Rect {
                    ui.Column {
                        width = 50,
                        ui.Column {
                            width = 50,
                            ui.Rect { width = 40, height = 20, visible = false },
                            ui.Rect { width = 40, height = 20, visible = false },
                        },
                    },
                    ui.Rect {
                        width = 0, height = 0, visible = false,
                        ui.Text { text = "hidden with its parent" },
                    },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let layout = morf_layout::Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 100.0,
            height: 100.0,
        },
        &mut NoText,
    )
    .unwrap();
    runtime.lint_layout(&layout, root);
    runtime.poll_services();
    let logs = runtime.take_logs();
    assert!(
        logs.iter()
            .all(|log| !log.message.contains("laid out to nothing")),
        "hidden rows are not lost ones: {logs:?}"
    );
}

#[test]
fn capabilities_reach_the_configuration_and_the_wire() {
    // "Is there screencopy here" is a question a shell should be able to ask
    // rather than try and read the error; `morf info` is the same question
    // from a terminal.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "caps.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                assert(next(morf.capabilities) == nil, "empty before the connection, not absent")
                -- A plain table, like `morf.windows`: read when asked, not bound.
                morf.ipc.caps = function()
                    local c = morf.capabilities
                    return tostring(c.screencopy) .. "/" .. tostring(c.gpu) .. "/" .. tostring(c.scale_120)
                end
                ui.Text { text = "caps" }
            "#,
        )
        .unwrap();
    runtime.set_capabilities(&[
        ("screencopy".to_owned(), "true".to_owned()),
        ("gpu".to_owned(), "Intel".to_owned()),
        ("scale_120".to_owned(), "240".to_owned()),
    ]);
    assert_eq!(
        runtime.call_ipc("caps", &[]).unwrap(),
        [IpcValue::String("true/Intel/240".to_owned())],
        "a boolean, a string and a number each arrive as their own kind"
    );
    assert_eq!(
        runtime.capabilities(),
        ["screencopy=true", "gpu=Intel", "scale_120=240"]
    );
}
