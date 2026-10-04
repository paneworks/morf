//! The engine tests that read the runtime's own state (its reactive state,
//! the Lua context, crate-private modules): unit tests, where those are in
//! reach. Everything a test can reach through the public API is in
//! `tests/engine`.
#![allow(dead_code)]

use crate::*;

/// Polls until `done` holds, for at most a second.
///
/// Not "until a poll reports a repaint": other things than the one a test is
/// waiting for owe a repaint too — the settings portal answering, on a desktop
/// that has one, is the usual one.
fn poll_until(runtime: &mut Runtime, done: impl Fn(&Runtime) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !done(runtime) && std::time::Instant::now() < deadline {
        runtime.poll_services();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

mod core_api;
mod dbus_private;
mod events_animation;
mod fs_time;
mod fs_watch;
mod image_ops;
mod images;
mod modules;
mod timers;
mod wake;
mod window_events;
