//! The Wayland backend as a [`Backend`]: each call is the protocol request
//! for the window's kind.

use std::sync::Arc;
use std::time::Duration;

use crate::backend::wayland::LayerClient;
use crate::backend::{Backend, Capabilities, RenderTarget, WindowKind, Woke};
use crate::{Edge, Event, InputRect, KeyboardFocus, LayerConfig, Output, PopupConfig, WindowId};

impl Backend for LayerClient {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            layer_shell: self.supports_layer_shell(),
            layer_surfaces: self.supports_layer_surfaces(),
            live_layer_change: self.supports_live_layer_change(),
            // xdg-shell is required, so a toplevel can always be opened.
            toplevels: true,
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

    fn has_window(&self, id: WindowId) -> bool {
        match id {
            WindowId::Layer(id) => self.layer_surface(id).is_some(),
            WindowId::Toplevel(id) => self.floating_surface(id).is_some(),
            WindowId::Popup(id) => self.popup_surface(id).is_some(),
            WindowId::Lock(index) => self.lock_size(index).is_some(),
        }
    }

    fn damage(&self, id: WindowId, x: i32, y: i32, width: i32, height: i32) {
        let surface = match id {
            WindowId::Layer(id) => self.layer_surface(id).cloned(),
            WindowId::Toplevel(id) => self.floating_surface(id).cloned(),
            WindowId::Popup(id) => self.popup_surface(id).cloned(),
            WindowId::Lock(index) => self.lock_surface(index).cloned(),
        };
        if let Some(surface) = surface {
            surface.damage_buffer(x, y, width, height);
        }
    }

    fn wait(
        &mut self,
        timeout: Option<Duration>,
        wake: Option<std::os::fd::BorrowedFd<'_>>,
    ) -> Result<Woke, String> {
        self.wait_for(timeout, wake)
            .map_err(|error| error.to_string())
    }
    fn has_queued_events(&self) -> bool {
        LayerClient::has_queued_events(self)
    }
    fn set_waker(&mut self, waker: fn()) {
        LayerClient::set_waker(self, waker)
    }

    fn as_wayland(&self) -> Option<&LayerClient> {
        Some(self)
    }

    fn screens(&self) -> &[Output] {
        LayerClient::screens(self)
    }
    fn own_output(&self) -> Option<Output> {
        LayerClient::own_output(self)
    }
    fn window_output(&self, id: WindowId) -> Option<Output> {
        match id {
            WindowId::Lock(index) => self.lock_screen(index),
            // Before it is known which output the shell is on, the only one.
            _ => LayerClient::own_output(self).or_else(|| match self.screens() {
                [only] => Some(only.clone()),
                _ => None,
            }),
        }
    }
    fn surface_scale_120(&self, id: WindowId) -> u32 {
        LayerClient::surface_scale_120(self, id)
    }
    fn primary_logical_size(&self) -> (u32, u32) {
        LayerClient::primary_logical_size(self)
    }
    fn primary_scale_120(&self) -> u32 {
        LayerClient::primary_scale_120(self)
    }
    fn physical_size(&self) -> (u32, u32) {
        LayerClient::physical_size(self)
    }
    fn layer_logical_size(&self, id: u64) -> Option<(u32, u32)> {
        LayerClient::layer_logical_size(self, id)
    }
    fn layer_scale_120(&self, id: u64) -> Option<u32> {
        LayerClient::layer_scale_120(self, id)
    }
    fn lock_size(&self, index: usize) -> Option<(u32, u32)> {
        LayerClient::lock_size(self, index)
    }
    fn lock_scale_120(&self, index: usize) -> Option<u32> {
        LayerClient::lock_scale_120(self, index)
    }
    fn lock_physical_size(&self, index: usize) -> Option<(u32, u32)> {
        LayerClient::lock_physical_size(self, index)
    }
    fn layer_frame_wait(&self, id: u64) -> Option<Duration> {
        LayerClient::layer_frame_wait(self, id)
    }

    fn supports_layer_shell(&self) -> bool {
        LayerClient::supports_layer_shell(self)
    }
    fn supports_layer_surfaces(&self) -> bool {
        LayerClient::supports_layer_surfaces(self)
    }
    fn supports_live_layer_change(&self) -> bool {
        LayerClient::supports_live_layer_change(self)
    }
    fn supports_backdrop_blur(&self) -> bool {
        LayerClient::supports_backdrop_blur(self)
    }
    fn supports_drag_and_drop(&self) -> bool {
        LayerClient::supports_drag_and_drop(self)
    }
    fn supports_text_input(&self) -> bool {
        LayerClient::supports_text_input(self)
    }
    fn supports_input_method(&self) -> bool {
        LayerClient::supports_input_method(self)
    }
    fn supports_virtual_keyboard(&self) -> bool {
        LayerClient::supports_virtual_keyboard(self)
    }
    fn supports_idle_inhibit(&self) -> bool {
        LayerClient::supports_idle_inhibit(self)
    }
    fn supports_clipboard(&self) -> bool {
        LayerClient::supports_clipboard(self)
    }
    fn can_set_clipboard(&self) -> bool {
        LayerClient::can_set_clipboard(self)
    }

    fn set_layer_geometry(&mut self, id: u64, config: &LayerConfig) -> Result<(), String> {
        LayerClient::set_layer_geometry(self, id, config).map_err(|error| error.to_string())
    }
    fn map_layer_blank(&mut self, id: u64) -> Result<(), String> {
        LayerClient::map_layer_blank(self, id).map_err(|error| error.to_string())
    }
    fn set_layer_blank_color(&mut self, id: u64, alpha: u8) -> Result<(), String> {
        LayerClient::set_layer_blank_color(self, id, alpha).map_err(|error| error.to_string())
    }
    fn set_layer_opaque(&self, id: u64, opaque: bool) {
        LayerClient::set_layer_opaque(self, id, opaque)
    }
    fn set_layer_keyboard_focus(&self, id: u64, focus: KeyboardFocus) -> bool {
        LayerClient::set_layer_keyboard_focus(self, id, focus)
    }
    fn set_layer_composed_input_region(
        &self,
        id: u64,
        regions: &[morf_value::region::Region],
    ) -> Result<(), String> {
        LayerClient::set_layer_composed_input_region(self, id, regions)
            .map_err(|error| error.to_string())
    }
    fn set_layer_backdrop_region(
        &self,
        id: u64,
        rectangles: Option<&[morf_value::region::Rect]>,
    ) -> Result<(), String> {
        LayerClient::set_layer_backdrop_region(self, id, rectangles)
            .map_err(|error| error.to_string())
    }
    fn reposition_popup(&mut self, id: u64, config: PopupConfig) -> Result<bool, String> {
        LayerClient::reposition_popup(self, id, config).map_err(|error| error.to_string())
    }

    fn set_clipboard(&mut self, text: String) -> bool {
        LayerClient::set_clipboard(self, text)
    }
    fn set_idle_inhibited(&mut self, inhibited: bool) -> bool {
        LayerClient::set_idle_inhibited(self, inhibited)
    }
    fn set_shortcuts_inhibited(&mut self, inhibited: bool) -> bool {
        LayerClient::set_shortcuts_inhibited(self, inhibited)
    }
    fn start_drag(&mut self, data: Vec<(String, Arc<Vec<u8>>)>) -> bool {
        LayerClient::start_drag(self, data)
    }
    fn accept_drag(&mut self, mime: Option<&str>) {
        LayerClient::accept_drag(self, mime)
    }
    fn finish_drop(&mut self) {
        LayerClient::finish_drop(self)
    }
    fn read_offer(&mut self, request_id: u64, offer_id: u64, mime: &str) {
        LayerClient::read_offer(self, request_id, offer_id, mime)
    }

    fn enable_text_input(&mut self) -> bool {
        LayerClient::enable_text_input(self)
    }
    fn disable_text_input(&mut self) -> bool {
        LayerClient::disable_text_input(self)
    }
    fn set_text_input_surrounding(&self, text: &str, cursor: i32, anchor: i32) -> bool {
        LayerClient::set_text_input_surrounding(self, text, cursor, anchor)
    }
    fn set_text_input_cursor_rect(&self, rect: InputRect) -> bool {
        LayerClient::set_text_input_cursor_rect(self, rect)
    }
    fn set_text_input_content_type(&self, hints: u32, purpose: u32) -> bool {
        LayerClient::set_text_input_content_type(self, hints, purpose)
    }
    fn enable_input_method(&mut self) -> bool {
        LayerClient::enable_input_method(self)
    }
    fn input_method_commit(&self, text: &str) -> bool {
        LayerClient::input_method_commit(self, text)
    }
    fn input_method_preedit(&self, text: &str, begin: i32, end: i32) -> bool {
        LayerClient::input_method_preedit(self, text, begin, end)
    }
    fn input_method_delete(&self, before: u32, after: u32) -> bool {
        LayerClient::input_method_delete(self, before, after)
    }
    fn send_virtual_key(&mut self, keycode: u32, pressed: bool) -> bool {
        LayerClient::send_virtual_key(self, keycode, pressed)
    }
    fn send_virtual_modifiers(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) -> bool {
        LayerClient::send_virtual_modifiers(self, depressed, latched, locked, group)
    }
}
