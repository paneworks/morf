//! Scene graph, typed properties, and animation targets for morf.

mod model;

pub use model::{ListChange, ListModel, ModelId, ViewItem, ViewTransition, VirtualList};

mod animation;
mod coerce;
mod color;
mod decoration;
mod error;
mod exit;
mod fling;
mod gradient;
mod groups;
mod hashing;
pub mod hct;
mod keyframes;
mod mask;
mod motion;
mod motion_values;
mod path_style;
mod playback;
mod rich_text;
mod scene;
mod scene_access;
mod scene_behavior;
mod scene_default;
mod scene_revision;
mod schema;
mod spline;
mod stretch;
mod terminal;
mod types;

pub use animation::*;
pub use coerce::{ANCHOR_KEYS, CURSOR_SHAPES};
pub use color::{ColorSpace, HueDirection, mix as mix_colors};
pub use decoration::*;
pub use exit::ExitSpec;
pub use gradient::*;
pub use groups::*;
pub use hashing::*;
pub use keyframes::*;
pub use mask::*;
pub use path_style::*;
pub use rich_text::{MAX_SPANS, RichSpan, RichText};
pub use schema::{ELEMENTS, PropertyInfo, element_schemas};
pub use spline::{MAX_SPLINE_SEGMENTS, MAX_SPLINES, intern_spline, spline_value};
pub use stretch::{STRETCH_MAX_GAP, Stretch, spring_step};
pub use terminal::*;
pub use types::*;
#[cfg(test)]
mod tests;
