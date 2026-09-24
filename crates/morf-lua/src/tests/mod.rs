mod arguments;
mod backdrop;
mod colors;
use morf_scene::NodeHandle;

use crate::*;

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
mod audio;
mod clipboard_dnd;
mod config;
mod core_api;
mod diagnostics;
mod entering;
mod events_animation;
mod examples;
mod flushing;
mod fs_time;
mod gradients;
mod http;
mod idle_input;
mod image_ops;
mod input_api;
mod layer_surfaces;
mod lib_hyprland;
mod lifecycle_io;
mod modules;
mod pam_session;
mod prefers;
mod sandbox_limits;
mod scene;
mod screens;
mod services;
mod shaders;
mod state_tables;
mod text_input;
mod text_style;
mod themes;
mod views_states;
