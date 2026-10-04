use morf_app::Backend;
use morf_app::WindowKind;
use morf_app::{
    InputRect, PRIMARY_LAYER, PopupAnchor, PopupConfig, PopupConstraints, PopupGravity, WindowId,
};
use morf_lua::{PopupSurfaceConfig, WindowSurfaceConfig, WindowSurfaceKind};
use std::collections::{HashMap, HashSet};

use crate::host::windows::{Kind, Windows};
use crate::{surface_layers::*, surfaces::*};

// Popups: how a configuration's description of one becomes a positioner, and
// what has to be rebuilt when that description changes.

pub fn popup_anchor(value: &str) -> Result<PopupAnchor, String> {
    Ok(match value {
        "none" => PopupAnchor::None,
        "top" => PopupAnchor::Top,
        "bottom" => PopupAnchor::Bottom,
        "left" => PopupAnchor::Left,
        "right" => PopupAnchor::Right,
        "top_left" => PopupAnchor::TopLeft,
        "top_right" => PopupAnchor::TopRight,
        "bottom_left" => PopupAnchor::BottomLeft,
        "bottom_right" => PopupAnchor::BottomRight,
        value => return Err(format!("invalid popup anchor `{value}`")),
    })
}

pub fn popup_gravity(value: &str) -> Result<PopupGravity, String> {
    Ok(match value {
        "none" => PopupGravity::None,
        "top" => PopupGravity::Top,
        "bottom" => PopupGravity::Bottom,
        "left" => PopupGravity::Left,
        "right" => PopupGravity::Right,
        "top_left" => PopupGravity::TopLeft,
        "top_right" => PopupGravity::TopRight,
        "bottom_left" => PopupGravity::BottomLeft,
        "bottom_right" => PopupGravity::BottomRight,
        value => return Err(format!("invalid popup gravity `{value}`")),
    })
}

pub fn window_surface_parent(surface: &WindowSurfaceConfig) -> Option<u64> {
    match &surface.kind {
        WindowSurfaceKind::Popup(config) => config.parent,
        WindowSurfaceKind::Toplevel(config) => config.parent,
        WindowSurfaceKind::Layer(_) => None,
    }
}

pub fn window_surface_effectively_visible(
    id: u64,
    surfaces: &HashMap<u64, &WindowSurfaceConfig>,
    visiting: &mut HashSet<u64>,
) -> bool {
    let Some(surface) = surfaces.get(&id) else {
        return false;
    };
    if !surface.visible || !visiting.insert(id) {
        return false;
    }
    let visible = window_surface_parent(surface)
        .is_none_or(|parent| window_surface_effectively_visible(parent, surfaces, visiting));
    visiting.remove(&id);
    visible
}

/// Builds the client-side geometry request for one configured popup.
///
/// Every field here is a positioner field, which is what lets the same builder
/// serve both creating a popup and moving one.
pub fn popup_client_config(config: &PopupSurfaceConfig) -> Result<PopupConfig, String> {
    Ok(PopupConfig {
        anchor: InputRect {
            x: config.anchor_x,
            y: config.anchor_y,
            width: config.anchor_width,
            height: config.anchor_height,
        },
        width: config.width,
        height: config.height,
        anchor_edge: popup_anchor(&config.anchor_edge)?,
        gravity: popup_gravity(&config.gravity)?,
        offset_x: config.offset_x,
        offset_y: config.offset_y,
        constraints: PopupConstraints {
            slide_x: config.constraints.slide_x,
            slide_y: config.constraints.slide_y,
            flip_x: config.constraints.flip_x,
            flip_y: config.constraints.flip_y,
            resize_x: config.constraints.resize_x,
            resize_y: config.constraints.resize_y,
        },
        grab_focus: config.grab_focus,
    })
}

/// Whether a popup's new configuration needs a brand-new `xdg_popup`.
///
/// Everything a positioner carries — anchor rectangle, anchor edge, gravity,
/// offset, constraint policy, size — is replaceable on a mapped popup, so a
/// change to any of them is *positional* and goes through `xdg_popup.reposition`.
/// Two fields are not: the parent is bound when the popup object is created, and
/// the grab is taken once against an input serial. Only those force a teardown.
pub fn popup_change_is_structural(current: &PopupSurfaceConfig, next: &PopupSurfaceConfig) -> bool {
    current.parent != next.parent || current.grab_focus != next.grab_focus
}

/// Resolves the surface a popup anchors to, defaulting to the shell's own layer.
pub fn popup_parent_role(
    config: &PopupSurfaceConfig,
    surfaces_by_id: &HashMap<u64, &WindowSurfaceConfig>,
) -> Result<WindowId, String> {
    let Some(parent) = config.parent else {
        return Ok(WindowId::Layer(PRIMARY_LAYER));
    };
    let parent = surfaces_by_id
        .get(&parent)
        .ok_or_else(|| "popup parent is stale".to_owned())?;
    Ok(match parent.kind {
        WindowSurfaceKind::Popup(_) => WindowId::Popup(parent.id),
        WindowSurfaceKind::Toplevel(_) => WindowId::Toplevel(parent.id),
        WindowSurfaceKind::Layer(_) => WindowId::Layer(window_layer_id(parent.id)),
    })
}

/// Creates a popup's Wayland surface and records the host state that tracks it.
///
/// The renderer starts empty and is rebuilt from the first configure. It cannot
/// be carried over: a new `xdg_popup` carries a new `wl_surface`, and a wgpu
/// swapchain cannot outlive the surface it was created from. That cost is why
/// this path is taken only when the popup truly cannot be moved in place.
pub fn open_popup_surface(
    client: &mut dyn Backend,
    surface: &WindowSurfaceConfig,
    config: &PopupSurfaceConfig,
    parent: WindowId,
    windows: &mut Windows,
) -> Result<(), String> {
    client
        .open(
            WindowId::Popup(surface.id),
            WindowKind::Popup {
                parent,
                config: popup_client_config(config)?,
            },
        )
        .map_err(|error| error.to_string())?;
    windows.insert(
        Kind::Popup,
        surface.id,
        Window {
            id: surface.id,
            root: surface.root,
            updates_enabled: surface.updates_enabled,
            width: config.width,
            height: config.height,
            renderer: None,
            layout: None,
            popup_config: Some(config.clone()),
            floating_config: None,
            layer_config: None,
            needs_paint: true,
            owns_root: false,
            built_for: None,
        },
    );
    Ok(())
}
