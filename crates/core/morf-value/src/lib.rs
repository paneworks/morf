//! The values that cross every boundary in morf: a colour, and the bounded
//! value and table that Lua, the widget archetypes, IPC and a signal's state
//! exchange.
//!
//! The bottom of the crate graph: it depends on nothing of morf's, and
//! nothing here knows Lua, a scene or a window.

pub mod color;
pub mod hct;
/// Input regions: the shapes a surface takes the pointer in, composed into
/// the rectangles a compositor is told.
pub mod region;
pub mod shader_abi;
mod value;

pub use color::{Color, ColorSpace, HueDirection, mix as mix_colors};
pub use value::{IpcTable, IpcValue};
