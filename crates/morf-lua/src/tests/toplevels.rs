use crate::*;

fn window(identifier: &str, title: &str) -> Toplevel {
    Toplevel {
        identifier: identifier.into(),
        title: title.into(),
        app_id: format!("{identifier}.app"),
        controllable: true,
        ..Toplevel::default()
    }
}

fn texts(runtime: &Runtime, node: morf_scene::NodeHandle, out: &mut Vec<String>) {
    if let Ok(text) = runtime.scene().string_value(node, "text") {
        out.push(text.to_owned());
    }
    for child in runtime.scene().children(node).unwrap_or_default() {
        texts(runtime, *child, out);
    }
}

#[test]
fn bindings_and_repeaters_follow_the_compositor_windows() {
    // A dock used to poll `morf.windows` on a timer, because the table
    // changed under it with no word. `morf.toplevels` is tracked, keyed and
    // announced.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "dock.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local heard = {}
                local count = ui.Text {
                    text = function() return tostring(#morf.toplevels.list()) end,
                }
                ui.Column {
                    ui.Repeater {
                        model = morf.toplevels.model,
                        delegate = function(window)
                            return ui.Text { text = window.title }
                        end,
                    },
                }
                local active = ui.Text {
                    text = function()
                        local first = morf.toplevels.get("a")
                        return first and tostring(first.activated) or "gone"
                    end,
                }
                morf.toplevels.on_changed(function(change)
                    heard[#heard + 1] = #change.opened .. "/" .. #change.closed
                        .. "/" .. #change.changed
                end)
                morf.ipc.heard = function() return table.concat(heard, " ") end
                morf.ipc.snapshot = function() return #morf.windows end
            "#,
        )
        .unwrap();
    let roots = runtime.scene().roots();
    let (count, column, active) = (roots[0], roots[1], roots[2]);
    let text = |runtime: &Runtime, node| {
        runtime
            .scene()
            .string_value(node, "text")
            .unwrap()
            .to_owned()
    };
    let rows = |runtime: &Runtime| {
        let mut out = Vec::new();
        texts(runtime, column, &mut out);
        out
    };
    assert_eq!(text(&runtime, count), "0");
    assert_eq!(text(&runtime, active), "gone");

    runtime.set_windows(&[window("a", "Editor"), window("b", "Terminal")]);
    runtime.poll_services();
    assert_eq!(text(&runtime, count), "2");
    assert_eq!(rows(&runtime), vec!["Editor", "Terminal"]);
    assert_eq!(text(&runtime, active), "false");
    // The plain snapshot is still there.
    assert_eq!(
        runtime.call_ipc("snapshot", &[]).unwrap(),
        [IpcValue::Integer(2)]
    );

    // A retitle and a focus change are changes, not a close and an open.
    let mut focused = window("a", "Editor - notes");
    focused.activated = true;
    runtime.set_windows(&[focused.clone(), window("b", "Terminal")]);
    runtime.poll_services();
    assert_eq!(rows(&runtime), vec!["Editor - notes", "Terminal"]);
    assert_eq!(text(&runtime, active), "true");

    // The same list again says nothing.
    runtime.set_windows(&[focused, window("b", "Terminal")]);
    runtime.set_windows(&[window("b", "Terminal")]);
    runtime.poll_services();
    assert_eq!(text(&runtime, count), "1");
    assert_eq!(rows(&runtime), vec!["Terminal"]);
    assert_eq!(text(&runtime, active), "gone");
    assert_eq!(
        runtime.call_ipc("heard", &[]).unwrap(),
        [IpcValue::String("2/0/0 0/0/1 0/1/0".into())]
    );
}
