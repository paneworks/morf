use morf_lua::Runtime;
use morf_render::{RenderEngine, WgpuBackend};
use morf_wayland::{LayerClient, LayerEvent, PRIMARY_LAYER, SurfaceRole, physical_size};
use std::sync::mpsc;

use crate::{
    capture::*, lock::*, pacing::*, paint::*, surface_keys::*, surface_layers::*,
    surface_pointer::*, surfaces::*,
};

pub(crate) fn handle_surface_event(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &mut LayerClient,
    state: &mut SurfaceEventState,
    event: LayerEvent,
    tx: &mpsc::Sender<SupervisorMessage>,
    name: &str,
) -> Result<bool, String> {
    let mut repaint = false;
    // Captures first: they are the events that need the renderer and the
    // client at once, and they are handled where the rest of capture lives.
    let event = match handle_capture_event(runtime, renderer, client, event) {
        Ok(repaint) => return Ok(repaint),
        Err(event) => event,
    };
    // Then selections and drags, which need the layout and the client.
    let event = match crate::surface_drag::handle_data_event(runtime, client, state, event) {
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
        LayerEvent::Configure { id, .. } | LayerEvent::Scale { id, .. } if id == PRIMARY_LAYER => {
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
                        physical_size((surface.width, surface.height), client.scale_120());
                    renderer.resize(width, height);
                }
            }
            // The opaque region is a size, so it follows the new one.
            apply_primary_opaque(runtime, client);
            repaint = true;
        }
        LayerEvent::Configure { id, width, height } => {
            layer_surface_configure(runtime, client, state, id, width, height)?;
        }
        LayerEvent::Scale { id, .. } => layer_surface_scale(runtime, client, state, id)?,
        LayerEvent::Frame { id, time_ms } if id == PRIMARY_LAYER => {
            repaint |= primary_frame(runtime, client, state, time_ms)?;
            repaint |= std::mem::take(&mut state.primary_deferred);
        }
        LayerEvent::Frame { id, .. } => layer_surface_frame(runtime, client, state, id)?,
        LayerEvent::Closed { id } if id == PRIMARY_LAYER => {
            return Err("layer surface was closed".to_owned());
        }
        LayerEvent::Closed { id } => layer_surface_closed(runtime, client, state, id),
        LayerEvent::Idle {
            timeout_ms,
            input_only,
            idle,
        } => {
            repaint |= runtime.dispatch_idle(timeout_ms, input_only, idle);
        }
        LayerEvent::Clipboard { text } => {
            repaint |= runtime.dispatch_clipboard(text);
        }
        LayerEvent::KeyboardFocus { active } => repaint |= runtime.dispatch_keyboard_focus(active),
        // Already taken above; named so a new event cannot slip past unmatched.
        LayerEvent::Screencopy { .. } | LayerEvent::CaptureOffer { .. } => {}
        LayerEvent::Selection { .. }
        | LayerEvent::OfferRead { .. }
        | LayerEvent::DragEnter { .. }
        | LayerEvent::DragMotion { .. }
        | LayerEvent::DragLeave { .. }
        | LayerEvent::Drop { .. }
        | LayerEvent::DragSourceEnded { .. } => {}
        LayerEvent::InputMethod(state) => {
            repaint |= runtime.dispatch_input_method(
                state.active,
                state.surrounding_text,
                state.cursor,
                state.anchor,
                state.serial,
            );
        }
        LayerEvent::TextInput(state) => {
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
        LayerEvent::Key {
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
        LayerEvent::PopupConfigure { id, width, height } => {
            if let Some(surface) = state.popup_surfaces.get_mut(&id) {
                let initial = surface.renderer.is_none();
                surface.width = width.max(1);
                surface.height = height.max(1);
                let (physical_width, physical_height) = physical_size(
                    (surface.width, surface.height),
                    client.surface_scale_120(SurfaceRole::Popup(id)),
                );
                if let Some(renderer) = &mut surface.renderer {
                    renderer.resize(physical_width, physical_height);
                } else {
                    let target = client
                        .popup_window_target(id)
                        .ok_or_else(|| "configured popup disappeared".to_owned())?;
                    let backend = pollster::block_on(WgpuBackend::new_surface(
                        target,
                        physical_width,
                        physical_height,
                    ))
                    .map_err(|error| error.to_string())?;
                    surface.renderer = Some(RenderEngine::new(backend));
                }
                if initial || surface.updates_enabled {
                    paint_popup_surface(runtime, client, surface)?;
                }
            }
        }
        LayerEvent::ShortcutsInhibited { active } => {
            repaint |= runtime.dispatch_shortcuts_inhibited(active);
        }
        LayerEvent::AuxScale { role, scale_120 } => {
            // A popup on a 2x screen opened from a bar on a 1x one used to be
            // rendered at the bar's scale and stretched. It has its own now.
            let surface = match role {
                SurfaceRole::Popup(id) => state.popup_surfaces.get_mut(&id),
                SurfaceRole::Floating(id) => state.floating_surfaces.get_mut(&id),
                SurfaceRole::Layer(_) | SurfaceRole::Lock(_) => None,
            };
            if let Some(surface) = surface
                && let Some(renderer) = &mut surface.renderer
            {
                let (width, height) = physical_size((surface.width, surface.height), scale_120);
                renderer.resize(width, height);
                repaint = true;
            }
        }
        LayerEvent::PopupFrame { id, .. } => {
            if let Some(surface) = state
                .popup_surfaces
                .get_mut(&id)
                .filter(|surface| surface.updates_enabled)
            {
                paint_popup_surface(runtime, client, surface)?;
            }
        }
        LayerEvent::PopupDone { id } => {
            if let Some(surface) = state.popup_surfaces.remove(&id) {
                runtime.set_window_surface_visible(surface.id, false);
            }
        }
        LayerEvent::FloatingConfigure { id, width, height } => {
            if let Some(surface) = state.floating_surfaces.get_mut(&id) {
                let initial = surface.renderer.is_none();
                surface.width = width.max(1);
                surface.height = height.max(1);
                let (physical_width, physical_height) = physical_size(
                    (surface.width, surface.height),
                    client.surface_scale_120(SurfaceRole::Floating(id)),
                );
                if let Some(renderer) = &mut surface.renderer {
                    renderer.resize(physical_width, physical_height);
                } else {
                    let target = client
                        .floating_window_target(id)
                        .ok_or_else(|| "configured floating surface disappeared".to_owned())?;
                    let backend = pollster::block_on(WgpuBackend::new_surface(
                        target,
                        physical_width,
                        physical_height,
                    ))
                    .map_err(|error| error.to_string())?;
                    surface.renderer = Some(RenderEngine::new(backend));
                }
                if initial || surface.updates_enabled {
                    paint_floating_surface(runtime, client, surface)?;
                }
            }
        }
        LayerEvent::FloatingFrame { id, .. } => {
            if let Some(surface) = state
                .floating_surfaces
                .get_mut(&id)
                .filter(|surface| surface.updates_enabled)
            {
                paint_floating_surface(runtime, client, surface)?;
            }
        }
        LayerEvent::FloatingClose { id } => {
            if let Some(surface) = state.floating_surfaces.remove(&id) {
                runtime.set_window_surface_visible(surface.id, false);
            }
        }
        // Already taken above, by the pointer path.
        LayerEvent::PointerMotion { .. }
        | LayerEvent::PointerLeave { .. }
        | LayerEvent::PointerAxis { .. }
        | LayerEvent::PointerButton { .. }
        | LayerEvent::TouchDown { .. }
        | LayerEvent::TouchMotion { .. }
        | LayerEvent::TouchUp { .. }
        | LayerEvent::TouchCancel => {}
        LayerEvent::SessionLocked
        | LayerEvent::SessionLockFinished
        | LayerEvent::SessionLockConfigure { .. }
        | LayerEvent::SessionLockSurfaceRemoved { .. }
        | LayerEvent::SessionLockFrame { .. } => {}
        LayerEvent::Screens(screens) => {
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
