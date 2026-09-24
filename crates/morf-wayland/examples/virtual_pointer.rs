//! A scripted pointer, for driving morf inside a nested, headless compositor.
//!
//! Steps, run in order:
//!
//! - `abs X Y W H`: move to an absolute point of a `W`x`H` output
//! - `down` / `up`: press or release the left button; `rdown`/`rup` the
//!   right one, `mdown`/`mup` the middle one
//! - `click` / `rclick` / `mclick`: press and release that button
//! - `wheel DX DY`: that many wheel notches (positive `DY` scrolls down)
//! - `drag X1 Y1 X2 Y2 W H STEPS`: press at the first point, move in
//!   `STEPS` steps (a few milliseconds apart) to the second, release
//! - `hover X1 Y1 X2 Y2 W H STEPS`: the same moves with no button held
//! - `wait MS`: wait Meant for `WLR_BACKENDS=headless cage -- script`: a compositor
//! with no input devices at all, where this is the only pointer there is.
//! Never point it at a desktop someone is using.

use std::time::{Duration, Instant};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_pointer, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerManagerV1,
        _: <ZwlrVirtualPointerManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerV1,
        _: <ZwlrVirtualPointerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let connection = Connection::connect_to_env().expect("a Wayland display");
    let (globals, mut queue) = registry_queue_init::<State>(&connection).expect("globals");
    let qh = queue.handle();
    let manager: ZwlrVirtualPointerManagerV1 = globals
        .bind(&qh, 1..=2, ())
        .expect("zwlr_virtual_pointer_manager_v1");
    let pointer = manager.create_virtual_pointer(None, &qh, ());
    let started = Instant::now();
    let now = || started.elapsed().as_millis() as u32;
    let mut state = State;
    let mut words = args.iter().map(String::as_str);
    let number = |word: Option<&str>| -> u32 {
        word.and_then(|word| word.parse().ok())
            .expect("a whole number")
    };
    let press = |button: u32, down: bool, time: u32| {
        let state = if down {
            wl_pointer::ButtonState::Pressed
        } else {
            wl_pointer::ButtonState::Released
        };
        pointer.button(time, button, state);
        pointer.frame();
    };
    while let Some(word) = words.next() {
        match word {
            "abs" => {
                let (x, y, width, height) = (
                    number(words.next()),
                    number(words.next()),
                    number(words.next()),
                    number(words.next()),
                );
                pointer.motion_absolute(now(), x, y, width, height);
                pointer.frame();
            }
            "down" | "up" => press(BTN_LEFT, word == "down", now()),
            "rdown" | "rup" => press(BTN_RIGHT, word == "rdown", now()),
            "mdown" | "mup" => press(BTN_MIDDLE, word == "mdown", now()),
            "click" | "rclick" | "mclick" => {
                let button = match word {
                    "click" => BTN_LEFT,
                    "rclick" => BTN_RIGHT,
                    _ => BTN_MIDDLE,
                };
                press(button, true, now());
                queue.roundtrip(&mut state).expect("roundtrip");
                std::thread::sleep(Duration::from_millis(30));
                press(button, false, now());
            }
            "wheel" => {
                let signed = |word: Option<&str>| -> i32 {
                    word.and_then(|word| word.parse().ok())
                        .expect("a whole number")
                };
                let (dx, dy) = (signed(words.next()), signed(words.next()));
                pointer.axis_source(wl_pointer::AxisSource::Wheel);
                for (axis, notches) in [
                    (wl_pointer::Axis::HorizontalScroll, dx),
                    (wl_pointer::Axis::VerticalScroll, dy),
                ] {
                    if notches != 0 {
                        pointer.axis_discrete(now(), axis, f64::from(notches) * 15.0, notches);
                    }
                }
                pointer.frame();
            }
            "drag" | "hover" => {
                let (x1, y1, x2, y2, width, height, steps) = (
                    number(words.next()),
                    number(words.next()),
                    number(words.next()),
                    number(words.next()),
                    number(words.next()),
                    number(words.next()),
                    number(words.next()).max(1),
                );
                pointer.motion_absolute(now(), x1, y1, width, height);
                pointer.frame();
                queue.roundtrip(&mut state).expect("roundtrip");
                std::thread::sleep(Duration::from_millis(40));
                if word == "drag" {
                    press(BTN_LEFT, true, now());
                    queue.roundtrip(&mut state).expect("roundtrip");
                    std::thread::sleep(Duration::from_millis(40));
                }
                for step in 1..=steps {
                    let along = |from: u32, to: u32| {
                        let from = f64::from(from);
                        let to = f64::from(to);
                        (from + (to - from) * f64::from(step) / f64::from(steps)).round() as u32
                    };
                    pointer.motion_absolute(now(), along(x1, x2), along(y1, y2), width, height);
                    pointer.frame();
                    queue.roundtrip(&mut state).expect("roundtrip");
                    std::thread::sleep(Duration::from_millis(16));
                }
                if word == "drag" {
                    std::thread::sleep(Duration::from_millis(40));
                    press(BTN_LEFT, false, now());
                }
            }
            "wait" => {
                queue.roundtrip(&mut state).expect("roundtrip");
                std::thread::sleep(Duration::from_millis(u64::from(number(words.next()))));
            }
            other => panic!("unknown step `{other}`"),
        }
        queue.roundtrip(&mut state).expect("roundtrip");
    }
    pointer.destroy();
    queue.roundtrip(&mut state).expect("roundtrip");
}
