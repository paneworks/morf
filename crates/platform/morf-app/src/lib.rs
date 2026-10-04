//! morf's own windowing: what a window is, every kind a shell needs, one
//! event model, outputs, input and cursors, over two backends -- Wayland and
//! headless.

pub mod backend;
mod data;
mod event;
mod input;
mod kind;
pub mod mime;
mod output;
pub mod placement;
mod positioner;
pub mod transfer;
mod window;

#[cfg(feature = "wayland")]
pub use backend::wayland::*;
pub use backend::{
    Backend, Capabilities, PRIMARY_LAYER, RenderTarget, WindowKind, Woke, physical_size,
};
pub use data::*;
pub use event::*;
pub use input::*;
pub use kind::*;
pub use output::*;
pub use positioner::*;
pub use window::*;
