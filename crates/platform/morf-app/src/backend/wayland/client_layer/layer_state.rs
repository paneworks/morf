//! The layer state's own bookkeeping: subsurface stacking, which surface
//! keys go to, and the parent a popup attaches to.

use wayland_client::protocol::{wl_subcompositor, wl_surface};

use crate::backend::wayland::{state_types::*, surface_types::*};
use crate::placement::{arrange, stacking, stacks_below_primary};

use super::PRIMARY_LAYER;

/// Which surface keys go to under the layer-shell fallback.
///
/// Every stand-in shares the toplevel's keyboard focus, so the compositor
/// cannot choose: of the surfaces asking for the keyboard, one that asks
/// exclusively wins over one that asks on demand, and the latest to ask wins
/// among equals. `None` when none asks, and keys stay where the compositor
/// sent them. `surfaces` is `(id, focus, serial)`.
pub(crate) fn fallback_key_target(surfaces: &[(u64, KeyboardFocus, u64)]) -> Option<u64> {
    surfaces
        .iter()
        .filter(|(_, focus, _)| *focus != KeyboardFocus::None)
        .max_by_key(|(id, focus, serial)| (*focus == KeyboardFocus::Exclusive, *serial, *id))
        .map(|(id, _, _)| *id)
}

impl LayerState {
    /// Hands out the next value of the layer counter.
    pub(crate) fn next_layer_sequence(&self) -> u64 {
        let next = self.layer_sequence.get() + 1;
        self.layer_sequence.set(next);
        next
    }

    /// The surface and subcompositor a new layer surface hangs from under the
    /// layer-shell fallback, or `None` when it has to be a toplevel itself:
    /// it is the primary, or there is no primary toplevel yet, or no
    /// subcompositor.
    pub(crate) fn subsurface_parent(
        &self,
        id: u64,
    ) -> Option<(wl_surface::WlSurface, wl_subcompositor::WlSubcompositor)> {
        if id == PRIMARY_LAYER {
            return None;
        }
        let primary = self.layers.get(&PRIMARY_LAYER)?;
        primary.surface.as_window()?;
        Some((
            primary.surface.wl_surface().clone(),
            self.subcompositor.clone()?,
        ))
    }

    /// Gives every subsurface a new role object under a primary that was just
    /// recreated: the old parent's wl_surface is gone, and a subsurface whose
    /// parent is destroyed stays unmapped for good.
    pub(crate) fn reparent_subsurfaces(&mut self, qh: &wayland_client::QueueHandle<Self>) {
        let Some((parent, subcompositor)) = self.subsurface_parent(u64::MAX) else {
            return;
        };
        for record in self.layers.values_mut() {
            let ShellSurface::Subsurface(sub) = &mut record.surface else {
                continue;
            };
            sub.subsurface.destroy();
            sub.subsurface = subcompositor.get_subsurface(&sub.surface, &parent, qh, ());
            sub.subsurface.set_desync();
            record.placed = None;
        }
        self.subsurface_stack.clear();
    }

    /// Places every subsurface standing in for a layer surface.
    ///
    /// The primary's configured size stands for the output's, and each
    /// subsurface goes where layer-shell would put its surface on it
    /// (`placement::arrange`). One that moves gets a new position; one
    /// whose size changed, or that was never placed, gets a configure of its
    /// own — the compositor sends a subsurface none, and nothing paints until
    /// one arrives. Positions and stacking are pending state of the parent, so
    /// the primary is committed when either changed.
    pub(crate) fn arrange_subsurfaces(&mut self) {
        let Some(primary) = self.layers.get(&PRIMARY_LAYER) else {
            return;
        };
        if primary.surface.as_window().is_none() || !primary.configured {
            return;
        }
        let size = (primary.width, primary.height);
        let parent = primary.surface.wl_surface().clone();
        let mut subs = self
            .layers
            .iter()
            .filter(|(_, record)| record.surface.as_subsurface().is_some())
            .map(|(id, record)| (record.sequence, *id, record.request))
            .collect::<Vec<_>>();
        subs.sort_unstable_by_key(|(sequence, id, _)| (*sequence, *id));
        let requests = subs
            .iter()
            .map(|(_, id, request)| (*id, *request))
            .collect::<Vec<_>>();
        let mut commit_parent = false;
        for (id, placement) in arrange(size, &requests) {
            let Some(record) = self.layers.get_mut(&id) else {
                continue;
            };
            let Some(sub) = record.surface.as_subsurface() else {
                continue;
            };
            let position = (placement.x, placement.y);
            if record.placed != Some(position) {
                sub.subsurface.set_position(position.0, position.1);
                record.placed = Some(position);
                commit_parent = true;
            }
            if record.configured
                && record.width == placement.width
                && record.height == placement.height
            {
                continue;
            }
            record.width = placement.width;
            record.height = placement.height;
            if let Some(viewport) = &record.viewport {
                viewport.set_destination(record.width as i32, record.height as i32);
            }
            record.configured = true;
            self.attach_blank_buffer(id);
            self.events.push_back(Event::Configure {
                id,
                width: placement.width,
                height: placement.height,
            });
        }
        let order = stacking(
            &subs
                .iter()
                .map(|(sequence, id, request)| (*id, request.layer, *sequence))
                .collect::<Vec<_>>(),
        );
        if order != self.subsurface_stack {
            // Bottom first. Each one below the primary is put directly under
            // it, so the last of them ends up nearest to it; the ones above
            // go in reverse, each directly over it, for the same reason.
            let layers = &self.layers;
            let sub_of = |id: &u64| {
                layers.get(id).and_then(|record| {
                    Some((record.surface.as_subsurface()?, record.request.layer))
                })
            };
            for (sub, _) in order
                .iter()
                .filter_map(sub_of)
                .filter(|(_, layer)| stacks_below_primary(*layer))
            {
                sub.subsurface.place_below(&parent);
            }
            for (sub, _) in order
                .iter()
                .rev()
                .filter_map(sub_of)
                .filter(|(_, layer)| !stacks_below_primary(*layer))
            {
                sub.subsurface.place_above(&parent);
            }
            self.subsurface_stack = order;
            commit_parent = true;
        }
        if commit_parent {
            parent.commit();
        }
    }

    /// The role keys are delivered to.
    ///
    /// Whatever the compositor focused, except under the layer-shell fallback
    /// with the toplevel focused: every layer surface then shares that one
    /// focus, and keys go to the one that most recently asked for them
    /// ([`fallback_key_target`]).
    pub(crate) fn key_target(&self) -> WindowId {
        let held = self
            .keyboard_surface
            .unwrap_or(WindowId::Layer(PRIMARY_LAYER));
        if self.layer_shell.is_some() || held != WindowId::Layer(PRIMARY_LAYER) {
            return held;
        }
        let askers = self
            .layers
            .iter()
            .map(|(id, record)| {
                let (focus, serial) = record.keyboard.get();
                (*id, focus, serial)
            })
            .collect::<Vec<_>>();
        fallback_key_target(&askers).map_or(held, WindowId::Layer)
    }

    /// A press on a layer surface that takes the keyboard on demand makes it
    /// the latest to ask, under the layer-shell fallback — the click that
    /// would have focused it on a compositor with layer-shell.
    pub(crate) fn note_press(&self, role: WindowId) {
        if self.layer_shell.is_some() {
            return;
        }
        let WindowId::Layer(id) = role else {
            return;
        };
        let Some(record) = self.layers.get(&id) else {
            return;
        };
        if record.keyboard.get().0 == KeyboardFocus::OnDemand {
            record
                .keyboard
                .set((KeyboardFocus::OnDemand, self.next_layer_sequence()));
        }
    }

    /// What a popup opened against layer surface `id` hangs from under the
    /// layer-shell fallback, and the offset to add to its anchor rectangle:
    /// the fallback toplevel, plus a subsurface's position inside it. `None`
    /// with layer-shell, where the popup is attached with `get_popup`.
    pub(crate) fn fallback_popup_parent(
        &self,
        id: u64,
    ) -> Option<(
        &smithay_client_toolkit::shell::xdg::window::Window,
        (i32, i32),
    )> {
        if self.layer_shell.is_some() {
            return None;
        }
        let record = self.layers.get(&id)?;
        let window = self.layers.get(&PRIMARY_LAYER)?.surface.as_window()?;
        let offset = match &record.surface {
            ShellSurface::Window(_) => (0, 0),
            ShellSurface::Subsurface(_) => record.placed.unwrap_or((0, 0)),
            ShellSurface::Layer(_) => return None,
        };
        Some((window, offset))
    }
}
