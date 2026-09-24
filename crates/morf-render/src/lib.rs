//! Backend-independent draw lists, damage tracking, and GPU instance data.

mod gpu;

pub use gpu::dmabuf::{
    DmabufImage, DmabufPlane, DmabufSupport, FOURCC_ARGB8888, FOURCC_XRGB8888, MODIFIER_LINEAR,
    split_dev_t,
};
pub use gpu::{GpuError, GpuInfo, ShaderRegistration, WgpuBackend};

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
