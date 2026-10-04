//! Keeping the compositor's windows in step with the configuration: opening,
//! moving and closing its popup, floating and layer windows.

use morf_lua::{Runtime, WindowSurfaceKind};
use morf_app::{ToplevelConfig, LayerClient, WindowId};
use std::collections::{HashMap, HashSet};

use crate::host::windows::{Kind, Windows};
use crate::{surface_layers::*, surface_popups::*};
use morf_app::{Backend as _, WindowKind};

use super::Window;

/// Opens, moves and closes the popup, floating and layer windows the
/// configuration asks for. A popup or floating window taken off screen here
/// has its `on_closed` run once the sync is done.
pub fn sync_window_surfaces(
    runtime: &mut Runtime,
    client: &mut LayerClient,
    windows: &mut Windows,
    output: &str,
) -> Result<bool, String> {
    let mut resumed = false;
    let surfaces = runtime.window_surface_configs();
    let surfaces_by_id = surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<HashMap<_, _>>();
    // A surface is wanted only when it and every ancestor it hangs off are
    // visible, and the three kinds are then handled in identifier order so a
    // parent is always opened before the child anchored to it.
    let desired = |wanted: fn(&WindowSurfaceKind) -> bool| {
        let mut surfaces = surfaces
            .iter()
            .filter(|surface| {
                wanted(&surface.kind)
                    && window_surface_effectively_visible(
                        surface.id,
                        &surfaces_by_id,
                        &mut HashSet::new(),
                    )
            })
            .collect::<Vec<_>>();
        surfaces.sort_by_key(|surface| surface.id);
        surfaces
    };
    let desired_popups = desired(|kind| matches!(kind, WindowSurfaceKind::Popup(_)));
    let desired_floatings = desired(|kind| matches!(kind, WindowSurfaceKind::Toplevel(_)));
    let desired_layers = desired(|kind| matches!(kind, WindowSurfaceKind::Layer(_)));
    let desired_popup_ids = desired_popups
        .iter()
        .map(|surface| surface.id)
        .collect::<HashSet<_>>();
    let desired_floating_ids = desired_floatings
        .iter()
        .map(|surface| surface.id)
        .collect::<HashSet<_>>();

    let mut stale_popups = windows
        .ids(Kind::Popup)
        .into_iter()
        .filter(|id| !desired_popup_ids.contains(id))
        .collect::<Vec<_>>();
    stale_popups.sort_unstable_by(|a, b| b.cmp(a));
    let mut closed = Vec::new();
    for id in stale_popups {
        client.close(WindowId::Popup(id));
        windows.remove(Kind::Popup, id);
        closed.push(id);
    }
    let mut stale_floatings = windows
        .ids(Kind::Toplevel)
        .into_iter()
        .filter(|id| !desired_floating_ids.contains(id))
        .collect::<Vec<_>>();
    stale_floatings.sort_unstable_by(|a, b| b.cmp(a));
    for id in stale_floatings {
        client.close(WindowId::Toplevel(id));
        windows.remove(Kind::Toplevel, id);
        closed.push(id);
    }
    resumed |= sync_layer_surfaces(client, output, &desired_layers, windows)?;
    let mut reopened = HashSet::new();
    for surface in desired_floatings {
        let id = surface.id;
        let WindowSurfaceKind::Toplevel(config) = &surface.kind else {
            unreachable!();
        };
        let changed = windows
            .get(Kind::Toplevel, id)
            .is_none_or(|current| current.floating_config.as_ref() != Some(config))
            || config
                .parent
                .is_some_and(|parent| reopened.contains(&parent));
        if changed {
            client.close(WindowId::Toplevel(id));
            client
                .open(WindowId::Toplevel(id), WindowKind::Toplevel { parent: config.parent, config: ToplevelConfig {
                        width: config.width,
                        height: config.height,
                        minimum_width: config.minimum_width,
                        minimum_height: config.minimum_height,
                        maximum_width: config.maximum_width,
                        maximum_height: config.maximum_height,
                        title: config.title.clone(),
                        app_id: config.app_id.clone(),
                        minimized: config.minimized,
                        maximized: config.maximized,
                        fullscreen: config.fullscreen,
                    } })
                .map_err(|error| error.to_string())?;
            reopened.insert(id);
            windows.insert(
                Kind::Toplevel,
                id,
                Window {
                    id: surface.id,
                    root: surface.root,
                    updates_enabled: surface.updates_enabled,
                    width: config.width,
                    height: config.height,
                    renderer: None,
                    layout: None,
                    popup_config: None,
                    floating_config: Some(config.clone()),
                    layer_config: None,
                    needs_paint: true,
                },
            );
        } else if let Some(current) = windows.get_mut(Kind::Toplevel, id) {
            // The stored size is the compositor's, from its last configure,
            // and is left alone: a change to the requested size is a change
            // to the config and reopens the window above. Writing the
            // requested size here put a window the person had resized back
            // to its first size on every sync — until the next configure.
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
    }
    for surface in desired_popups {
        let id = surface.id;
        let WindowSurfaceKind::Popup(config) = &surface.kind else {
            unreachable!();
        };
        // A popup the compositor has dismissed is gone from the client while the
        // host still tracks it, and has nothing left to reposition.
        let tracked = windows
            .get(Kind::Popup, id)
            .and_then(|current| current.popup_config.as_ref())
            .filter(|_| client.popup_surface(id).is_some());
        // A popup whose parent was just re-created is anchored to a surface that
        // no longer exists, so it follows its parent down whatever its geometry.
        let mut structural = tracked
            .is_none_or(|tracked| popup_change_is_structural(tracked, config))
            || config
                .parent
                .is_some_and(|parent| reopened.contains(&parent));
        if !structural && tracked != Some(config) {
            // Only the positioner moved, so the popup moves with its wl_surface,
            // its GPU surface and its swapchain all intact. A compositor whose
            // `xdg_popup` predates version 3 has no `reposition` request and says
            // so by changing nothing; then the popup has to be rebuilt after all.
            structural = !client
                .reposition_popup(id, popup_client_config(config)?)
                .map_err(|error| error.to_string())?;
        }
        if structural {
            let parent = popup_parent_role(config, &surfaces_by_id)?;
            open_popup_surface(client, surface, config, parent, windows)?;
            reopened.insert(id);
        } else if let Some(current) = windows.get_mut(Kind::Popup, id) {
            // The stored size is deliberately left alone. A repositioned popup
            // keeps its current dimensions until the compositor answers with the
            // configure carrying the geometry it settled on, and that configure
            // is also what resizes the swapchain — writing the requested size
            // here would let the two disagree for a frame.
            resumed |= !current.updates_enabled && surface.updates_enabled;
            let moved = current.root != surface.root;
            current.root = surface.root;
            current.updates_enabled = surface.updates_enabled;
            current.popup_config = Some(config.clone());
            if moved {
                current.layout = None;
            }
        }
    }
    // Last, with every surface settled: a callback that opens another window
    // is heard by the next sync rather than this one.
    for id in closed {
        resumed |= runtime.dispatch_window_closed(id);
    }
    Ok(resumed)
}
