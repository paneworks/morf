//! morf's own windowing: what a window is, every kind a shell needs, one
//! event model, outputs, input and cursors, over two backends -- Wayland and
//! headless.

pub mod backend;
mod data;
mod event;
mod input;
mod kind;
mod output;
pub mod placement;
mod positioner;
mod window;

pub use backend::{Backend, Capabilities, RenderTarget, WindowKind};
#[cfg(feature = "wayland")]
pub use backend::wayland::*;
pub use data::*;
pub use event::*;
pub use input::*;
pub use kind::*;
pub use output::*;
pub use positioner::*;
pub use window::*;
