//! The layer surfaces: the shell's own, the ones a configuration asks for,
//! and the engine's edge reservers.

use morf_app::Backend;
use morf_app::{
    KeyboardFocus, LayerAnchors, LayerConfig, PRIMARY_LAYER, ShellLayer, physical_size,
};
use morf_lua::{LayerSurfaceConfig, Runtime, WindowSurfaceConfig, WindowSurfaceKind};
use morf_paint::RenderEngine;
use std::collections::HashSet;

use crate::host::windows::{Kind, Windows};
use crate::paint::render_target::surface_backend;
use crate::{host::surfaces::*, paint::*};
use morf_app::WindowId;
use morf_app::WindowKind;

/// Re-states the primary surface's opacity claim.
///
/// Called wherever its size or its configuration can have changed, because the
/// claim is a region of a particular size and a stale one is worse than none:
/// a bar that grew keeps a smaller opaque region, and the compositor is right
/// to blend the difference.
pub fn apply_primary_opaque(runtime: &Runtime, client: &dyn Backend) {
    client.set_layer_opaque(PRIMARY_LAYER, runtime.layer_surface_config().opaque);
}

/// First identifier of the four internal edge reservers.
///
/// Reservers are engine-owned rather than configured, so they take identifiers
/// from the top of the range where no Lua window surface can reach them.
pub const RESERVE_LAYER_BASE: u64 = u64::MAX - 3;

/// Wayland identifier of the layer surface backing one Lua window surface.
///
/// Window surface identifiers start at zero, which the shell's own surface
/// already owns, so every configured layer surface sits one above its Lua id.
pub fn window_layer_id(id: u64) -> u64 {
    id.wrapping_add(1)
}

/// Inverse of [`window_layer_id`], or `None` for engine-owned surfaces.
pub fn window_surface_id(layer: u64) -> Option<u64> {
    (layer != PRIMARY_LAYER && layer < crate::paint::backdrop::BACKDROP_LAYER).then(|| layer - 1)
}

/// Builds the configuration for one single-edge reserver surface.
///
/// The surface draws nothing and accepts no input. Its whole purpose is the
/// exclusive zone: a layer surface anchored to exactly one edge leaves the
/// compositor no doubt about which edge to shrink, which is what keeps tiled
/// windows out from under a frame drawn on all four edges at once.
pub fn reserve_bar_config(edge: &str, thickness: u32, output: &str) -> LayerConfig {
    LayerConfig {
        namespace: format!("morf-reserve-{edge}"),
        width: 1,
        height: 1,
        exclusive_zone: i32::try_from(thickness).unwrap_or(i32::MAX),
        output: Some(output.to_owned()),
        anchors: LayerAnchors {
            top: edge == "top",
            right: edge == "right",
            bottom: edge == "bottom",
            left: edge == "left",
        },
        margin_top: 0,
        margin_right: 0,
        margin_bottom: 0,
        margin_left: 0,
        layer: ShellLayer::Bottom,
        keyboard_focus: KeyboardFocus::None,
    }
}

/// Brings the reserver surfaces up to date with `morf.surface.reserve`.
///
/// A reserver holds an edge of the output away from the windows, and the only
/// thing that changes as the border animates is its exclusive zone — one of the
/// fields wlr-layer-shell lets a mapped surface change. Configured layer
/// surfaces already sort a live geometry change from a genuine rebuild for
/// exactly this reason; reservers ignored all of it and recreated the zwlr
/// surface, the wl_surface, the fractional scale and the viewport every time
/// the number moved, which is an unmap and a remap the compositor has to
/// rearrange around.
pub fn open_reserve_layers(
    client: &mut dyn Backend,
    config: &LayerSurfaceConfig,
    output: &str,
) -> Result<(), String> {
    if !client.supports_layer_shell() {
        return Ok(());
    }
    for (index, (edge, thickness)) in config.reserve.edges().into_iter().enumerate() {
        let id = RESERVE_LAYER_BASE + index as u64;
        if thickness == 0 {
            client.close(WindowId::Layer(id));
            continue;
        }
        let reserve = reserve_bar_config(edge, thickness, output);
        if client.has_window(WindowId::Layer(id)) {
            // Already mapped: move it rather than rebuild it.
            client
                .set_layer_geometry(id, &reserve)
                .map_err(|error| error.to_string())?;
            client.commit(WindowId::Layer(id));
            continue;
        }
        client
            .open(WindowId::Layer(id), WindowKind::Layer(reserve))
            .map_err(|error| error.to_string())?;
        client.set_input_region(WindowId::Layer(id), Some(&[]));
        // A reserver draws nothing, but it still has to map: a compositor
        // computes the output's usable area from the layer surfaces it
        // arranges, and an unmapped one is skipped, so its exclusive zone would
        // reserve nothing at all.
        client
            .map_layer_blank(id)
            .map_err(|error| error.to_string())?;
        client.commit(WindowId::Layer(id));
    }
    Ok(())
}

/// How a configured layer surface reaches its new configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerUpdate {
    /// The surface already matches; the compositor hears nothing.
    None,
    /// Geometry moved, and every part of it may change on a live surface.
    Geometry,
    /// Something fixed at creation moved, so the surface has to be rebuilt.
    Recreate,
}

/// Decides how one layer surface is brought up to date.
///
/// wlr-layer-shell lets a mapped surface change its size, anchors, margins,
/// exclusive zone and keyboard interactivity, and from version 2 its layer
/// (`live_layer`); namespace and output are fixed when the surface is
/// created, and so is the layer on an older compositor. Sorting the two
/// apart is what keeps an animated margin or a growing border from destroying
/// the zwlr surface, the wl_surface, the fractional scale, the viewport and the
/// renderer once per frame — a visible unmap and remap for a geometry change
/// the protocol supports outright.
pub fn layer_update(
    current: Option<&LayerSurfaceConfig>,
    next: &LayerSurfaceConfig,
    live_layer: bool,
) -> LayerUpdate {
    let Some(current) = current else {
        return LayerUpdate::Recreate;
    };
    if current.namespace != next.namespace || (!live_layer && current.layer != next.layer) {
        return LayerUpdate::Recreate;
    }
    if current == next {
        LayerUpdate::None
    } else {
        LayerUpdate::Geometry
    }
}

/// Opens, updates, and closes the layer surfaces a configuration asks for.
pub fn sync_layer_surfaces(
    client: &mut dyn Backend,
    output: &str,
    desired: &[&WindowSurfaceConfig],
    windows: &mut Windows,
) -> Result<bool, String> {
    let mut resumed = false;
    let live = desired
        .iter()
        .map(|surface| surface.id)
        .collect::<HashSet<_>>();
    let mut stale = windows
        .ids(Kind::Layer)
        .into_iter()
        .filter(|id| !live.contains(id))
        .collect::<Vec<_>>();
    stale.sort_unstable_by(|a, b| b.cmp(a));
    for id in stale {
        client.close(WindowId::Layer(window_layer_id(id)));
        windows.remove(Kind::Layer, id);
    }
    for surface in desired {
        let id = surface.id;
        let WindowSurfaceKind::Layer(config) = &surface.kind else {
            unreachable!("only layer surfaces reach the layer sync");
        };
        let update = layer_update(
            windows
                .get(Kind::Layer, id)
                .and_then(|current| current.layer_config.as_ref()),
            config,
            client.supports_live_layer_change(),
        );
        if update == LayerUpdate::Recreate {
            client
                .open(
                    WindowId::Layer(window_layer_id(id)),
                    WindowKind::Layer(runtime_bar_config(config, output)?),
                )
                .map_err(|error| error.to_string())?;
            windows.insert(
                Kind::Layer,
                id,
                Window {
                    id,
                    root: surface.root,
                    updates_enabled: surface.updates_enabled,
                    width: config.width.max(1),
                    height: config.height.max(1),
                    renderer: None,
                    layout: None,
                    popup_config: None,
                    floating_config: None,
                    layer_config: Some(config.clone()),
                    needs_paint: true,
                    owns_root: false,
                    built_for: None,
                },
            );
            continue;
        }
        if update == LayerUpdate::Geometry {
            client
                .set_layer_geometry(window_layer_id(id), &runtime_bar_config(config, output)?)
                .map_err(|error| error.to_string())?;
        }
        let Some(current) = windows.get_mut(Kind::Layer, id) else {
            continue;
        };
        if update == LayerUpdate::Geometry {
            // The mask travels in the same configuration, and it is applied
            // when the surface paints, so a geometry change owes one repaint
            // even before the compositor answers with a configure.
            current.layer_config = Some(config.clone());
            current.needs_paint = true;
            resumed = true;
        }
        resumed |= !current.updates_enabled && surface.updates_enabled;
        let moved = current.root != surface.root;
        current.root = surface.root;
        current.updates_enabled = surface.updates_enabled;
        // Only when the tree it lays out actually changed. `CachedLayout`
        // already re-checks the revision, the size and the scale, so
        // clearing it here on every sync threw away a valid layout — and
        // with an anchored popup, which re-syncs whenever its anchor moves,
        // that was every frame.
        if moved {
            current.layout = None;
        }
    }
    Ok(resumed)
}

/// Applies one compositor configure to a configured layer surface.
pub fn layer_surface_configure(
    runtime: &mut Runtime,
    client: &dyn Backend,
    state: &mut SurfaceEventState,
    layer: u64,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let Some(id) = window_surface_id(layer) else {
        return Ok(());
    };
    let Some(surface) = state.windows.get_mut(Kind::Layer, id) else {
        return Ok(());
    };
    let initial = surface.renderer.is_none() && surface.layout.is_none();
    surface.width = width.max(1);
    surface.height = height.max(1);
    let scale = client.layer_scale_120(layer).unwrap_or(120);
    let (physical_width, physical_height) = client
        .buffer_size(WindowId::Layer(layer))
        .unwrap_or_else(|| physical_size((surface.width, surface.height), scale));
    if let Some(renderer) = &mut surface.renderer {
        renderer.resize(physical_width, physical_height);
    } else if state.painter.gpu().is_some() {
        let target = client
            .render_target(WindowId::Layer(layer))
            .ok_or_else(|| "configured layer surface disappeared".to_owned())?;
        let backend = surface_backend(target, physical_width, physical_height)
            .map_err(|error| error.to_string())?;
        surface.renderer = Some(RenderEngine::new(backend));
    }
    if initial || surface.updates_enabled {
        paint_layer_surface(runtime, client, surface, state.painter.layout_only())?;
    }
    Ok(())
}

/// Resizes one configured layer surface after its preferred scale changed.
pub fn layer_surface_scale(
    runtime: &mut Runtime,
    client: &dyn Backend,
    state: &mut SurfaceEventState,
    layer: u64,
) -> Result<(), String> {
    let Some(id) = window_surface_id(layer) else {
        return Ok(());
    };
    let Some(surface) = state.windows.get_mut(Kind::Layer, id) else {
        return Ok(());
    };
    let scale = client.layer_scale_120(layer).unwrap_or(120);
    if let Some(renderer) = &mut surface.renderer {
        let (width, height) = client
            .buffer_size(WindowId::Layer(layer))
            .unwrap_or_else(|| physical_size((surface.width, surface.height), scale));
        renderer.resize(width, height);
    }
    // And then draw into it. Resizing the swapchain without repainting leaves
    // the surface showing whatever the old buffer held, at the new size, until
    // something unrelated happens to mark it dirty — which on a static bar can
    // be a very long time. Its two siblings, the primary-layer scale change and
    // the configure for this same surface, both repaint here.
    if surface.updates_enabled {
        paint_layer_surface(runtime, client, surface, state.painter.layout_only())?;
    }
    Ok(())
}

/// Paints one configured layer surface when the compositor permits a frame.
pub fn layer_surface_frame(
    runtime: &mut Runtime,
    client: &dyn Backend,
    state: &mut SurfaceEventState,
    layer: u64,
) -> Result<(), String> {
    let Some(id) = window_surface_id(layer) else {
        return Ok(());
    };
    let Some(surface) = state
        .windows
        .get_mut(Kind::Layer, id)
        .filter(|surface| surface.updates_enabled && surface.needs_paint)
    else {
        return Ok(());
    };
    paint_layer_surface(runtime, client, surface, state.painter.layout_only())
}

/// Drops one configured layer surface the compositor closed.
pub fn layer_surface_closed(
    runtime: &mut Runtime,
    client: &mut dyn Backend,
    state: &mut SurfaceEventState,
    layer: u64,
) {
    client.close(WindowId::Layer(layer));
    let Some(id) = window_surface_id(layer) else {
        return;
    };
    if state.windows.remove(Kind::Layer, id).is_some() {
        runtime.set_window_surface_visible(id, false);
    }
}
