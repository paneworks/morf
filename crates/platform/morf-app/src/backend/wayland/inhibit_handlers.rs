//! Things the shell asks the compositor *not* to do.
//!
//! Idle inhibition and shortcut inhibition are the same shape: an object whose
//! existence is the request, destroyed to withdraw it. Split from
//! `protocol_handlers` at the line gate, and a fair seam -- everything there
//! describes a surface; these describe a favour asked of the compositor.

use std::collections::HashSet;

use smithay_client_toolkit::shell::WaylandSurface;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::{self, ZwpKeyboardShortcutsInhibitorV1},
};

use crate::backend::wayland::{
    state_types::LayerState,
    surface_types::{LayerEvent, SurfaceRole},
};

// Neither half of idle inhibition says anything back: the manager only makes
// inhibitors, and an inhibitor is a token whose existence is the whole message.
wayland_client::delegate_noop!(LayerState: ignore ZwpIdleInhibitManagerV1);
wayland_client::delegate_noop!(LayerState: ignore ZwpIdleInhibitorV1);
wayland_client::delegate_noop!(LayerState: ignore ZwpKeyboardShortcutsInhibitManagerV1);

/// The shell's hold on the compositor's shortcuts, as one answer.
///
/// The protocol inhibits per surface, and honours an inhibitor only while its
/// surface has keyboard focus: a shell whose key field sits in a floating
/// window (a settings window rebinding a key) holds nothing by inhibiting its
/// layer surface. So every surface that can take the keyboard gets one, and
/// the shell hears a single yes or no: whether *any* of them is honoured.
/// Focus moving from one of the shell's surfaces to another is not news.
#[derive(Debug, Default)]
pub(crate) struct ShortcutsInhibit {
    /// What the shell asked for.
    pub(crate) wanted: bool,
    active: HashSet<SurfaceRole>,
}

impl ShortcutsInhibit {
    /// Whether the compositor honours any of the shell's inhibitors.
    pub(crate) fn active(&self) -> bool {
        !self.active.is_empty()
    }

    /// Notes the compositor's answer for one surface; returns the shell-wide
    /// answer when it changed.
    pub(crate) fn set(&mut self, role: SurfaceRole, on: bool) -> Option<bool> {
        let before = self.active();
        if on {
            self.active.insert(role);
        } else {
            self.active.remove(&role);
        }
        (self.active() != before).then(|| self.active())
    }

    /// Forgets every answer (the shell let go); returns `Some(false)` when
    /// one was being honoured.
    pub(crate) fn clear(&mut self) -> Option<bool> {
        let before = self.active();
        self.active.clear();
        before.then_some(false)
    }
}

impl LayerState {
    /// Asks the compositor to stop taking the shell's keys, or lets it again.
    ///
    /// A compositor binds keys for itself -- Super for the launcher, Alt-Tab
    /// for the switcher -- and a shell that draws its own launcher, or a key
    /// field recording a new shortcut, never sees the key it most wants. This
    /// says: while any of my surfaces has focus, give me all of them. Whether
    /// the compositor agrees arrives as an event.
    pub(crate) fn set_shortcuts_inhibited(&mut self, inhibited: bool, qh: &QueueHandle<Self>) {
        if inhibited == self.shortcuts_inhibit.wanted {
            return;
        }
        self.shortcuts_inhibit.wanted = inhibited;
        if !inhibited {
            for (_, inhibitor) in self.shortcuts_inhibitors.drain() {
                inhibitor.destroy();
            }
            if let Some(active) = self.shortcuts_inhibit.clear() {
                self.events
                    .push_back(LayerEvent::ShortcutsInhibited { active });
            }
            return;
        }
        let mut surfaces: Vec<(SurfaceRole, WlSurface)> = Vec::new();
        if let Some(layer) = self.layers.get(&crate::backend::wayland::PRIMARY_LAYER) {
            surfaces.push((
                SurfaceRole::Layer(crate::backend::wayland::PRIMARY_LAYER),
                layer.surface.wl_surface().clone(),
            ));
        }
        for (id, window) in &self.floatings {
            surfaces.push((SurfaceRole::Floating(*id), window.wl_surface().clone()));
        }
        for (role, surface) in surfaces {
            self.inhibit_surface_shortcuts(role, &surface, qh);
        }
    }

    /// Gives one surface an inhibitor while the shell holds the shortcuts --
    /// for a floating window opened after the shell asked. A second inhibitor
    /// on the same surface is a protocol error, so one that has one is left.
    pub(crate) fn inhibit_surface_shortcuts(
        &mut self,
        role: SurfaceRole,
        surface: &WlSurface,
        qh: &QueueHandle<Self>,
    ) {
        if !self.shortcuts_inhibit.wanted || self.shortcuts_inhibitors.contains_key(&role) {
            return;
        }
        let (Some(manager), Some(seat)) = (
            self.shortcuts_inhibit_manager.as_ref(),
            self.seats.seats().next(),
        ) else {
            return;
        };
        let inhibitor = manager.inhibit_shortcuts(surface, &seat, qh, role);
        self.shortcuts_inhibitors.insert(role, inhibitor);
    }

    /// Withdraws one surface's inhibitor, before the surface goes.
    pub(crate) fn release_surface_shortcuts(&mut self, role: SurfaceRole) {
        if let Some(inhibitor) = self.shortcuts_inhibitors.remove(&role) {
            inhibitor.destroy();
        }
        if let Some(active) = self.shortcuts_inhibit.set(role, false) {
            self.events
                .push_back(LayerEvent::ShortcutsInhibited { active });
        }
    }
}

impl Dispatch<ZwpKeyboardShortcutsInhibitorV1, SurfaceRole> for LayerState {
    /// Whether the compositor is actually honouring the request.
    ///
    /// Asking is not getting: a compositor may refuse, or grant and later
    /// withdraw when focus moves. A shell that assumed its keys were its own
    /// would draw a launcher and then watch Super open the compositor's.
    fn event(
        state: &mut Self,
        _inhibitor: &ZwpKeyboardShortcutsInhibitorV1,
        event: zwp_keyboard_shortcuts_inhibitor_v1::Event,
        role: &SurfaceRole,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let on = match event {
            zwp_keyboard_shortcuts_inhibitor_v1::Event::Active => true,
            zwp_keyboard_shortcuts_inhibitor_v1::Event::Inactive => false,
            _ => return,
        };
        if let Some(active) = state.shortcuts_inhibit.set(*role, on) {
            state
                .events
                .push_back(LayerEvent::ShortcutsInhibited { active });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShortcutsInhibit;
    use crate::backend::wayland::surface_types::SurfaceRole;

    #[test]
    fn focus_moving_between_the_shells_surfaces_is_not_news() {
        let mut hold = ShortcutsInhibit::default();
        assert_eq!(hold.set(SurfaceRole::Layer(0), true), Some(true));
        // The settings window takes focus: the layer's inhibitor goes
        // inactive after the window's went active, and the shell still holds.
        assert_eq!(hold.set(SurfaceRole::Floating(3), true), None);
        assert_eq!(hold.set(SurfaceRole::Layer(0), false), None);
        assert!(hold.active());
        // Focus leaves the shell entirely.
        assert_eq!(hold.set(SurfaceRole::Floating(3), false), Some(false));
        assert!(!hold.active());
    }

    #[test]
    fn an_answer_for_a_window_alone_is_the_shells_answer() {
        let mut hold = ShortcutsInhibit::default();
        assert_eq!(hold.set(SurfaceRole::Floating(7), true), Some(true));
        assert_eq!(hold.set(SurfaceRole::Floating(7), true), None);
        assert_eq!(hold.clear(), Some(false));
        assert_eq!(hold.clear(), None);
    }
}
