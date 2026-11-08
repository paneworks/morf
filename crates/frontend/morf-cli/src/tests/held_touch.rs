// A tap held back under a pan (`morf_host::input::stitch`): the loop wakes
// for it on its own, and a settle waits for it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use morf_app::{Event, PRIMARY_LAYER, WindowId};
use morf_host::headless::{Headless, LoadOptions};
use morf_lua::DeadlineCause;
use morf_value::IpcValue;

const PANEL: &str = r#"
    local ui = require("morf.ui")
    morf.surface.width, morf.surface.height = 400, 600
    local clicks = 0
    morf.ipc.clicks = function() return clicks end
    ui.Item { width = 400, height = 600, on_panned = function() end,
        ui.MouseArea { width = 400, height = 200, on_clicked = function() clicks = clicks + 1 end },
    }
"#;

fn load() -> Headless {
    let mut options = LoadOptions::new(PathBuf::from("held-touch-test.lua"));
    options.source = Some(PANEL.as_bytes().to_vec());
    options.size = (400, 600);
    Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error))
}

fn clicks(headless: &mut Headless) -> IpcValue {
    headless.runtime.call_ipc("clicks", &[]).unwrap().remove(0)
}

fn tap(headless: &mut Headless) {
    let surface = WindowId::Layer(PRIMARY_LAYER);
    let (x, y, id, time_ms) = (100.0, 100.0, 0, None);
    let down = Event::TouchDown {
        surface,
        id,
        x,
        y,
        time_ms,
    };
    headless.pointer(down).unwrap();
    headless.advance(Duration::from_millis(40), Duration::ZERO);
    let up = Event::TouchUp {
        surface,
        id,
        x,
        y,
        time_ms,
    };
    headless.pointer(up).unwrap();
}

#[test]
fn a_held_tap_wakes_the_loop_when_it_is_due() {
    let mut headless = load();
    tap(&mut headless);
    assert_eq!(clicks(&mut headless), IpcValue::Integer(0));
    let (at, cause) = headless.runtime.next_deadline().expect("a deadline");
    assert_eq!(cause, DeadlineCause::HeldTouch);
    assert!(at <= Instant::now() + Duration::from_millis(61));
    headless.advance(Duration::from_millis(80), Duration::ZERO);
    assert_eq!(clicks(&mut headless), IpcValue::Integer(1));
    assert!(headless.runtime.next_deadline().is_none());
}

#[test]
fn a_settle_waits_for_a_held_tap() {
    let mut headless = load();
    tap(&mut headless);
    headless.settle(Duration::from_millis(1000));
    assert_eq!(clicks(&mut headless), IpcValue::Integer(1));
}
