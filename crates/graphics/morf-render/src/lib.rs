//! GPU renderer for morf: the wgpu backend that presents the backend-
//! independent draw lists built by morf-paint.

mod gpu;

pub use gpu::dmabuf::{
    DmabufImage, DmabufPlane, DmabufSupport, FOURCC_ARGB8888, FOURCC_XRGB8888, MODIFIER_LINEAR,
    split_dev_t,
};
pub use gpu::hdr;
pub use gpu::{
    GpuError, GpuInfo, HdrRequest, OutputDepth, PresentStalls, ShaderRegistration, WgpuBackend,
    lose_devices, opened_device_count, present_stalls,
};

pub use morf_paint::*;

#[cfg(test)]
mod tests;
