//! A scripted pointer, for driving morf inside a nested, headless compositor.
//!
//! `virtual_pointer abs X Y W H | down | up | wait MS ...` moves to an
//! absolute point of a `W`x`H` output, presses or releases the left button,
//! or waits. Meant for `WLR_BACKENDS=headless cage -- script`: a compositor
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
            "down" | "up" => {
                let state = if word == "down" {
                    wl_pointer::ButtonState::Pressed
                } else {
                    wl_pointer::ButtonState::Released
                };
                pointer.button(now(), 0x110, state);
                pointer.frame();
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
