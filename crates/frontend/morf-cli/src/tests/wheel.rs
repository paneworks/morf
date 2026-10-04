// The wheel bubbles: it reaches the topmost area that would do something with
// it, passing over the buttons and switches laid on a scrolling page.

use crate::pointer_cursor::CursorShapes;
use crate::surface_pointer::handle_pointer_event;
use crate::surfaces::{PointerInput, SurfaceLayouts};
use morf_layout::{Layout, Size};
use morf_lua::Runtime;
use morf_value::IpcValue;
use morf_scene::NodeHandle;
use morf_app::{LayerEvent, SurfaceRole};

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

struct Shapes;

impl CursorShapes for Shapes {
    fn set_cursor_shape(&mut self, _shape: &str) {}
}

struct One(Layout);

impl SurfaceLayouts for One {
    fn layout_of(&self, _surface: SurfaceRole) -> Option<&Layout> {
        Some(&self.0)
    }
}

fn runtime(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime.execute("wheel.lua", source.as_bytes()).unwrap();
    runtime
}

fn laid_out(runtime: &Runtime) -> One {
    let root = runtime.scene().roots()[0];
    One(Layout::compute(
        &runtime.scene(),
        root,
        Size {
            width: 400.0,
            height: 300.0,
        },
        &mut NoText,
    )
    .unwrap())
}

fn wheel(runtime: &mut Runtime, x: f64, y: f64, vertical: f64) -> bool {
    let layouts = laid_out(runtime);
    let event = LayerEvent::PointerAxis {
        surface: SurfaceRole::Layer(morf_app::PRIMARY_LAYER),
        x,
        y,
        horizontal: 0.0,
        vertical,
        horizontal_steps: 0,
        vertical_steps: vertical.signum() as i32,
        modifiers: Default::default(),
    };
    let mut input = PointerInput::default();
    match handle_pointer_event(runtime, &mut Shapes, &mut input, &layouts, event) {
        Ok(Ok(repaint)) => repaint,
        Ok(Err(event)) => panic!("pointer path handed back {event:?}"),
        Err(error) => panic!("pointer path failed: {error}"),
    }
}

fn ask(runtime: &mut Runtime, verb: &str) -> IpcValue {
    runtime.call_ipc(verb, &[]).unwrap()[0].clone()
}

fn number(value: IpcValue) -> f64 {
    match value {
        IpcValue::Number(number) => number,
        IpcValue::Integer(number) => number as f64,
        other => panic!("not a number: {other:?}"),
    }
}

const PAGE: &str = r##"
    local ui = require("morf.ui")
    local page, dial = 0, 0
    morf.ipc.page = function() return page end
    morf.ipc.dial = function() return dial end
    ui.MouseArea {
        width = 400, height = 300,
        on_wheel = function() page = page + 1 end,
        -- A switch: it clicks, and has nothing to do with a wheel.
        ui.MouseArea {
            x = 10, y = 10, width = 100, height = 40,
            on_clicked = function() end,
        },
        -- A dial: it turns with the wheel itself.
        ui.MouseArea {
            x = 10, y = 100, width = 100, height = 40,
            on_wheel = function() dial = dial + 1 end,
        },
    }
"##;

#[test]
fn a_wheel_passes_over_an_area_with_no_wheel_handler() {
    let mut runtime = runtime(PAGE);
    assert!(wheel(&mut runtime, 20.0, 20.0, 15.0));
    assert_eq!(ask(&mut runtime, "page"), IpcValue::Integer(1));
    assert_eq!(ask(&mut runtime, "dial"), IpcValue::Integer(0));
}

#[test]
fn an_area_with_a_wheel_handler_keeps_its_wheel() {
    let mut runtime = runtime(PAGE);
    assert!(wheel(&mut runtime, 20.0, 110.0, 15.0));
    assert_eq!(ask(&mut runtime, "dial"), IpcValue::Integer(1));
    assert_eq!(ask(&mut runtime, "page"), IpcValue::Integer(0));
}

/// A long list (160 tall in a 100 tall viewport) with a button at its top,
/// a short one that fits, and a page-wide wheel handler under both.
const LISTS: &str = r##"
    local ui = require("morf.ui")
    local outer = 0
    local long = ui.Flickable {
        width = 200, height = 100,
        ui.Column {
            ui.MouseArea { width = 200, height = 80, on_clicked = function() end },
            ui.Item { width = 200, height = 80 },
        },
    }
    local short = ui.Flickable {
        y = 200, width = 200, height = 100,
        ui.Item { width = 200, height = 50 },
    }
    morf.ipc.outer = function() return outer end
    morf.ipc.long = function() return long.content_y end
    morf.ipc.short = function() return short.content_y end
    ui.MouseArea {
        width = 400, height = 300,
        on_wheel = function() outer = outer + 1 end,
        ui.Item { width = 400, height = 300, long, short },
    }
"##;

#[test]
fn a_flickable_scrolls_under_a_button_and_stops_at_its_end() {
    let mut runtime = runtime(LISTS);
    assert!(wheel(&mut runtime, 20.0, 20.0, 15.0));
    assert_eq!(number(ask(&mut runtime, "long")), 15.0);
    assert_eq!(ask(&mut runtime, "outer"), IpcValue::Integer(0));
    // Only 60 of content lie past the viewport.
    wheel(&mut runtime, 20.0, 20.0, 500.0);
    assert_eq!(number(ask(&mut runtime, "long")), 60.0);
    wheel(&mut runtime, 20.0, 20.0, -500.0);
    assert_eq!(number(ask(&mut runtime, "long")), 0.0);
}

#[test]
fn a_flickable_with_nothing_to_scroll_lets_the_wheel_through() {
    let mut runtime = runtime(LISTS);
    assert!(wheel(&mut runtime, 20.0, 220.0, 15.0));
    assert_eq!(number(ask(&mut runtime, "short")), 0.0);
    assert_eq!(ask(&mut runtime, "outer"), IpcValue::Integer(1));
}
