//! The Wayland backend: layer, xdg and lock surfaces, popups, fractional scale,
//! seats and frame callbacks. (The desktop protocols still living here move to
//! `morf-desktop` in phase 4.)

#[cfg(feature = "a11y")]
pub mod accesskit;
mod backend_impl;
mod buffer_sink;
mod capture_dmabuf;
mod capture_handlers;
mod client_backdrop;
mod client_connection;
mod client_data;
mod client_floating;
mod client_input;
mod client_layer;
mod client_lock;
mod client_services;
mod client_surface;
mod cursor;
mod data_handlers;
mod helpers;
mod inhibit_handlers;
mod input_handlers;
mod key_repeat;
mod protocol_handlers;
mod state_methods;
mod state_types;
mod surface_handlers;
mod surface_types;
mod toplevel_control;
mod toplevel_handlers;
mod types;

pub use buffer_sink::WaylandBufferSink;
pub use client_layer::*;
pub use client_services::Woke;
pub use cursor::cursor_shape;
pub use helpers::*;
pub use state_types::*;
pub use surface_types::*;
pub use types::*;
#[cfg(test)]
mod tests;
