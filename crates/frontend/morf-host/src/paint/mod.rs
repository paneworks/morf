//! Drawing a frame, pacing frames, and when the loop sleeps.

pub mod backdrop;
pub mod outputless;
pub mod pacing;
pub mod paint;
pub mod painter;
pub mod render_target;
pub mod wake_plan;

// What the module of the same name held, where it has always been found.
pub use paint::*;
