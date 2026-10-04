//! The Wayland backend as a [`Backend`]: each call is the protocol request
//! for the window's kind.

use std::time::Duration;

use crate::backend::wayland::{LayerClient, Woke};
use crate::backend::{Backend, Capabilities, RenderTarget, WindowKind};
use crate::{Edge, Event, InputRect, Output, WindowId};

impl Backend for LayerClient {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            layer_shell: self.supports_layer_shell(),
            layer_surfaces: self.supports_layer_surfaces(),
            live_layer_change: self.supports_live_layer_change(),
            toplevels: self.supports_toplevels(),
            popups: true,
            session_lock: self.state.has_session_lock,
            drag_and_drop: self.supports_drag_and_drop(),
            text_input: self.supports_text_input(),
            input_method: self.supports_input_method(),
            backdrop_blur: self.supports_backdrop_blur(),
        }
    }

    fn outputs(&self) -> &[Output] {
        self.screens()
    }

    fn open(&mut self, id: WindowId, kind: WindowKind) -> Result<(), String> {
        let opened = match (id, kind) {
            (WindowId::Layer(id), WindowKind::Layer(config)) => self.open_layer(id, config),
            (WindowId::Toplevel(id), WindowKind::Toplevel { parent, config }) => {
                self.open_floating(id, parent, config)
            }
            (WindowId::Popup(id), WindowKind::Popup { parent, config }) => {
                self.open_popup(id, parent, config)
            }
            (id, kind) => return Err(format!("{id:?} cannot be opened as {kind:?}")),
        };
        opened.map_err(|error| error.to_string())
    }

    fn close(&mut self, id: WindowId) {
        match id {
            WindowId::Layer(id) => self.close_layer(id),
            WindowId::Toplevel(id) => self.close_floating(id),
            WindowId::Popup(id) => self.close_popup(id),
            // Lock surfaces go with the lock.
            WindowId::Lock(_) => {}
        }
    }

    fn logical_size(&self, id: WindowId) -> Option<(u32, u32)> {
        match id {
            WindowId::Layer(id) => self.layer_logical_size(id),
            WindowId::Toplevel(id) => self.state.floating_sizes.get(&id).copied(),
            WindowId::Popup(id) => self.state.popup_sizes.get(&id).copied(),
            WindowId::Lock(index) => self.lock_size(index),
        }
    }

    fn scale_120(&self, id: WindowId) -> u32 {
        self.surface_scale_120(id)
    }

    fn request_frame(&self, id: WindowId) {
        match id {
            WindowId::Layer(id) => self.request_layer_frame(id),
            WindowId::Toplevel(id) => self.request_floating_frame(id),
            WindowId::Popup(id) => self.request_popup_frame(id),
            WindowId::Lock(index) => self.request_lock_frame(index),
        }
    }

    fn commit(&self, id: WindowId) {
        match id {
            WindowId::Layer(id) => self.commit_layer(id),
            WindowId::Lock(index) => self.commit_lock(index),
            WindowId::Toplevel(id) => {
                if let Some(surface) = self.floating_surface(id) {
                    surface.commit();
                }
            }
            WindowId::Popup(id) => {
                if let Some(surface) = self.popup_surface(id) {
                    surface.commit();
                }
            }
        }
    }

    fn set_input_region(&self, id: WindowId, region: Option<&[InputRect]>) {
        // Only layer surfaces take a region from the host; the others take
        // the pointer everywhere.
        if let WindowId::Layer(id) = id {
            self.set_layer_input_region(id, region);
        }
    }

    fn start_move(&self, id: WindowId) -> bool {
        matches!(id, WindowId::Toplevel(id) if self.start_floating_move(id))
    }

    fn start_resize(&self, id: WindowId, edge: Edge) -> bool {
        matches!(id, WindowId::Toplevel(id) if self.start_floating_resize(id, edge))
    }

    fn set_cursor(&mut self, shape: &str) -> bool {
        self.set_cursor_shape(shape)
    }

    fn lock(&mut self) -> Result<(), String> {
        self.begin_session_lock().map_err(|error| error.to_string())
    }

    fn unlock(&mut self) -> Result<(), String> {
        self.unlock_session().map_err(|error| error.to_string())
    }

    fn render_target(&self, id: WindowId) -> Option<RenderTarget> {
        let target = match id {
            WindowId::Layer(id) => self.layer_window_target(id),
            WindowId::Toplevel(id) => self.floating_window_target(id),
            WindowId::Popup(id) => self.popup_window_target(id),
            WindowId::Lock(index) => self.lock_window_target(index),
        };
        target.map(RenderTarget::Wayland)
    }

    fn next_event(&mut self) -> Option<Event> {
        LayerClient::next_event(self)
    }

    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<bool, String> {
        self.wait_for(timeout, None)
            .map(|woke| matches!(woke, Woke::Queued | Woke::Compositor))
            .map_err(|error| error.to_string())
    }
}
