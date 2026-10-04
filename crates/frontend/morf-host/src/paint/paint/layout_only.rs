//! A paint with nothing to draw with: everything a frame does to the scene
//! and the window system -- the layout, its lint and observers, the text
//! inputs, the input region, the keyboard focus, the next frame asked for --
//! and no pixels. What a host without a GPU does each frame.

use morf_app::{Backend, WindowId};
use morf_lua::{LayerSurfaceConfig, Runtime};
use morf_scene::NodeHandle;
use morf_text::TextSystem;

use crate::surface_layers::window_layer_id;
use crate::surfaces::{Window, keyboard_focus_of};

use super::layer::send_input_region;
use super::{AuxiliaryKind, CachedLayout, layout_for};

/// Lays out one layer as [`super::layer::paint_layer`] paints it.
pub fn lay_out_layer(
    runtime: &mut Runtime,
    text: &mut TextSystem,
    client: &dyn Backend,
    layer: u64,
    root: NodeHandle,
    config: &LayerSurfaceConfig,
    mut cache: Option<&mut CachedLayout>,
) -> Result<CachedLayout, String> {
    let (width, height) = client
        .layer_logical_size(layer)
        .ok_or_else(|| "layer surface disappeared while painting".to_owned())?;
    let scale_120 = client.layer_scale_120(layer).unwrap_or(120);
    if cache
        .as_deref()
        .is_none_or(|cached| cached.keyboard_focus != config.keyboard_focus)
        && let Some(focus) = keyboard_focus_of(&config.keyboard_focus)
    {
        client.set_layer_keyboard_focus(layer, focus);
    }
    let revision = runtime.scene().layout_revision_of(root);
    let (layout, fresh) = layout_for(
        runtime,
        cache.as_deref_mut(),
        root,
        (revision, (width, height), scale_120),
        text,
    )?;
    // No lint here: a runner that only lays out lints what it settles on
    // (the headless runner, once its turns are quiet), not each pass on the
    // way there.
    runtime.sync_text_inputs(&layout, text);
    runtime.observe_stretch(&layout);
    let input = {
        let scene = runtime.scene();
        send_input_region(&scene, &layout, client, layer, config, cache.as_deref())?
    };
    // The clock the animations run on is the frame callbacks, drawn or not.
    client.request_frame(WindowId::Layer(layer));
    runtime.observe_layout_with(&layout, fresh);
    Ok(CachedLayout {
        layout,
        revision,
        size: (width, height),
        scale_120,
        input,
        backdrop: Vec::new(),
        keyboard_focus: config.keyboard_focus.clone(),
    })
}

/// Lays out one configured layer surface, as `paint_layer_surface` paints it.
pub fn lay_out_layer_surface(
    runtime: &mut Runtime,
    text: &mut TextSystem,
    client: &dyn Backend,
    surface: &mut Window,
) -> Result<(), String> {
    surface.needs_paint = false;
    let config = surface
        .layer_config
        .clone()
        .ok_or_else(|| "layer surface lost its configuration".to_owned())?;
    let laid = lay_out_layer(
        runtime,
        text,
        client,
        window_layer_id(surface.id),
        surface.root,
        &config,
        surface.layout.as_mut(),
    )?;
    surface.needs_paint = runtime.scene().layout_revision_of(surface.root) != laid.revision;
    surface.layout = Some(laid);
    Ok(())
}

/// Lays out one popup or toplevel, as `paint_auxiliary_surface` paints it.
pub fn lay_out_auxiliary(
    kind: AuxiliaryKind,
    runtime: &mut Runtime,
    text: &mut TextSystem,
    client: &dyn Backend,
    surface: &mut Window,
) -> Result<(), String> {
    let revision = runtime.scene().layout_revision_of(surface.root);
    let size = (surface.width, surface.height);
    let scale_120 = client.surface_scale_120(kind.role(surface.id));
    let (layout, fresh) = layout_for(
        runtime,
        surface.layout.as_mut(),
        surface.root,
        (revision, size, scale_120),
        text,
    )?;
    runtime.sync_text_inputs(&layout, text);
    runtime.observe_stretch(&layout);
    if fresh {
        kind.request_frame(client, surface.id);
    }
    runtime.observe_layout_with(&layout, fresh);
    if runtime.scene().layout_revision_of(surface.root) != revision {
        kind.request_frame(client, surface.id);
    }
    surface.layout = Some(CachedLayout {
        layout,
        revision,
        size,
        scale_120,
        input: Vec::new(),
        backdrop: Vec::new(),
        keyboard_focus: String::new(),
    });
    Ok(())
}
