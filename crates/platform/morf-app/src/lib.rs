//! morf's own windowing: what a window is, every kind a shell needs, one
//! event model, outputs, input and cursors, over two backends -- Wayland and
//! headless.

pub mod backend;

pub use backend::wayland::*;
