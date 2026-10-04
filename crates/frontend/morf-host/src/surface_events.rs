use morf_lua::Runtime;
use morf_render::{RenderEngine, WgpuBackend};
use morf_app::Backend as _;
use morf_app::{Event, LayerClient, PRIMARY_LAYER, WindowId, physical_size};
use std::sync::mpsc;

use crate::render_target::surface_backend;
use crate::{
    lock::*, pacing::*, paint::*, surface_keys::*, surface_layers::*,
    surface_pointer::*, surfaces::*,
};

pub fn handle_surface_event(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &mut LayerClient,
    desktop: &mut morf_desktop::Desktop,
    state: &mut SurfaceEventState,
    event: Event,
    tx: &mpsc::Sender<SupervisorMessage>,
    name: &str,
) -> Result<bool, String> {
    let mut repaint = false;
    // Then selections and drags, which need the layout and the client.
    let event = match crate::surface_drag::handle_data_event(runtime, client, desktop, state, event) {
        Ok(repaint) => return repaint,
        Err(event) => event,
    };
    // Then the pointer and the fingers, which need only the layouts.
    let layouts = LayerLayouts {
        layout: &state.layout,
        popups: &state.popup_surfaces,
        floatings: &state.floating_surfaces,
        layers: &state.layer_surfaces,
    };
    let event = match handle_pointer_event(runtime, client, &mut state.input, &layouts, event)? {
        Ok(repaint) => return Ok(repaint),
        Err(event) => event,
    };
    match event {
        Event::Configure { id, .. } | Event::Scale { id, .. } if id == PRIMARY_LAYER => {
            let (width, height) = client.physical_size();
            renderer.resize(width, height);
            for surface in state
                .popup_surfaces
                .values_mut()
                .chain(state.floating_surfaces.values_mut())
            {
                if let Some(renderer) = &mut surface.renderer {
                    // Still the layer's scale here, as a fallback: a compositor
                    // with no fractional-scale protocol never sends `AuxScale`,
                    // and these surfaces would otherwise never be resized at
                    // all. Where it does, `AuxScale` arrives too and corrects
                    // this with the surface's own.
                    let (width, height) =
                        physical_size((surface.width, surface.height), client.primary_scale_120());
                    renderer.resize(width, height);
                }
            }
            // The opaque region is a size, so it follows the new one.
            apply_primary_opaque(runtime, client);
            repaint = true;
        }
        Event::Configure { id, width, height } => {
            layer_surface_configure(runtime, client, state, id, width, height)?;
        }
        Event::Scale { id, .. } => layer_surface_scale(runtime, client, state, id)?,
        Event::Frame { id, time_ms } if id == PRIMARY_LAYER => {
            repaint |= primary_frame(runtime, client, state, time_ms)?;
            repaint |= std::mem::take(&mut state.primary_deferred);
        }
        Event::Frame { id, .. } => layer_surface_frame(runtime, client, state, id)?,
        Event::Closed { id } if id == PRIMARY_LAYER => {
            return Err(crate::supervisor::SURFACE_CLOSED.to_owned());
        }
        Event::Closed { id } => layer_surface_closed(runtime, client, state, id),
        Event::Clipboard { text } => {
            repaint |= runtime.dispatch_clipboard(text);
        }
        Event::KeyboardFocus { active } => repaint |= runtime.dispatch_keyboard_focus(active),
        Event::SurfaceKeyboard { surface, focused } => {
            // The node with focus shows it only while its surface has the
            // keyboard, and shows it again when the keyboard comes back.
            if let Some(root) = surface_root(
                surface,
                state.primary_root,
                &state.popup_surfaces,
                &state.floating_surfaces,
                &state.layer_surfaces,
            ) {
                repaint |= runtime.set_focus_active(root, focused);
                state.keyboard_changes.push((root, focused));
            }
            if let Some(window) = surface_window(surface) {
                repaint |= runtime.dispatch_surface_focus(window, focused);
            }
        }
        Event::SurfacePointer { surface, inside } => {
            if let Some(window) = surface_window(surface) {
                repaint |= runtime.dispatch_surface_pointer(window, inside);
            }
        }
        Event::OfferRead { .. }
        | Event::DragEnter { .. }
        | Event::DragMotion { .. }
        | Event::DragLeave { .. }
        | Event::Drop { .. }
        | Event::DragSourceEnded { .. } => {}
        Event::InputMethod(state) => {
            repaint |= runtime.dispatch_input_method(
                state.active,
                state.surrounding_text,
                state.cursor,
                state.anchor,
                state.serial,
            );
        }
        Event::TextInput(state) => {
            repaint |= runtime.dispatch_text_input(
                state.focused,
                state.preedit,
                state.preedit_begin,
                state.preedit_end,
                state.commit,
                state.delete_before,
                state.delete_after,
                state.serial,
            );
        }
        Event::Key {
            surface,
            pressed,
            repeat,
            keysym,
            text,
            modifiers,
        } => {
            repaint |= surface_key(
                runtime,
                state,
                surface,
                KeyAction::of(pressed, repeat),
                keysym,
                text.as_deref(),
                modifiers,
            );
        }
        Event::PopupConfigure { id, width, height } => {
            if let Some(surface) = state.popup_surfaces.get_mut(&id) {
                let initial = surface.renderer.is_none();
                surface.width = width.max(1);
                surface.height = height.max(1);
                // Before the paint, so the bindings that read `win.width` and
                // `win.height` lay the root out at the size it is drawn at.
                runtime.set_window_surface_size(surface.id, surface.width, surface.height);
                let (physical_width, physical_height) = physical_size(
                    (surface.width, surface.height),
                    client.surface_scale_120(WindowId::Popup(id)),
                );
                if let Some(renderer) = &mut surface.renderer {
                    renderer.resize(physical_width, physical_height);
                } else {
                    let target = client
                        .render_target(WindowId::Popup(id))
                        .ok_or_else(|| "configured popup disappeared".to_owned())?;
                    let backend = surface_backend(target, physical_width, physical_height)
                    .map_err(|error| error.to_string())?;
                    surface.renderer = Some(RenderEngine::new(backend));
                }
                if initial || surface.updates_enabled {
                    paint_popup_surface(runtime, client, surface)?;
                }
            }
        }
        Event::ShortcutsInhibited { active } => {
            repaint |= runtime.dispatch_shortcuts_inhibited(active);
        }
        Event::AuxScale { role, scale_120 } => {
            // A popup on a 2x screen opened from a bar on a 1x one used to be
            // rendered at the bar's scale and stretched. It has its own now.
            let surface = match role {
                WindowId::Popup(id) => state.popup_surfaces.get_mut(&id),
                WindowId::Toplevel(id) => state.floating_surfaces.get_mut(&id),
                WindowId::Layer(_) | WindowId::Lock(_) => None,
            };
            if let Some(surface) = surface
                && let Some(renderer) = &mut surface.renderer
            {
                let (width, height) = physical_size((surface.width, surface.height), scale_120);
                renderer.resize(width, height);
                repaint = true;
            }
        }
        Event::PopupFrame { id, .. } => {
            if let Some(surface) = state
                .popup_surfaces
                .get_mut(&id)
                .filter(|surface| surface.updates_enabled)
            {
                paint_popup_surface(runtime, client, surface)?;
            }
        }
        Event::PopupDone { id } => {
            if let Some(surface) = state.popup_surfaces.remove(&id) {
                runtime.set_window_surface_visible(surface.id, false);
                repaint |= runtime.dispatch_window_closed(surface.id);
            }
        }
        Event::ToplevelConfigure { id, width, height } => {
            if let Some(surface) = state.floating_surfaces.get_mut(&id) {
                let initial = surface.renderer.is_none();
                surface.width = width.max(1);
                surface.height = height.max(1);
                // Before the paint, so the bindings that read `win.width` and
                // `win.height` lay the root out at the size it is drawn at.
                runtime.set_window_surface_size(surface.id, surface.width, surface.height);
                let (physical_width, physical_height) = physical_size(
                    (surface.width, surface.height),
                    client.surface_scale_120(WindowId::Toplevel(id)),
                );
                if let Some(renderer) = &mut surface.renderer {
                    renderer.resize(physical_width, physical_height);
                } else {
                    let target = client
                        .render_target(WindowId::Toplevel(id))
                        .ok_or_else(|| "configured floating surface disappeared".to_owned())?;
                    let backend = surface_backend(target, physical_width, physical_height)
                    .map_err(|error| error.to_string())?;
                    surface.renderer = Some(RenderEngine::new(backend));
                }
                if initial || surface.updates_enabled {
                    paint_floating_surface(runtime, client, surface)?;
                }
            }
        }
        Event::ToplevelFrame { id, .. } => {
            if let Some(surface) = state
                .floating_surfaces
                .get_mut(&id)
                .filter(|surface| surface.updates_enabled)
            {
                paint_floating_surface(runtime, client, surface)?;
            }
        }
        Event::ToplevelClose { id } => {
            // A request, not a close: the window stays until the
            // configuration's `on_close_requested` lets it go, and then the
            // next sync takes it down like any hidden window.
            if state.floating_surfaces.contains_key(&id) {
                repaint |= runtime.request_window_close(id);
            }
        }
        // Already taken above, by the pointer path.
        Event::PointerMotion { .. }
        | Event::PointerLeave { .. }
        | Event::PointerAxis { .. }
        | Event::PointerButton { .. }
        | Event::TouchDown { .. }
        | Event::TouchMotion { .. }
        | Event::TouchUp { .. }
        | Event::TouchCancel => {}
        Event::SessionLocked
        | Event::SessionLockFinished
        | Event::SessionLockConfigure { .. }
        | Event::SessionLockSurfaceRemoved { .. }
        | Event::SessionLockFrame { .. } => {}
        Event::Screens(screens) => {
            // This client sees every output, not just the one it draws to. The
            // supervisor records the list and hands it back to every worker, so
            // each runtime's `morf.screens` follows the hotplug rather than
            // keeping the entry for a monitor that has gone away.
            tx.send(SupervisorMessage::Worker(WorkerMessage::Screens {
                output: name.to_owned(),
                screens,
            }))
            .map_err(|_| "output supervisor stopped".to_owned())?;
        }
    }
    Ok(repaint)
}

/// Which of the configuration's surfaces a role is: `Some(None)` for the
/// shell's own, `Some(Some(id))` for a window, `None` for one the engine
/// keeps for itself (the backdrop, the edge reservers, a lock surface).
fn surface_window(surface: morf_app::WindowId) -> Option<Option<u64>> {
    use morf_app::WindowId;
    match surface {
        WindowId::Layer(morf_app::PRIMARY_LAYER) => Some(None),
        WindowId::Layer(layer) => crate::surface_layers::window_surface_id(layer)
            .filter(|_| layer < crate::surface_layers::RESERVE_LAYER_BASE)
            .map(Some),
        WindowId::Popup(id) | WindowId::Toplevel(id) => Some(Some(id)),
        WindowId::Lock(_) => None,
    }
}
