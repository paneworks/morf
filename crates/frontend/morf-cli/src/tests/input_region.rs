// The input region a surface hands the compositor: kept between frames where
// nothing in the tree moved, and following a change that needs no new layout.

use std::path::PathBuf;
use std::time::Duration;

use morf_app::{PRIMARY_LAYER, WindowId};
use morf_host::headless::{Headless, LoadOptions};
use morf_value::IpcValue;

const TWO_AREAS: &str = r#"
    local ui = require("morf.ui")
    morf.surface.anchors = {}
    morf.surface.width = 200
    morf.surface.height = 100
    local second = morf.signal("second", true)
    morf.ipc.second = function(on) second:set(on == "on") return true end
    ui.Item {
        anchors = { fill = true },
        ui.MouseArea { x = 10, y = 10, width = 50, height = 20 },
        ui.MouseArea {
            x = 100, y = 10, width = 50, height = 20,
            enabled = function() return second:get() end,
        },
    }
"#;

fn load() -> Headless {
    let mut options = LoadOptions::new(PathBuf::from("input-region-test.lua"));
    options.source = Some(TWO_AREAS.as_bytes().to_vec());
    options.size = (800, 600);
    Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error))
}

fn areas(headless: &Headless) -> Vec<i32> {
    headless
        .input_region(WindowId::Layer(PRIMARY_LAYER))
        .expect("the surface has an input region")
        .iter()
        .map(|rect| rect.x)
        .collect()
}

fn second(headless: &mut Headless, on: &str) {
    headless
        .runtime
        .call_ipc("second", &[IpcValue::String(on.into())])
        .unwrap();
    headless.advance(Duration::from_millis(100), Duration::ZERO);
}

#[test]
fn a_change_with_no_new_layout_still_reaches_the_input_region() {
    let mut headless = load();
    headless.advance(Duration::from_millis(100), Duration::ZERO);
    assert_eq!(areas(&headless), [10, 100]);
    // Frames where nothing moved keep the region as it was.
    headless.advance(Duration::from_millis(100), Duration::ZERO);
    assert_eq!(areas(&headless), [10, 100]);
    second(&mut headless, "off");
    assert_eq!(areas(&headless), [10], "a disabled area takes no input");
    second(&mut headless, "on");
    assert_eq!(areas(&headless), [10, 100]);
}
