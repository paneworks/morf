//! Backend-independent draw lists, damage tracking, and GPU instance data:
//! a laid-out scene painted into commands, with no GPU or window system.

pub mod backdrop;
mod commands;
pub mod damage;
pub mod effects;
pub mod field;
mod gradient;
mod lcd;
mod paint;
mod paint_fields;
mod paint_text_input;
pub mod path;
mod sdf;

pub use commands::*;
pub use damage::*;
pub use field::*;
pub use lcd::{
    LCD_TAPS, SubpixelText, blend_stripes, stripe_centres, stripe_coverage, subpixel_text_for,
};
pub use morf_text::{FontSubpixel, LcdFilter, SubpixelOrder, font_subpixel};
pub use paint::{CopiedRun, Copies, RunCursor, extend_runs};
pub use path::PathPaint;
pub use sdf::*;

/// The space translucent colours are mixed in when they are drawn over what
/// is already there.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum BlendSpace {
    /// Linear light: physically right, and what morf has always done. A
    /// faint colour over a dark ground comes out brighter than the same
    /// design in a browser.
    #[default]
    Linear,
    /// sRGB-encoded values, as browsers, Qt and GTK mix them: 50% white over
    /// black is `#808080`. For matching a design made in any of those.
    Srgb,
}

impl BlendSpace {
    /// Reads `"linear"` or `"srgb"`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "linear" => Some(Self::Linear),
            "srgb" => Some(Self::Srgb),
            _ => None,
        }
    }

    /// The name [`Self::parse`] reads.
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Srgb => "srgb",
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;
