//! Scene graph, typed properties, and animation targets for morf.

mod model;

pub use model::{ListChange, ListModel, ModelId, ViewItem, ViewTransition, VirtualList};

mod accessible;
mod animation;
mod channel;
mod coerce;
// Colour lives in morf-value, the bottom of the graph; the scene keeps it
// under its old paths.
pub(crate) use morf_value::color;
mod decoration;
mod direction;
mod error;
mod exit;
mod fling;
mod focus;
mod gradient;
mod groups;
mod hashing;
pub use morf_value::hct;
mod keyframes;
mod mask;
mod motion;
mod motion_values;
pub mod overlay;
mod path_style;
mod playback;
mod property_store;
/// The reactive signal graph the scene's properties live in.
pub mod reactive;
/// Retention: locks that keep something alive until they are let go.
pub mod retain;
mod rich_text;
mod scene;
mod scene_access;
mod scene_behavior;
mod scene_default;
mod scene_revision;
mod scene_shown;
mod scene_tick;
mod schema;
mod scroll_fling;
mod spline;
mod stretch;
mod terminal;
mod types;

pub use accessible::{AccessibleNode, AccessibleValue, Checked, ROLES as ACCESSIBLE_ROLES};
pub use animation::*;
pub use channel::{
    Channel, MAX_CHANNEL_LEN, channel, channel_by_id, channels_generation, drop_channel,
};
pub use coerce::{ANCHOR_KEYS, CURSOR_SHAPES};
pub use color::{ColorSpace, HueDirection, mix as mix_colors};
pub use decoration::*;
pub use direction::{default_rtl, locale_is_rtl, locale_rtl_from_env, set_default_rtl};
pub use exit::ExitSpec;
pub use focus::FocusPolicy;
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
