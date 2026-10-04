//! The compositor bridges: idle, output power, gamma, clipboard,
//! screencopy, the virtual keyboard and the window list.

use super::*;

#[test]
fn idle_callbacks_receive_compositor_state() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "idle.lua",
            br#"
                local idle = morf.signal("idle", false)
                morf.idle.subscribe(30000, function(value) idle:set(value) end)
                morf.ipc["idle.get"] = function() return idle:get() end
            "#,
        )
        .unwrap();

    assert_eq!(runtime.idle_timeouts(), [(30_000, false)]);
    assert!(runtime.dispatch_idle(30_000, false, true));
    assert_eq!(
        runtime.call_ipc("idle.get", &[]).unwrap(),
        [IpcValue::Boolean(true)]
    );
}

#[test]
fn output_power_requests_are_bounded_and_ordered() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "power.lua",
            br#"
                morf.output_power.set("off")
                morf.output_power.set("on")
            "#,
        )
        .unwrap();

    assert_eq!(runtime.take_output_power_requests(), [false, true]);
    assert!(runtime.take_output_power_requests().is_empty());
}

#[test]
fn gamma_requests_keep_the_last_word_per_output() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "gamma.lua",
            br#"
                assert(morf.gamma.supported() == false, "unknown until the shell connects")
                for k = 6500, 3000, -500 do morf.gamma.set { temperature = k } end
                morf.gamma.set { output = "DP-1", temperature = 4000, brightness = 0.8, gamma = 1.1 }
                morf.gamma.reset("HDMI-A-1")
                assert(not pcall(morf.gamma.set, { temperature = 100 }))
                assert(not pcall(morf.gamma.set, { brightness = 2 }))
                assert(not pcall(morf.gamma.set, { warmth = 1 }))
                assert(not pcall(morf.gamma.set, { output = 3 }))
            "#,
        )
        .unwrap();
    let requests = runtime.take_gamma_requests();
    assert_eq!(
        requests,
        [
            GammaRequest {
                output: None,
                set: Some((3000.0, 1.0, 1.0)),
            },
            GammaRequest {
                output: Some("DP-1".to_owned()),
                set: Some((4000.0, 0.8, 1.1)),
            },
            GammaRequest {
                output: Some("HDMI-A-1".to_owned()),
                set: None,
            },
        ]
    );
    runtime.set_capabilities(&[("gamma_control".to_owned(), "true".to_owned())]);
    runtime
        .execute(
            "later.lua",
            b"assert(morf.gamma.supported()) morf.gamma.reset()",
        )
        .unwrap();
    assert_eq!(
        runtime.take_gamma_requests(),
        [GammaRequest {
            output: None,
            set: None
        }]
    );
}

#[test]
fn clipboard_bridges_publications_and_selections() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "clipboard.lua",
            br#"
                local current = morf.signal("clipboard", "")
                morf.clipboard.subscribe(function(text) current:set(text or "none") end)
                morf.clipboard.set("copied")
                morf.ipc["clipboard.get"] = function() return current:get() end
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.take_clipboard_requests(),
        [ClipboardRequest {
            data: b"copied".to_vec(),
            mime: None,
            primary: false,
        }]
    );
    assert!(runtime.dispatch_clipboard(Some("pasted".to_owned())));
    assert_eq!(
        runtime.call_ipc("clipboard.get", &[]).unwrap(),
        [IpcValue::String("pasted".to_owned())]
    );
    assert!(runtime.dispatch_clipboard(None));
    assert_eq!(
        runtime.call_ipc("clipboard.get", &[]).unwrap(),
        [IpcValue::String("none".to_owned())]
    );
}

#[test]
fn screencopy_bridges_bounded_requests_and_pixels() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "screencopy.lua",
            br#"
                local result = morf.signal("capture", "pending")
                morf.screencopy.capture(true, function(frame, err)
                    if err then
                        result:set(err)
                    else
                        result:set(frame.format .. ":" .. frame.width .. ":" ..
                            #frame.pixels .. ":" .. string.byte(frame.pixels, 1))
                    end
                end)
                local second = morf.signal("second", "pending")
                morf.screencopy.capture(false, function(_, err) second:set(err) end)
                morf.ipc["capture.get"] = function() return result:get() end
                morf.ipc["second.get"] = function() return second:get() end
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.take_screencopy_requests(),
        [
            ScreencopyRequest {
                id: 0,
                include_cursor: true,
                window: None,
                gpu: false,
                name: None,
                output: None,
            },
            ScreencopyRequest {
                id: 1,
                include_cursor: false,
                window: None,
                gpu: false,
                name: None,
                output: None,
            },
        ]
    );
    assert!(runtime.dispatch_screencopy(1, Err("second failed".to_owned())));
    assert!(runtime.dispatch_screencopy(
        0,
        Ok(Screencopy {
            width: 2,
            height: 1,
            stride: 8,
            format: "argb8888".to_owned(),
            y_invert: false,
            gpu: false,
            source: "memory:capture/0".to_owned(),
            pixels: vec![7; 8],
        })
    ));
    assert_eq!(
        runtime.call_ipc("capture.get", &[]).unwrap(),
        [IpcValue::String("argb8888:2:8:7".to_owned())]
    );
    assert_eq!(
        runtime.call_ipc("second.get", &[]).unwrap(),
        [IpcValue::String("second failed".to_owned())]
    );
}

#[test]
fn virtual_keyboard_requests_preserve_protocol_order() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "keyboard.lua",
            br#"
                morf.virtual_keyboard.modifiers(1, 2, 4, 0)
                morf.virtual_keyboard.key(30, true)
                morf.virtual_keyboard.key(30, false)
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.take_virtual_keyboard_requests(),
        [
            VirtualKeyboardRequest::Modifiers {
                depressed: 1,
                latched: 2,
                locked: 4,
                group: 0,
            },
            VirtualKeyboardRequest::Key {
                keycode: 30,
                pressed: true,
            },
            VirtualKeyboardRequest::Key {
                keycode: 30,
                pressed: false,
            },
        ]
    );
}

#[test]
fn the_window_list_is_there_before_any_compositor_speaks() {
    // `morf.windows` exists from the first line of a configuration, empty, and
    // is filled in place when the compositor reports something. Empty rather
    // than absent so `#morf.windows` is a number on a compositor that does not
    // report windows at all, and so a configuration can capture the table and
    // watch it rather than having to ask for it again.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "windows.lua",
            br#"
                morf.ipc["count"] = function() return #morf.windows end
                morf.ipc["first"] = function()
                    local window = morf.windows[1]
                    return window and (window.app_id .. ":" .. window.title) or "none"
                end
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.call_ipc("count", &[]).unwrap(),
        [IpcValue::Integer(0)],
        "the table is there before any compositor has said anything",
    );

    runtime.set_windows(&[
        Toplevel {
            identifier: "b".to_owned(),
            title: "second".to_owned(),
            app_id: "kitty".to_owned(),
            ..Toplevel::default()
        },
        Toplevel {
            identifier: "a".to_owned(),
            title: "first".to_owned(),
            app_id: "zen".to_owned(),
            ..Toplevel::default()
        },
    ]);
    assert_eq!(
        runtime.call_ipc("count", &[]).unwrap(),
        [IpcValue::Integer(2)]
    );
    assert_eq!(
        runtime.call_ipc("first", &[]).unwrap(),
        [IpcValue::String("kitty:second".to_owned())],
        "the order handed in is the order seen",
    );

    // And a shorter list does not leave the tail of a longer one behind.
    runtime.set_windows(&[Toplevel {
        identifier: "a".to_owned(),
        title: "only".to_owned(),
        app_id: "zen".to_owned(),
        ..Toplevel::default()
    }]);
    assert_eq!(
        runtime.call_ipc("count", &[]).unwrap(),
        [IpcValue::Integer(1)],
        "the table is replaced, not appended to",
    );
}
