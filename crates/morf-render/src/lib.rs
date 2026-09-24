//! Backend-independent draw lists, damage tracking, and GPU instance data.

mod gpu;

pub use gpu::dmabuf::{
    DmabufImage, DmabufPlane, DmabufSupport, FOURCC_ARGB8888, FOURCC_XRGB8888, MODIFIER_LINEAR,
    split_dev_t,
};
pub use gpu::{GpuError, GpuInfo, ShaderRegistration, WgpuBackend};

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

mod commands;
mod damage;
mod effects;
mod field;
mod gradient;
mod paint;
mod paint_fields;
mod paint_text_input;
mod path;
mod sdf;

pub use commands::*;
pub use damage::*;
pub use field::*;
pub use path::PathPaint;
pub use sdf::*;
#[cfg(test)]
mod tests;
