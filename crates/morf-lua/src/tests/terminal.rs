//! `ui.Terminal`: a real program on a real pseudo-terminal, laid out, fed
//! through `poll_services` as the loop feeds it, and read back with
//! `:text()`.

use std::thread;
use std::time::{Duration, Instant};

use morf_scene::NodeHandle;

use super::*;

const RETURN: u32 = 0xff0d;
const NONE: KeyModifiers = KeyModifiers {
    ctrl: false,
    shift: false,
    alt: false,
    logo: false,
};
const CTRL: KeyModifiers = KeyModifiers {
    ctrl: true,
    shift: false,
    alt: false,
    logo: false,
};

fn start(source: &str) -> (Runtime, NodeHandle) {
    start_with(Runtime::default(), source)
}

fn start_with(mut runtime: Runtime, source: &str) -> (Runtime, NodeHandle) {
    runtime
        .execute(
            "terminal.lua",
            format!("morf = require(\"morf\")\nui = require(\"morf.ui\")\n{source}").as_bytes(),
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    (runtime, node)
}

/// Lays the terminal out at `width` × `height`, which starts its program.
fn lay_out(
    runtime: &mut Runtime,
    node: NodeHandle,
    width: f64,
    height: f64,
) -> morf_text::TextSystem {
    let mut text = morf_text::TextSystem::new();
    let layout = runtime
        .compute_layout(node, morf_layout::Size { width, height }, &mut text)
        .unwrap();
    runtime.sync_text_inputs(&layout, &mut text);
    text
}

/// Pumps the runtime until `check` runs without error.
fn wait_for(runtime: &mut Runtime, check: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        runtime.poll_services();
        let error = match runtime.execute("check.lua", check.as_bytes()) {
            Ok(()) => return,
            Err(error) => error,
        };
        if Instant::now() > deadline {
            panic!("never became true: {check}\nlast error: {error}");
        }
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_terminal_runs_its_program_at_its_laid_out_size() {
    let (mut runtime, node) = start(
        r#"
        exited = nil
        term = ui.Terminal {
            width = 400, height = 200, font_size = 13,
            command = { "sh", "-c", "printf 'hello\\n\\033[31mworld\\033[0m\\n'; stty size" },
            on_exit = function(code, signal) exited = { code = code, signal = signal } end,
        }
        assert(term.running == false, "nothing runs until it is laid out")
        "#,
    );
    lay_out(&mut runtime, node, 400.0, 200.0);
    wait_for(
        &mut runtime,
        r#"
        assert(exited, "no exit yet")
        assert(exited.code == 0 and exited.signal == nil)
        assert(term.columns > 20 and term.rows > 5, term.columns .. "x" .. term.rows)
        local expected = ("hello\nworld\n%d %d"):format(term.rows, term.columns)
        assert(term:text() == expected, term:text())
        assert(term.running == false and term.exit_code == 0)
        "#,
    );
    let screen = runtime
        .scene()
        .terminal_screen(node)
        .cloned()
        .expect("a picture of the screen for the renderer");
    assert_eq!(screen.lines[0].text(), "hello");
    // Red, from the program, resolved to the default palette's red.
    assert_eq!(
        screen.lines[1].cells[0].foreground,
        morf_terminal::Palette::default().ansi[1]
    );
}

#[test]
fn keys_reach_the_program_and_its_title_comes_back() {
    let (mut runtime, node) = start(
        r#"
        titles, exited = {}, nil
        term = ui.Terminal {
            width = 400, height = 200,
            command = { "sh", "-c", "printf '\\033]0;my title\\007'; exec cat" },
            on_title = function(title) titles[#titles + 1] = title end,
            on_exit = function(code) exited = code end,
        }
        "#,
    );
    lay_out(&mut runtime, node, 400.0, 200.0);
    wait_for(
        &mut runtime,
        r#"assert(term.title == "my title" and titles[1] == "my title")"#,
    );
    assert_eq!(runtime.key_target_for_node(node), Some(node));
    runtime.dispatch_key(node, 'h' as u32, Some("h"), NONE);
    runtime.dispatch_key(node, 'i' as u32, Some("i"), NONE);
    runtime.dispatch_key(node, RETURN, Some("\r"), NONE);
    // The terminal echoes the line, then cat prints it back.
    wait_for(
        &mut runtime,
        r#"assert(term:text() == "hi\nhi", term:text())"#,
    );
    // ^D ends cat's input.
    runtime.dispatch_key(node, 'd' as u32, Some("\u{4}"), CTRL);
    wait_for(&mut runtime, r#"assert(exited == 0, tostring(exited))"#);
}

#[test]
fn write_and_paste_go_to_the_program() {
    let (mut runtime, node) = start(
        r#"
        term = ui.Terminal { width = 400, height = 200, command = { "cat" } }
        -- Written before it starts, and kept until it has.
        assert(term:write("early\r") == true)
        "#,
    );
    lay_out(&mut runtime, node, 400.0, 200.0);
    wait_for(
        &mut runtime,
        r#"assert(term:text() == "early\nearly", term:text())"#,
    );
    runtime
        .execute("paste.lua", br#"assert(term:paste("pasted\n") == true)"#)
        .unwrap();
    wait_for(
        &mut runtime,
        r#"assert(term:text() == "early\nearly\npasted\npasted", term:text())"#,
    );
    runtime
        .execute("kill.lua", br#"assert(term:kill("KILL") == true)"#)
        .unwrap();
    wait_for(
        &mut runtime,
        r#"assert(term.running == false and term.exit_code == 137)"#,
    );
}

#[test]
fn a_program_that_cannot_start_says_so() {
    let (mut runtime, node) = start(
        r#"
        exited = nil
        term = ui.Terminal {
            width = 400, height = 100,
            command = { "/nonexistent/program" },
            on_exit = function(code) exited = code end,
        }
        "#,
    );
    lay_out(&mut runtime, node, 400.0, 100.0);
    wait_for(
        &mut runtime,
        r#"
        assert(exited == 127, tostring(exited))
        assert(term:text():find("/nonexistent/program", 1, true), term:text())
        "#,
    );
}

#[test]
fn what_the_runtime_keeps_is_read_only() {
    let (mut runtime, _) =
        start(r#"term = ui.Terminal { width = 100, height = 100, command = { "true" } }"#);
    for property in ["columns", "rows", "title", "running", "exit_code"] {
        let error = runtime
            .execute("write.lua", format!("term.{property} = 1").as_bytes())
            .unwrap_err();
        assert!(
            error.to_string().contains("read-only"),
            "{property}: {error}"
        );
    }
}

#[test]
fn terminals_are_counted() {
    let limits = Limits {
        terminals: 1,
        ..Limits::default()
    };
    let (mut runtime, _) = start_with(
        Runtime::new(limits),
        r#"a = ui.Terminal { width = 10, height = 10, command = { "true" } }"#,
    );
    let error = runtime
        .execute(
            "b.lua",
            br#"b = ui.Terminal { width = 10, height = 10, command = { "true" } }"#,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("more than 1 terminals"),
        "{error}"
    );
    // One destroyed makes room for another.
    runtime
        .execute(
            "c.lua",
            br#"ui.destroy(a); c = ui.Terminal { width = 10, height = 10, command = { "true" } }"#,
        )
        .unwrap();
}

#[test]
fn destroying_a_terminal_hangs_its_program_up() {
    let (mut runtime, node) =
        start(r#"term = ui.Terminal { width = 200, height = 100, command = { "sleep", "100" } }"#);
    lay_out(&mut runtime, node, 200.0, 100.0);
    wait_for(&mut runtime, r#"pid = term:pid(); assert(pid and pid > 0)"#);
    runtime
        .execute("pid.lua", b"morf.ipc.pid = function() return pid end")
        .unwrap();
    let Ok(values) = runtime.call_ipc("pid", &[]) else {
        panic!("no pid");
    };
    let Some(IpcValue::Integer(pid)) = values.first().cloned() else {
        panic!("no pid: {values:?}");
    };
    runtime.execute("destroy.lua", b"ui.destroy(term)").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    // Reaped by the reactor once it goes: then there is no such process.
    while std::path::Path::new(&format!("/proc/{pid}")).exists() {
        assert!(
            Instant::now() < deadline,
            "the program outlived its terminal"
        );
        runtime.poll_services();
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_wheel_scrolls_the_history_when_the_program_does_not_want_it() {
    let (mut runtime, node) = start(
        r#"
        exited = nil
        term = ui.Terminal {
            width = 300, height = 80, scrollback = 100,
            command = { "sh", "-c", "for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do echo line$i; done" },
            on_exit = function(code) exited = code end,
        }
        "#,
    );
    lay_out(&mut runtime, node, 300.0, 80.0);
    wait_for(
        &mut runtime,
        r#"assert(exited == 0); assert(term:text():find("line20"))"#,
    );
    assert!(runtime.takes_wheel(node));
    // One detent up is three lines into the history.
    runtime.dispatch_wheel_event(
        node,
        EventPoint::new((10.0, 10.0), (10.0, 10.0)),
        (0.0, -15.0),
        (0, -1),
    );
    runtime
        .execute(
            "scrolled.lua",
            br#"assert(not term:text():find("line20"), term:text())"#,
        )
        .unwrap();
    runtime
        .execute(
            "bottom.lua",
            br#"assert(term:scroll() == true); assert(term:text():find("line20"))"#,
        )
        .unwrap();
}

#[test]
fn a_key_its_handler_claims_never_reaches_the_program() {
    let (mut runtime, node) = start(
        r#"
        claimed = {}
        term = ui.Terminal {
            width = 400, height = 200,
            command = { "cat" },
            on_key_pressed = function(keysym, text)
                if text == "x" then claimed[#claimed + 1] = text return true end
            end,
        }
        "#,
    );
    lay_out(&mut runtime, node, 400.0, 200.0);
    wait_for(&mut runtime, "assert(term.running)");
    for (keysym, text) in [('a' as u32, "a"), ('x' as u32, "x"), ('b' as u32, "b")] {
        runtime.dispatch_key(node, keysym, Some(text), NONE);
    }
    runtime.dispatch_key(node, RETURN, Some("\r"), NONE);
    // "x" was the handler's; the handler saw "a" and "b" too and let them by.
    wait_for(
        &mut runtime,
        r#"assert(term:text() == "ab\nab", term:text()) assert(#claimed == 1)"#,
    );
}
