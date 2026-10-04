//! morf's own windowing: what a window is, every kind a shell needs, one
//! event model, outputs, input and cursors, over two backends -- Wayland and
//! headless.

pub mod backend;
mod data;
mod event;
mod input;
mod kind;
mod output;
mod positioner;
mod window;

pub use backend::wayland::*;
pub use data::*;
pub use event::*;
pub use input::*;
pub use kind::*;
pub use output::*;
pub use positioner::*;
pub use window::*;
