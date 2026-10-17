//! A surface whose GPU device was lost moves to a fresh one in place, rather
//! than its worker failing and the output being brought up again.

use morf_lua::Runtime;
use morf_paint::RenderEngine;
use morf_render::WgpuBackend;

/// Rebuilds `renderer` on a fresh device if its own was lost, registers the
/// configuration's shaders with it again and has the next frame drawn in
/// full. Returns whether it did; `what` names the surface in the log.
pub fn recover_lost_device(
    runtime: &Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    what: &str,
) -> Result<bool, String> {
    if !renderer.backend_mut().device_lost() {
        return Ok(false);
    }
    let started = std::time::Instant::now();
    renderer
        .backend_mut()
        .recover()
        .map_err(|error| format!("{what}: could not recover from a lost GPU device: {error}"))?;
    renderer.forget();
    crate::host::run::register_shaders(runtime, renderer)?;
    eprintln!(
        "morf: gpu: device lost; {what} recovered on a fresh device in {:.1} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(true)
}
