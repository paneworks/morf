mod arguments;
mod backdrop;
mod colors;
use morf_scene::NodeHandle;

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

struct NoText;

impl morf_layout::TextMeasurer for NoText {
    fn measure(
        &mut self,
        _node: NodeHandle,
        _text: &str,
        _family: &str,
        _size: f64,
        _options: morf_layout::TextOptions,
    ) -> morf_layout::Size {
        morf_layout::Size::default()
    }
}

mod animation_groups;
mod animation_playback;
mod async_io;
mod audio;
mod clipboard_dnd;
mod config;
mod core_api;
mod dbus_private;
mod destroying;
mod diagnostics;
mod drawers;
mod entering;
mod events_animation;
mod examples;
mod flush_construction;
mod flushing;
mod fs_time;
mod fs_watch;
mod gradients;
mod harness;
mod http;
mod idle_input;
mod idle_motion;
mod image_ops;
mod images;
mod input_api;
mod layer_surfaces;
mod lib_dbus_services;
mod lib_hyprland;
mod lib_material;
mod lib_palette;
mod lib_sysinfo_web;
mod lifecycle_io;
mod modules;
mod pam_session;
mod paths;
mod prefers;
mod reactivity;
mod sandbox_limits;
mod scene;
mod screens;
mod services;
mod session_lock;
mod shaders;
mod state_tables;
mod terminal;
mod text_fuzzy;
mod text_input;
mod text_style;
mod themes;
mod timers;
mod toplevels;
mod views_states;
mod wake;
mod window_events;
