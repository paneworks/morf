//! A surface's configuration applied to its renderer: subpixel text,
//! opacity and blend space.

use morf_app::Backend;
use morf_paint::{BlendSpace, RenderEngine};
use morf_render::WgpuBackend;

/// Draws a renderer's text in subpixels where this output and the
/// configuration allow it (morf-render's lcd.rs has the rules), and tells it
/// whether the surface is declared opaque. A change draws the next frame in
/// full.
pub fn apply_subpixel(
    renderer: &mut RenderEngine<WgpuBackend>,
    setting: &str,
    client: &dyn Backend,
    opaque: bool,
) {
    let text = client.own_output().and_then(|output| {
        morf_paint::subpixel_text_for(
            setting,
            morf_paint::font_subpixel(),
            output.subpixel,
            output.transform,
        )
    });
    let backend = renderer.backend_mut();
    let changed = backend.set_subpixel_text(text) | backend.set_opaque_surface(opaque);
    if changed {
        renderer.forget();
    }
}

/// Puts a renderer in the blend space a surface's configuration names.
///
/// Returns whether it changed, which rebuilt the renderer's target and
/// pipelines: the next frame is drawn in full, and whoever registered shaders
/// with it registers them again.
pub fn apply_blend(renderer: &mut RenderEngine<WgpuBackend>, blend: &str) -> bool {
    let blend = BlendSpace::parse(blend).unwrap_or_default();
    if !renderer.backend_mut().set_blend(blend) {
        return false;
    }
    renderer.forget();
    true
}
