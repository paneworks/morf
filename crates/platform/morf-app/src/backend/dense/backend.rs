//! Backend forwarding and outgoing density conversion.

use super::*;

impl<B: Backend + ?Sized> Backend for Dense<B> {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn outputs(&self) -> &[Output] {
        self.inner.outputs()
    }
    fn open(&mut self, id: WindowId, kind: WindowKind) -> Result<(), String> {
        let kind = match kind {
            WindowKind::Layer(config) => {
                let zoom = self.zoom(id);
                if let WindowId::Layer(layer) = id {
                    self.layers.insert(layer, (config.clone(), zoom));
                }
                WindowKind::Layer(Self::layer_out(zoom, &config))
            }
            WindowKind::Toplevel { parent, config } => WindowKind::Toplevel {
                parent,
                config: Self::toplevel_out(self.zoom(id), &config),
            },
            WindowKind::Popup { parent, config } => WindowKind::Popup {
                parent,
                config: Self::popup_out(self.zoom(parent), &config),
            },
        };
        let opened = self.inner.open(id, kind);
        // The first surface is what says which output the shell is on.
        self.refresh();
        opened
    }
    fn close(&mut self, id: WindowId) {
        if let WindowId::Layer(layer) = id {
            self.layers.remove(&layer);
        }
        self.inner.close(id);
    }
    fn logical_size(&self, id: WindowId) -> Option<(u32, u32)> {
        let (width, height) = self.inner.logical_size(id)?;
        let zoom = self.zoom(id);
        Some((zoom.size_in(width), zoom.size_in(height)))
    }
    fn scale_120(&self, id: WindowId) -> u32 {
        self.zoom(id).units
    }
    fn buffer_size(&self, id: WindowId) -> Option<(u32, u32)> {
        self.inner.buffer_size(id)
    }
    fn window_output(&self, id: WindowId) -> Option<Output> {
        self.inner.window_output(id)
    }
    fn request_frame(&self, id: WindowId) {
        self.inner.request_frame(id);
    }
    fn commit(&self, id: WindowId) {
        self.inner.commit(id);
    }
    fn set_input_region(&self, id: WindowId, region: Option<&[InputRect]>) {
        let zoom = self.zoom(id);
        let region: Option<Vec<InputRect>> = region.map(|rects| {
            rects
                .iter()
                .map(|rect| Self::rect_out(zoom, rect))
                .collect()
        });
        self.inner.set_input_region(id, region.as_deref());
    }
    fn start_move(&self, id: WindowId) -> bool {
        self.inner.start_move(id)
    }
    fn start_resize(&self, id: WindowId, edge: Edge) -> bool {
        self.inner.start_resize(id, edge)
    }
    fn set_cursor(&mut self, shape: &str) -> bool {
        self.inner.set_cursor(shape)
    }
    fn lock(&mut self) -> Result<(), String> {
        self.inner.lock()
    }
    fn unlock(&mut self) -> Result<(), String> {
        self.inner.unlock()
    }
    fn render_target(&self, id: WindowId) -> Option<RenderTarget> {
        self.inner.render_target(id)
    }
    fn next_event(&mut self) -> Option<Event> {
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        let event = self.inner.next_event()?;
        // What a layer's conversion hangs on: its scale, and the output
        // it is on.
        let moves = matches!(
            event,
            Event::Configure { .. }
                | Event::Scale { .. }
                | Event::AuxScale { .. }
                | Event::Screens(_)
        );
        if moves {
            self.refresh();
        }
        let event = self.event_in(event);
        if moves {
            self.follow_zoom();
        }
        Some(event)
    }
    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<bool, String> {
        if !self.pending.is_empty() {
            return Ok(true);
        }
        self.inner.dispatch(timeout)
    }

    fn has_window(&self, id: WindowId) -> bool {
        self.inner.has_window(id)
    }
    fn damage(&self, id: WindowId, x: i32, y: i32, width: i32, height: i32) {
        // Buffer pixels: the same on both sides.
        self.inner.damage(id, x, y, width, height);
    }
    #[cfg(feature = "wayland")]
    fn as_wayland(&self) -> Option<&crate::backend::wayland::LayerClient> {
        self.inner.as_wayland()
    }
    #[cfg(feature = "headless")]
    fn as_headless(&self) -> Option<&crate::backend::headless::HeadlessBackend> {
        self.inner.as_headless()
    }
    #[cfg(feature = "headless")]
    fn as_headless_mut(&mut self) -> Option<&mut crate::backend::headless::HeadlessBackend> {
        self.inner.as_headless_mut()
    }
    fn wait(
        &mut self,
        timeout: Option<Duration>,
        wake: Option<BorrowedFd<'_>>,
    ) -> Result<Woke, String> {
        if !self.pending.is_empty() {
            return Ok(Woke::Queued);
        }
        self.inner.wait(timeout, wake)
    }
    fn has_queued_events(&self) -> bool {
        !self.pending.is_empty() || self.inner.has_queued_events()
    }
    fn set_waker(&mut self, waker: fn()) {
        self.inner.set_waker(waker);
    }
    fn screens(&self) -> &[Output] {
        self.inner.screens()
    }
    fn own_output(&self) -> Option<Output> {
        self.inner.own_output()
    }
    fn physical_size(&self) -> (u32, u32) {
        self.inner.physical_size()
    }
    fn lock_physical_size(&self, index: usize) -> Option<(u32, u32)> {
        self.inner.lock_physical_size(index)
    }
    fn layer_frame_wait(&self, id: u64) -> Option<Duration> {
        self.inner.layer_frame_wait(id)
    }

    fn supports_layer_shell(&self) -> bool {
        self.inner.supports_layer_shell()
    }
    fn supports_layer_surfaces(&self) -> bool {
        self.inner.supports_layer_surfaces()
    }
    fn supports_live_layer_change(&self) -> bool {
        self.inner.supports_live_layer_change()
    }
    fn supports_backdrop_blur(&self) -> bool {
        self.inner.supports_backdrop_blur()
    }
    fn supports_drag_and_drop(&self) -> bool {
        self.inner.supports_drag_and_drop()
    }
    fn supports_text_input(&self) -> bool {
        self.inner.supports_text_input()
    }
    fn supports_input_method(&self) -> bool {
        self.inner.supports_input_method()
    }
    fn supports_virtual_keyboard(&self) -> bool {
        self.inner.supports_virtual_keyboard()
    }
    fn supports_idle_inhibit(&self) -> bool {
        self.inner.supports_idle_inhibit()
    }
    fn supports_clipboard(&self) -> bool {
        self.inner.supports_clipboard()
    }
    fn can_set_clipboard(&self) -> bool {
        self.inner.can_set_clipboard()
    }

    fn set_layer_geometry(&mut self, id: u64, config: &LayerConfig) -> Result<(), String> {
        let zoom = self.zoom(WindowId::Layer(id));
        self.layers.insert(id, (config.clone(), zoom));
        self.inner
            .set_layer_geometry(id, &Self::layer_out(zoom, config))
    }
    fn map_layer_blank(&mut self, id: u64) -> Result<(), String> {
        self.inner.map_layer_blank(id)
    }
    fn set_layer_blank_color(&mut self, id: u64, alpha: u8) -> Result<(), String> {
        self.inner.set_layer_blank_color(id, alpha)
    }
    fn set_layer_opaque(&self, id: u64, opaque: bool) {
        self.inner.set_layer_opaque(id, opaque);
    }
    fn set_layer_keyboard_focus(&self, id: u64, focus: KeyboardFocus) -> bool {
        self.inner.set_layer_keyboard_focus(id, focus)
    }
    fn set_layer_composed_input_region(&self, id: u64, regions: &[Region]) -> Result<(), String> {
        // Composed over the size in morf's pixels, then converted as any
        // input region is.
        let (width, height) = self
            .layer_logical_size(id)
            .ok_or_else(|| "layer surface is not open".to_owned())?;
        let rectangles =
            morf_value::region::build(width, height, regions).map_err(|error| error.to_string())?;
        self.set_input_region(WindowId::Layer(id), Some(&rectangles));
        Ok(())
    }
    fn set_layer_backdrop_region(
        &self,
        id: u64,
        rectangles: Option<&[Rect]>,
    ) -> Result<(), String> {
        let zoom = self.zoom(WindowId::Layer(id));
        let rectangles: Option<Vec<Rect>> = rectangles.map(|rects| {
            rects
                .iter()
                .map(|rect| Self::rect_out(zoom, rect))
                .collect()
        });
        self.inner
            .set_layer_backdrop_region(id, rectangles.as_deref())
    }
    fn reposition_popup(&mut self, id: u64, config: PopupConfig) -> Result<bool, String> {
        let config = Self::popup_out(self.zoom(WindowId::Popup(id)), &config);
        self.inner.reposition_popup(id, config)
    }

    fn set_clipboard(&mut self, text: String) -> bool {
        self.inner.set_clipboard(text)
    }
    fn set_idle_inhibited(&mut self, inhibited: bool) -> bool {
        self.inner.set_idle_inhibited(inhibited)
    }
    fn set_shortcuts_inhibited(&mut self, inhibited: bool) -> bool {
        self.inner.set_shortcuts_inhibited(inhibited)
    }
    fn start_drag(&mut self, data: Vec<(String, Arc<Vec<u8>>)>) -> bool {
        self.inner.start_drag(data)
    }
    fn accept_drag(&mut self, mime: Option<&str>) {
        self.inner.accept_drag(mime);
    }
    fn finish_drop(&mut self) {
        self.inner.finish_drop();
    }
    fn read_offer(&mut self, request_id: u64, offer_id: u64, mime: &str) {
        self.inner.read_offer(request_id, offer_id, mime);
    }

    fn enable_text_input(&mut self) -> bool {
        self.inner.enable_text_input()
    }
    fn disable_text_input(&mut self) -> bool {
        self.inner.disable_text_input()
    }
    fn set_text_input_surrounding(&self, text: &str, cursor: i32, anchor: i32) -> bool {
        self.inner.set_text_input_surrounding(text, cursor, anchor)
    }
    fn set_text_input_cursor_rect(&self, rect: InputRect) -> bool {
        let zoom = self.zoom(WindowId::Layer(PRIMARY_LAYER));
        self.inner
            .set_text_input_cursor_rect(Self::rect_out(zoom, &rect))
    }
    fn set_text_input_content_type(&self, hints: u32, purpose: u32) -> bool {
        self.inner.set_text_input_content_type(hints, purpose)
    }
    fn enable_input_method(&mut self) -> bool {
        self.inner.enable_input_method()
    }
    fn input_method_commit(&self, text: &str) -> bool {
        self.inner.input_method_commit(text)
    }
    fn input_method_preedit(&self, text: &str, begin: i32, end: i32) -> bool {
        self.inner.input_method_preedit(text, begin, end)
    }
    fn input_method_delete(&self, before: u32, after: u32) -> bool {
        self.inner.input_method_delete(before, after)
    }
    fn send_virtual_key(&mut self, keycode: u32, pressed: bool) -> bool {
        self.inner.send_virtual_key(keycode, pressed)
    }
    fn send_virtual_modifiers(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) -> bool {
        self.inner
            .send_virtual_modifiers(depressed, latched, locked, group)
    }
}
