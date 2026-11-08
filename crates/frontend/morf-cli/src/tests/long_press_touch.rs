// A long press under a finger: once it fires, the rest of the contact is the
// long-pressed node's, and a move already queued when it comes due is seen
// before it.

use std::path::PathBuf;
use std::time::Duration;

use morf_app::{Backend, Event, PRIMARY_LAYER, WindowId};
use morf_host::headless::{Headless, LoadOptions};
use morf_value::IpcValue;

const PANEL: &str = r#"
    local ui = require("morf.ui")
    morf.surface.width, morf.surface.height = 400, 600
    local log = {}
    morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
    ui.Item { width = 400, height = 600,
        on_panned = function(phase) if phase == "begin" then log[#log + 1] = "pan" end end,
        on_swiped = function() log[#log + 1] = "swipe" end,
        ui.MouseArea { width = 400, height = 200,
            on_long_pressed = function() log[#log + 1] = "long" end,
            on_clicked = function() log[#log + 1] = "click" end,
            on_touch_canceled = function() log[#log + 1] = "canceled" end,
            on_touch_released = function() log[#log + 1] = "released" end },
    }
"#;

fn load() -> Headless {
    let mut options = LoadOptions::new(PathBuf::from("long-press-test.lua"));
    options.source = Some(PANEL.as_bytes().to_vec());
    options.size = (400, 600);
    Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error))
}

fn log(headless: &mut Headless) -> String {
    match headless.runtime.call_ipc("log", &[]).unwrap().remove(0) {
        IpcValue::String(text) => text,
        other => panic!("log answered {other:?}"),
    }
}

fn finger(kind: &str, x: f64, y: f64) -> Event {
    let surface = WindowId::Layer(PRIMARY_LAYER);
    match kind {
        "down" => Event::TouchDown {
            surface,
            id: 0,
            x,
            y,
            time_ms: None,
        },
        "move" => Event::TouchMotion {
            surface,
            id: 0,
            x,
            y,
            time_ms: None,
        },
        _ => Event::TouchUp {
            surface,
            id: 0,
            x,
            y,
            time_ms: None,
        },
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn a_long_pressed_contact_is_no_pans_and_no_swipes() {
    let mut headless = load();
    headless.pointer(finger("down", 100.0, 100.0)).unwrap();
    headless.advance(ms(600), Duration::ZERO);
    assert_eq!(log(&mut headless), "long");
    for step in 1..=8 {
        headless
            .pointer(finger("move", 100.0, 100.0 + 30.0 * f64::from(step)))
            .unwrap();
        headless.advance(ms(16), Duration::ZERO);
    }
    headless.pointer(finger("up", 100.0, 340.0)).unwrap();
    headless.advance(ms(300), Duration::ZERO);
    assert_eq!(log(&mut headless), "released");
}

#[test]
fn a_contact_that_moves_before_its_long_press_is_still_the_panels() {
    let mut headless = load();
    headless.pointer(finger("down", 100.0, 100.0)).unwrap();
    headless.advance(ms(300), Duration::ZERO);
    for step in 1..=8 {
        headless
            .pointer(finger("move", 100.0, 100.0 + 30.0 * f64::from(step)))
            .unwrap();
        headless.advance(ms(16), Duration::ZERO);
    }
    headless.pointer(finger("up", 100.0, 340.0)).unwrap();
    headless.advance(ms(300), Duration::ZERO);
    assert_eq!(log(&mut headless), "pan,canceled");
}

#[test]
fn a_move_queued_before_the_long_press_comes_due_is_seen_first() {
    let mut headless = load();
    headless.pointer(finger("down", 100.0, 100.0)).unwrap();
    headless.advance(ms(490), Duration::ZERO);
    // The move is queued; the loop only gets to it once the long press is due.
    headless.runtime.advance_virtual_clock(ms(20));
    let backend = headless
        .host
        .as_mut()
        .and_then(|host| host.backend.as_headless_mut());
    backend
        .expect("a headless backend")
        .inject(finger("move", 100.0, 130.0));
    headless.frame(Duration::ZERO);
    headless.advance(ms(100), Duration::ZERO);
    assert_eq!(log(&mut headless), "pan,canceled");
}
