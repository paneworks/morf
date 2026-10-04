//! Painting a layer: the shell's own surface, or one configured layer surface
//! in its own renderer.

use morf_app::Backend;
use morf_app::{InputRect, WindowId};
use morf_lua::{LayerSurfaceConfig, Runtime};
use morf_render::{RenderEngine, WgpuBackend};
use morf_scene::NodeHandle;
use morf_value::region::{Rect as RegionRect, Region};

use crate::{surface_layers::*, surfaces::*};

use super::{
    CachedLayout, FrameSplit, MASK_SENTINEL, apply_blend, apply_subpixel, frame_log_wanted,
    layout_for,
};

pub fn paint_layer(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &dyn Backend,
    layer: u64,
    root: NodeHandle,
    config: &LayerSurfaceConfig,
    mut cache: Option<&mut CachedLayout>,
) -> Result<CachedLayout, String> {
    let (width, height) = client
        .layer_logical_size(layer)
        .ok_or_else(|| "layer surface disappeared while painting".to_owned())?;
    // Read every paint like the keyboard focus below, so a configuration may
    // change it at any time; the renderer rebuilds only when it moved, and
    // its pipelines — the configuration's shaders among them — with it.
    if apply_blend(renderer, &config.blend) {
        crate::surface_run::register_shaders(runtime, renderer)?;
    }
    let scale_120 = client.layer_scale_120(layer).unwrap_or(120);
    apply_subpixel(renderer, &config.subpixel_text, client, config.opaque);
    // `morf.surface.keyboard_focus` is read every paint, so a configuration
    // may take the keyboard for a page and hand it back after, without a
    // second surface. Sent only when it differs from the last paint's.
    if cache
        .as_deref()
        .is_none_or(|cached| cached.keyboard_focus != config.keyboard_focus)
        && let Some(focus) = keyboard_focus_of(&config.keyboard_focus)
    {
        client.set_layer_keyboard_focus(layer, focus);
    }
    let mut split = FrameSplit::start();
    // This surface's tree's revision, not the scene's: a clock ticking on
    // another surface leaves this layout as it was.
    let revision = runtime.scene().layout_revision_of(root);
    let (layout, fresh) = layout_for(
        runtime,
        cache.as_deref_mut(),
        root,
        (revision, (width, height), scale_120),
        renderer.backend_mut(),
    )?;
    if fresh {
        // Only on a fresh layout: it is the one moment the answer can have
        // changed, and the cached one has already been looked at.
        split.mark("layout");
        runtime.lint_layout(&layout, root);
        split.mark("lint");
    }
    // Every frame, not only a fresh layout's: a caret that moved without the
    // text changing still has to be scrolled into view.
    runtime.sync_text_inputs(&layout, renderer.backend_mut().text_system());
    runtime.observe_stretch(&layout);
    split.mark("text inputs");
    let scene = runtime.scene();
    let input = send_input_region(&scene, &layout, client, layer, config, cache.as_deref())?;
    split.mark("input region");
    let mut backdrop = Vec::new();
    // Where the compositor should blur what is behind this surface. Nothing is
    // read back: the blur happens on the far side of this call, underneath a
    // surface that is about to be composited over it, and the only thing that
    // makes it visible is the alpha this configuration painted with.
    if client.supports_backdrop_blur() {
        let shapes: Vec<Region> = layout
            .backdrop_geometry(&scene)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(geometry, radii)| Region {
                // Whole pixels, not grid cells. Quantising the *position*
                // here was worth nothing — a moving shape never compares equal
                // to its cached self whatever the grid, and a still one
                // compares equal without any — and it cost up to half a cell of
                // registration against the shape drawn over it, in a direction
                // that changed every frame.
                rect: RegionRect {
                    x: geometry.x.floor() as i32,
                    y: geometry.y.floor() as i32,
                    width: (geometry.width.ceil() as i32).max(0),
                    height: (geometry.height.ceil() as i32).max(0),
                },
                shape: morf_value::region::Shape::Box,
                params: morf_value::region::ShapeParams {
                    radii,
                    ..morf_value::region::ShapeParams::default()
                },
                ..Region::default()
            })
            .collect();
        // What is *sent* is the previous frame's shapes, not this frame's.
        //
        // We do not own the commit. Mesa's Vulkan display queue attaches and
        // commits the buffer on its own thread, at its own pace, and repeats a
        // buffer when it has nothing newer — so a region set here lands on
        // whichever commit happens next, which may carry a buffer older than
        // the geometry it was derived from. There is no pairing to rely on.
        //
        // Which direction that error falls in is not symmetric. A blur that
        // trails the shape by a frame is what a blur does; a blur that arrives
        // before the thing casting it is wrong in a way that reads instantly as
        // the effect predicting the motion. So the region is deliberately one
        // frame behind: it can only ever lag, and lag is the physical answer.
        let previous = cache
            .map(|cached| std::mem::take(&mut cached.backdrop))
            .unwrap_or_default();
        if previous != shapes && !previous.is_empty() {
            let rectangles = morf_value::region::build_scaled(
                width,
                height,
                &previous,
                morf_value::region::COVERED_EDGE_GRID,
            )
            .map_err(|error| error.to_string())?;
            client
                .set_layer_backdrop_region(layer, Some(&rectangles))
                .map_err(|error| error.to_string())?;
        }
        backdrop = shapes;
    }

    split.mark("backdrop region");
    client.request_frame(WindowId::Layer(layer));
    if !client.has_window(WindowId::Layer(layer)) {
        return Err("layer surface disappeared while painting".to_owned());
    }
    // A backend presenting through its own buffers declares the damage with
    // the buffer itself (`WgpuBackend::declares_damage`).
    let declare = !renderer.backend_mut().declares_damage();
    let damage = renderer
        .render(&scene, &layout, scale_120, |damage| {
            if !declare {
                return;
            }
            // What actually changed, rather than the whole surface. A
            // compositor recomposites the area a client declares, so a
            // fullscreen overlay that declares everything costs a full screen
            // of blending every frame however little of it moved.
            for rect in damage {
                client.damage(
                    WindowId::Layer(layer),
                    rect.x as i32,
                    rect.y as i32,
                    rect.width as i32,
                    rect.height as i32,
                );
            }
        })
        .map_err(|error| error.to_string())?;
    split.mark("render");
    // What the frame repainted, beside how long it took: the one number that
    // says whether a change cost its own area or the whole surface.
    if frame_log_wanted() && !damage.is_empty() {
        let area: u64 = damage
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum();
        // With `MORF_FRAME_LOG=2`, where: the node that keeps a shell drawing
        // is found from the rectangle it repaints.
        let rects = if split.on {
            let listed = damage
                .iter()
                .take(4)
                .map(|rect| format!("{}x{}+{}+{}", rect.width, rect.height, rect.x, rect.y))
                .collect::<Vec<_>>()
                .join(" ");
            format!(": {listed}")
        } else {
            String::new()
        };
        eprintln!(
            "{} layer {layer} damaged {area} px in {} rect(s){rects}",
            crate::wake_plan::stamp(),
            damage.len()
        );
    }
    if damage.is_empty() {
        client.commit(WindowId::Layer(layer));
    }
    // After the frame is on its way, and only when nothing moves: text laid
    // out but hidden -- a preloaded panel -- gets its glyphs made now, so
    // the frame that shows it does not spend its time on them.
    if fresh && !runtime.has_motion() {
        renderer
            .backend_mut()
            .warm_hidden_text(&scene, &layout, root, scale_120);
        split.mark("warm hidden text");
    }
    drop(scene);
    // After the render: what the images became is known once they were drawn.
    runtime.sync_images(&layout, renderer.backend_mut().image_cache());
    runtime.observe_layout_with(&layout, fresh);
    split.mark("observe layout");
    split.finish();
    Ok(CachedLayout {
        layout,
        revision,
        size: (width, height),
        scale_120,
        input,
        backdrop,
        keyboard_focus: config.keyboard_focus.clone(),
    })
}

/// Paints one configured layer surface into its own renderer.
///
/// `text`: the host's text system when it draws nothing; the surface is laid
/// out with it instead.
pub fn paint_layer_surface(
    runtime: &mut Runtime,
    client: &dyn Backend,
    surface: &mut Window,
    text: Option<&mut morf_text::TextSystem>,
) -> Result<(), String> {
    if let Some(text) = text {
        return super::lay_out_layer_surface(runtime, text, client, surface);
    }
    // Cleared here rather than at one of the two call sites, because there are
    // two: the frame callback honoured the flag and the main repaint block did
    // not, so an animating configured layer surface was painted twice for every
    // tick — once by each — and the flag it was supposed to be gated on was
    // never cleared by the one that ignored it.
    let Some(renderer) = &mut surface.renderer else {
        return Ok(());
    };
    // Still waiting for the last frame's callback: presenting again would
    // block a FIFO swapchain until it comes, and on a surface the compositor
    // is not showing it never does. Kept owed; the callback paints it. A
    // surface that has never painted is exempt, since it is not mapped until
    // it does and an unmapped surface's callback waits for that.
    if surface.layout.is_some()
        && client
            .layer_frame_wait(window_layer_id(surface.id))
            .is_some()
    {
        surface.needs_paint = true;
        return Ok(());
    }
    surface.needs_paint = false;
    let config = surface
        .layer_config
        .clone()
        .ok_or_else(|| "layer surface lost its configuration".to_owned())?;
    let painted = paint_layer(
        runtime,
        renderer,
        client,
        window_layer_id(surface.id),
        surface.root,
        &config,
        surface.layout.as_mut(),
    )?;
    // A binding on this tree's layout geometry (`layout_width`, ...) hears
    // the frame as it is observed, after the render, and may move the tree
    // again: centred on its own measured width, a capture toolbar was drawn
    // where the first frame put it, off centre, until something else
    // repainted a surface that never does by itself. One more paint is owed;
    // the frame callback `paint_layer` asked for makes it.
    surface.needs_paint = runtime.scene().layout_revision_of(surface.root) != painted.revision;
    surface.layout = Some(painted);
    Ok(())
}

/// Hands the compositor where the layer takes the pointer: the configured
/// mask, or the live geometry of its interactive items. Sent only when it
/// differs from what `cache` says was sent last.
pub fn send_input_region(
    scene: &morf_scene::Scene,
    layout: &morf_layout::Layout,
    client: &dyn Backend,
    layer: u64,
    config: &LayerSurfaceConfig,
    cache: Option<&CachedLayout>,
) -> Result<Vec<InputRect>, String> {
    Ok(if let Some(regions) = &config.input_regions {
        // A configured mask is a static surface setting — nothing animates it —
        // so rasterising it and re-sending it every paint asks the compositor
        // to rebuild an identical region sixty times a second. The branch below
        // has always deduped; this one opted out of the cache by returning an
        // empty vector, which also made every frame look like a change.
        //
        // The sentinel is what the cache compares: an empty vector would match
        // a surface that genuinely has no interactive area, so a shape that
        // stands for "the configured mask, unchanged" is stored instead.
        let input = vec![MASK_SENTINEL];
        if cache.is_none_or(|cached| cached.input != input) {
            client
                .set_layer_composed_input_region(layer, regions)
                .map_err(|error| error.to_string())?;
        }
        input
    } else {
        let input = layout
            .input_geometry(scene)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|geometry| {
                let left = geometry.x.floor() as i32;
                let top = geometry.y.floor() as i32;
                let right = (geometry.x + geometry.width).ceil() as i32;
                let bottom = (geometry.y + geometry.height).ceil() as i32;
                InputRect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                }
            })
            .collect::<Vec<_>>();
        if cache.is_none_or(|cached| cached.input != input) {
            client.set_input_region(WindowId::Layer(layer), Some(&input));
        }
        input
    })
}
