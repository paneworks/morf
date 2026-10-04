//! A renderer for whatever a backend hands out to draw a window into.

use morf_app::RenderTarget;
use morf_render::{GpuError, WgpuBackend};

/// A GPU backend drawing into `target` at `width` x `height` pixels.
pub fn surface_backend(
    target: RenderTarget,
    width: u32,
    height: u32,
) -> Result<WgpuBackend, GpuError> {
    match target {
        RenderTarget::Wayland(window) => {
            let sink = window.buffer_sink();
            pollster::block_on(WgpuBackend::new_surface(window, sink, width, height))
        }
        RenderTarget::Offscreen { .. } => pollster::block_on(WgpuBackend::new(width, height)),
    }
}

/// The primary layer surface's target.
pub fn primary_target(client: &morf_app::LayerClient) -> Result<RenderTarget, String> {
    use morf_app::Backend as _;
    client
        .render_target(morf_app::WindowId::Layer(morf_app::PRIMARY_LAYER))
        .ok_or_else(|| "the primary surface is gone".to_owned())
}
