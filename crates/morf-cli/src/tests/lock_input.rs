// The lock screen's input: pointer, touch and wheel on lock surfaces go
// through the same hit test and handlers a layer surface's do.

use crate::lock_outputs::{LockLayouts, LockOutput};
use crate::pointer_cursor::CursorShapes;
use crate::surface_pointer::handle_pointer_event;
use crate::surfaces::PointerInput;
use morf_layout::{Layout, Size};
use morf_lua::{IpcValue, Runtime};
use morf_scene::NodeHandle;
use morf_wayland::{LayerEvent, SurfaceRole};

struct NoText;

impl morf_layout::TextMeasurer for NoText {
    fn measure(
        &mut self,
        _node: NodeHandle,
        _text: &str,
        _family: &str,
        _size: f64,
        _options: morf_layout::TextOptions,
    ) -> Size {
        Size::default()
    }
}

#[derive(Default)]
struct Shapes(Vec<String>);

impl CursorShapes for Shapes {
    fn set_cursor_shape(&mut self, shape: &str) {
        self.0.push(shape.to_owned());
    }
}

/// A lock with one button in its corner that counts clicks and wheel steps.
fn lock_runtime() -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-input.lua",
            br##"
                local ui = require("morf.ui")
                local clicks, wheels = 0, 0
                morf.surface.session_lock = true
                morf.ipc.clicks = function() return clicks end
                morf.ipc.wheels = function() return wheels end
                ui.Rect {
                    width = 400, height = 300, color = "#000000",
                    ui.MouseArea {
                        x = 10, y = 10, width = 100, height = 50,
                        cursor = "pointer",
                        on_clicked = function() clicks = clicks + 1 end,
                        on_wheel = function() wheels = wheels + 1 end,
                    },
                }
            "##,
        )
        .unwrap();
    runtime
}

fn laid_out(runtime: &Runtime) -> Layout {
    let root = runtime.scene().roots()[0];
    Layout::compute(
        &runtime.scene(),
        root,
        Size {
            width: 400.0,
            height: 300.0,
        },
        &mut NoText,
    )
    .unwrap()
}

fn count(runtime: &mut Runtime, verb: &str) -> IpcValue {
    runtime.call_ipc(verb, &[]).unwrap()[0].clone()
}

fn send(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    outputs: &[LockOutput],
    shapes: &mut Shapes,
    event: LayerEvent,
) -> bool {
    match handle_pointer_event(runtime, shapes, input, &LockLayouts(outputs), event) {
        Ok(Ok(repaint)) => repaint,
        Ok(Err(event)) => panic!("pointer path handed back {event:?}"),
        Err(error) => panic!("pointer path failed: {error}"),
    }
}

#[test]
fn a_click_on_a_lock_surface_reaches_its_mouse_area() {
    let mut runtime = lock_runtime();
    let outputs = vec![LockOutput {
        layout: Some(laid_out(&runtime)),
        ..LockOutput::default()
    }];
    let mut input = PointerInput::default();
    let mut shapes = Shapes::default();
    let surface = SurfaceRole::Lock(0);
    for event in [
        LayerEvent::PointerMotion {
            surface,
            x: 20.0,
            y: 20.0,
        },
        LayerEvent::PointerButton {
            surface,
            button: 0x110,
            pressed: true,
            x: 20.0,
            y: 20.0,
        },
        LayerEvent::PointerButton {
            surface,
            button: 0x110,
            pressed: false,
            x: 20.0,
            y: 20.0,
        },
    ] {
        send(&mut runtime, &mut input, &outputs, &mut shapes, event);
    }
    assert_eq!(count(&mut runtime, "clicks"), IpcValue::Integer(1));
    // The hovered area's cursor, asked of the compositor on the way in.
    assert_eq!(shapes.0, ["pointer"]);

    send(
        &mut runtime,
        &mut input,
        &outputs,
        &mut shapes,
        LayerEvent::PointerAxis {
            surface,
            x: 20.0,
            y: 20.0,
            horizontal: 0.0,
            vertical: 15.0,
            horizontal_steps: 0,
            vertical_steps: 1,
        },
    );
    assert_eq!(count(&mut runtime, "wheels"), IpcValue::Integer(1));
}

#[test]
fn a_tap_on_a_lock_surface_is_a_click() {
    let mut runtime = lock_runtime();
    let outputs = vec![LockOutput {
        layout: Some(laid_out(&runtime)),
        ..LockOutput::default()
    }];
    let mut input = PointerInput::default();
    let mut shapes = Shapes::default();
    let surface = SurfaceRole::Lock(0);
    send(
        &mut runtime,
        &mut input,
        &outputs,
        &mut shapes,
        LayerEvent::TouchDown {
            surface,
            id: 3,
            x: 30.0,
            y: 30.0,
        },
    );
    send(
        &mut runtime,
        &mut input,
        &outputs,
        &mut shapes,
        LayerEvent::TouchUp {
            surface,
            id: 3,
            x: 30.0,
            y: 30.0,
        },
    );
    assert_eq!(count(&mut runtime, "clicks"), IpcValue::Integer(1));
}

#[test]
fn a_lock_surface_is_hit_tested_against_its_own_layout() {
    let mut runtime = lock_runtime();
    // The second output has not drawn yet: nothing to hit, so nothing clicks,
    // and the first output's layout is not borrowed for it.
    let outputs = vec![
        LockOutput {
            layout: Some(laid_out(&runtime)),
            ..LockOutput::default()
        },
        LockOutput::default(),
    ];
    let mut input = PointerInput::default();
    let mut shapes = Shapes::default();
    let surface = SurfaceRole::Lock(1);
    for pressed in [true, false] {
        send(
            &mut runtime,
            &mut input,
            &outputs,
            &mut shapes,
            LayerEvent::PointerButton {
                surface,
                button: 0x110,
                pressed,
                x: 20.0,
                y: 20.0,
            },
        );
    }
    assert_eq!(count(&mut runtime, "clicks"), IpcValue::Integer(0));
    // A key is not the pointer path's, and comes back untouched.
    let key = LayerEvent::Key {
        surface,
        keysym: 0x61,
        text: Some("a".to_owned()),
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    let handed_back = handle_pointer_event(
        &mut runtime,
        &mut shapes,
        &mut input,
        &LockLayouts(&outputs),
        key.clone(),
    )
    .unwrap();
    assert_eq!(handed_back, Err(key));
}

#[test]
fn pointer_entry_before_the_first_lock_frame_selects_that_surface_after_layout() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "=initial-lock-pointer",
            br##"
                local ui = require("morf.ui")
                local root = ui.Rect { width = 300, height = 200, color = "#000000" }
                local inside = false
                morf.effect("pointer", function() inside = root.contains_pointer end, { owner = root })
                morf.ipc.inside = function() return inside end
            "##,
        )
        .unwrap();
    let mut outputs = vec![LockOutput::default()];
    let mut input = PointerInput::default();
    let mut shapes = Shapes::default();
    send(
        &mut runtime,
        &mut input,
        &outputs,
        &mut shapes,
        LayerEvent::PointerMotion {
            surface: SurfaceRole::Lock(0),
            x: 20.0,
            y: 20.0,
        },
    );
    assert_eq!(count(&mut runtime, "inside"), IpcValue::Boolean(false));
    outputs[0].layout = Some(laid_out(&runtime));
    assert!(crate::surface_pointer::answer_new_containment(
        &mut runtime,
        &input,
        &LockLayouts(&outputs),
    ));
    assert_eq!(count(&mut runtime, "inside"), IpcValue::Boolean(true));
}
