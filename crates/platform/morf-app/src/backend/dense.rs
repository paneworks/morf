//! A backend seen in morf's pixels rather than the compositor's.
//!
//! [`Dense`] wraps any backend and converts at its edge: sizes and pointer
//! positions coming in are divided by each window's [`Zoom`], sizes, margins
//! and regions going out multiplied, and the scale a window reports is the
//! one [`Density`] picks for its panel. What is above it (the host, layout,
//! Lua) never sees a compositor pixel; what is below it never sees one of
//! morf's. Buffers keep the size the compositor shows, so nothing is
//! stretched.

use std::collections::HashMap;
use std::os::fd::BorrowedFd;
use std::sync::Arc;
use std::time::Duration;

use morf_value::density::{Density, Zoom, output_units, ppi};
use morf_value::region::{Rect, Region};

use super::{Backend, Capabilities, PRIMARY_LAYER, RenderTarget, WindowKind, Woke};
use crate::{
    Edge, Event, InputRect, KeyboardFocus, LayerConfig, Output, PopupConfig, ToplevelConfig,
    WindowId,
};

pub struct Dense<B: Backend + ?Sized = dyn Backend> {
    inner: Box<B>,
    density: Density,
    /// The inner backend's outputs, sized in morf's pixels.
    outputs: Vec<Output>,
    screens: Vec<Output>,
    /// How dense the shell's own panel is.
    own_ppi: Option<f64>,
    /// Each layer's geometry in morf's pixels, and the conversion it was
    /// sent with: a layer opened before its scale and output were known is
    /// sent again once they are, and again when they change.
    layers: HashMap<u64, (LayerConfig, Zoom)>,
}

/// How dense `output`'s panel is.
fn output_ppi(output: &Output) -> Option<f64> {
    let (width, height) = output.pixels?;
    let (wide, high) = output.physical_size?;
    let unsigned = |value: i32| u32::try_from(value).ok();
    ppi(
        (unsigned(width)?, unsigned(height)?),
        (unsigned(wide)?, unsigned(high)?),
    )
}

impl<B: Backend + ?Sized> Dense<B> {
    /// `inner`, seen at `density`.
    pub fn new(inner: Box<B>, density: Density) -> Self {
        let mut dense = Self {
            inner,
            density,
            outputs: Vec::new(),
            screens: Vec::new(),
            own_ppi: None,
            layers: HashMap::new(),
        };
        dense.refresh();
        dense
    }

    /// The backend underneath, for what only it has.
    pub fn inner(&self) -> &B {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.inner
    }

    /// Reads the outputs again: they come and go, and change modes.
    fn refresh(&mut self) {
        self.outputs = self
            .inner
            .outputs()
            .iter()
            .map(|o| self.output(o))
            .collect();
        self.screens = self
            .inner
            .screens()
            .iter()
            .map(|o| self.output(o))
            .collect();
        self.own_ppi = self
            .inner
            .window_output(WindowId::Layer(PRIMARY_LAYER))
            .as_ref()
            .and_then(output_ppi);
    }

    /// `output`, its logical size in morf's pixels.
    fn output(&self, output: &Output) -> Output {
        let mut seen = output.clone();
        let unsigned = |(a, b): (i32, i32)| Some((u32::try_from(a).ok()?, u32::try_from(b).ok()?));
        if let (Some(pixels), Some(logical)) = (
            output.pixels.and_then(unsigned),
            output.size.and_then(unsigned),
        ) {
            let ((width, height), _) = output_units(
                self.density,
                pixels,
                logical,
                output.physical_size.and_then(unsigned),
            );
            seen.size = Some((width as i32, height as i32));
        }
        seen
    }

    /// Sends again the geometry of each layer whose conversion changed.
    fn follow_zoom(&mut self) {
        let changed: Vec<(u64, LayerConfig, Zoom)> = self
            .layers
            .iter()
            .filter_map(|(id, (config, sent))| {
                let zoom = self.zoom(WindowId::Layer(*id));
                (zoom != *sent).then(|| (*id, config.clone(), zoom))
            })
            .collect();
        for (id, config, zoom) in changed {
            if self
                .inner
                .set_layer_geometry(id, &Self::layer_out(zoom, &config))
                .is_ok()
            {
                self.layers.insert(id, (config, zoom));
            }
        }
    }

    /// How dense the panel `id` is on.
    fn ppi_of(&self, id: WindowId) -> Option<f64> {
        match id {
            WindowId::Lock(_) => self.inner.window_output(id).as_ref().and_then(output_ppi),
            _ => self.own_ppi,
        }
    }

    /// The conversion at `id`; a window not open yet converts as the
    /// shell's own surface does.
    fn zoom(&self, id: WindowId) -> Zoom {
        let id = if self.inner.has_window(id) {
            id
        } else {
            WindowId::Layer(PRIMARY_LAYER)
        };
        let logical = self.inner.scale_120(id);
        Zoom::new(logical, self.density.scale_120(self.ppi_of(id), logical))
    }

    fn rect_out(zoom: Zoom, rect: &Rect) -> Rect {
        let (x, y, width, height) = zoom.rect_out(
            rect.x,
            rect.y,
            rect.width.max(0) as u32,
            rect.height.max(0) as u32,
        );
        Rect {
            x,
            y,
            width: width as i32,
            height: height as i32,
        }
    }

    fn layer_out(zoom: Zoom, config: &LayerConfig) -> LayerConfig {
        LayerConfig {
            width: zoom.size_out(config.width),
            height: zoom.size_out(config.height),
            exclusive_zone: zoom.length_out(config.exclusive_zone),
            margin_top: zoom.length_out(config.margin_top),
            margin_right: zoom.length_out(config.margin_right),
            margin_bottom: zoom.length_out(config.margin_bottom),
            margin_left: zoom.length_out(config.margin_left),
            ..config.clone()
        }
    }

    fn toplevel_out(zoom: Zoom, config: &ToplevelConfig) -> ToplevelConfig {
        ToplevelConfig {
            width: zoom.size_out(config.width),
            height: zoom.size_out(config.height),
            minimum_width: zoom.size_out(config.minimum_width),
            minimum_height: zoom.size_out(config.minimum_height),
            maximum_width: config.maximum_width.map(|value| zoom.size_out(value)),
            maximum_height: config.maximum_height.map(|value| zoom.size_out(value)),
            ..config.clone()
        }
    }

    fn popup_out(zoom: Zoom, config: &PopupConfig) -> PopupConfig {
        PopupConfig {
            anchor: Self::rect_out(zoom, &config.anchor),
            width: zoom.size_out(config.width),
            height: zoom.size_out(config.height),
            offset_x: zoom.length_out(config.offset_x),
            offset_y: zoom.length_out(config.offset_y),
            ..*config
        }
    }

    /// `event`, in morf's pixels.
    fn event_in(&mut self, event: Event) -> Event {
        let point = |dense: &Self, surface: WindowId, x: f64, y: f64| {
            let zoom = dense.zoom(surface);
            (zoom.point_in(x), zoom.point_in(y))
        };
        let size = |dense: &Self, id: WindowId, width: u32, height: u32| {
            let zoom = dense.zoom(id);
            (zoom.size_in(width), zoom.size_in(height))
        };
        match event {
            Event::Configure { id, width, height } => {
                let (width, height) = size(self, WindowId::Layer(id), width, height);
                Event::Configure { id, width, height }
            }
            Event::PopupConfigure { id, width, height } => {
                let (width, height) = size(self, WindowId::Popup(id), width, height);
                Event::PopupConfigure { id, width, height }
            }
            Event::ToplevelConfigure { id, width, height } => {
                let (width, height) = size(self, WindowId::Toplevel(id), width, height);
                Event::ToplevelConfigure { id, width, height }
            }
            Event::SessionLockConfigure {
                index,
                width,
                height,
            } => {
                let (width, height) = size(self, WindowId::Lock(index), width, height);
                Event::SessionLockConfigure {
                    index,
                    width,
                    height,
                }
            }
            Event::Scale { id, scale_120 } => Event::Scale {
                id,
                scale_120: self
                    .density
                    .scale_120(self.ppi_of(WindowId::Layer(id)), scale_120),
            },
            Event::AuxScale { role, scale_120 } => Event::AuxScale {
                role,
                scale_120: self.density.scale_120(self.ppi_of(role), scale_120),
            },
            Event::PointerMotion { surface, x, y } => {
                let (x, y) = point(self, surface, x, y);
                Event::PointerMotion { surface, x, y }
            }
            Event::PointerButton {
                surface,
                button,
                pressed,
                x,
                y,
                modifiers,
            } => {
                let (x, y) = point(self, surface, x, y);
                Event::PointerButton {
                    surface,
                    button,
                    pressed,
                    x,
                    y,
                    modifiers,
                }
            }
            Event::PointerAxis {
                surface,
                x,
                y,
                horizontal,
                vertical,
                horizontal_steps,
                vertical_steps,
                modifiers,
            } => {
                let zoom = self.zoom(surface);
                Event::PointerAxis {
                    surface,
                    x: zoom.point_in(x),
                    y: zoom.point_in(y),
                    horizontal: zoom.point_in(horizontal),
                    vertical: zoom.point_in(vertical),
                    horizontal_steps,
                    vertical_steps,
                    modifiers,
                }
            }
            Event::TouchDown { surface, id, x, y } => {
                let (x, y) = point(self, surface, x, y);
                Event::TouchDown { surface, id, x, y }
            }
            Event::TouchMotion { surface, id, x, y } => {
                let (x, y) = point(self, surface, x, y);
                Event::TouchMotion { surface, id, x, y }
            }
            Event::TouchUp { surface, id, x, y } => {
                let (x, y) = point(self, surface, x, y);
                Event::TouchUp { surface, id, x, y }
            }
            Event::DragEnter {
                surface,
                x,
                y,
                offer,
            } => {
                let (x, y) = point(self, surface, x, y);
                Event::DragEnter {
                    surface,
                    x,
                    y,
                    offer,
                }
            }
            Event::DragMotion { surface, x, y } => {
                let (x, y) = point(self, surface, x, y);
                Event::DragMotion { surface, x, y }
            }
            Event::Drop {
                surface,
                x,
                y,
                drop,
            } => {
                let (x, y) = point(self, surface, x, y);
                Event::Drop {
                    surface,
                    x,
                    y,
                    drop,
                }
            }
            Event::Screens(_) => {
                self.refresh();
                Event::Screens(self.screens.clone())
            }
            other => other,
        }
    }
}

impl<B: Backend + ?Sized> Backend for Dense<B> {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn outputs(&self) -> &[Output] {
        &self.outputs
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
        self.inner
            .window_output(id)
            .map(|output| self.output(&output))
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
    fn as_wayland(&self) -> Option<&super::wayland::LayerClient> {
        self.inner.as_wayland()
    }
    #[cfg(feature = "headless")]
    fn as_headless(&self) -> Option<&super::headless::HeadlessBackend> {
        self.inner.as_headless()
    }
    #[cfg(feature = "headless")]
    fn as_headless_mut(&mut self) -> Option<&mut super::headless::HeadlessBackend> {
        self.inner.as_headless_mut()
    }
    fn wait(
        &mut self,
        timeout: Option<Duration>,
        wake: Option<BorrowedFd<'_>>,
    ) -> Result<Woke, String> {
        self.inner.wait(timeout, wake)
    }
    fn has_queued_events(&self) -> bool {
        self.inner.has_queued_events()
    }
    fn set_waker(&mut self, waker: fn()) {
        self.inner.set_waker(waker);
    }
    fn screens(&self) -> &[Output] {
        &self.screens
    }
    fn own_output(&self) -> Option<Output> {
        self.inner.own_output().map(|output| self.output(&output))
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
