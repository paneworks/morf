mod auxiliary;
mod cache;
mod frame_log;
mod layer;
mod layout_only;
mod renderer_settings;

use morf_app::{Backend, PRIMARY_LAYER};
use morf_lua::Runtime;
use morf_scene::NodeHandle;

pub use auxiliary::{
    AuxiliaryKind, paint_auxiliary_surface, paint_floating_surface, paint_popup_surface,
};
pub use cache::{CachedLayout, layout_for};
pub use frame_log::{frame_log_wanted, frame_split_wanted};
pub use layer::{MASK_SENTINEL, paint_layer, paint_layer_surface, send_input_region};
pub use layout_only::{lay_out_auxiliary, lay_out_layer, lay_out_layer_surface};
pub use renderer_settings::{apply_blend, apply_subpixel};

use crate::paint::painter::Painter;

pub fn paint(
    runtime: &mut Runtime,
    painter: &mut Painter,
    client: &dyn Backend,
    root: NodeHandle,
    cache: Option<&mut CachedLayout>,
    cover: Option<&crate::paint::cover::Shown>,
) -> Result<CachedLayout, String> {
    // `MORF_LOG=frame` prints how long each frame of the primary surface
    // took on the CPU side, submission included, so a configuration that
    // feels slow can be read rather than guessed at.
    let started = frame_log_wanted().then(std::time::Instant::now);
    let _stage = morf_lua::profile::stage(morf_lua::profile::Stage::Paint);
    let config = runtime.layer_surface_config();
    let painted = match painter {
        Painter::Gpu(renderer) => paint_layer(
            runtime,
            renderer,
            client,
            PRIMARY_LAYER,
            root,
            &config,
            cache,
            cover,
        ),
        Painter::Layout(text) => {
            lay_out_layer(runtime, text, client, PRIMARY_LAYER, root, &config, cache)
        }
    };
    if let Some(started) = started {
        eprintln!(
            "{} frame on {} took {:.2} ms",
            crate::paint::wake_plan::stamp(),
            std::thread::current().name().unwrap_or("?"),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    painted
}

pub fn clock_text() -> String {
    jiff::Zoned::now().strftime("%H:%M:%S").to_string()
}
