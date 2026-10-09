//! A backend seen in morf's pixels rather than the compositor's.
//!
//! [`Dense`](crate::Dense) wraps any backend and converts at its edge: sizes and pointer
//! positions coming in are divided by each window's [`Zoom`](morf_value::density::Zoom), sizes, margins
//! and regions going out multiplied, and the scale a window reports is the
//! one [`Density`](morf_value::density::Density) picks for its panel. Windows above it (the host, layout)
//! never see a compositor pixel; what is below it never sees one of morf's.
//! Outputs pass as the compositor tells them (a configuration's
//! `morf.screens` converts them itself). Buffers keep the size the
//! compositor shows, so nothing is stretched.

use std::collections::{HashMap, VecDeque};
use std::os::fd::BorrowedFd;
use std::sync::Arc;
use std::time::Duration;

use morf_value::density::{Density, Zoom, ppi};
use morf_value::region::{Rect, Region};

use super::{Backend, Capabilities, PRIMARY_LAYER, RenderTarget, WindowKind, Woke};
use crate::{
    Edge, Event, InputRect, KeyboardFocus, LayerConfig, Output, PopupConfig, ToplevelConfig,
    WindowId,
};

mod backend;

pub struct Dense<B: Backend + ?Sized = dyn Backend> {
    inner: Box<B>,
    density: Density,
    /// How dense the shell's own panel is.
    own_ppi: Option<f64>,
    /// Each layer's geometry in morf's pixels, and the conversion it was
    /// sent with: a layer opened before its scale and output were known is
    /// sent again once they are, and again when they change.
    layers: HashMap<u64, (LayerConfig, Zoom)>,
    /// What a change of density says to the host before anything else: each
    /// window's new scale and size, as if the compositor had sent them.
    pending: VecDeque<Event>,
}

/// `MORF_DENSITY_LOG=1`: what density each window is seen at, and when it
/// changes.
fn log(line: impl FnOnce() -> String) {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("MORF_DENSITY_LOG").is_some_and(|v| v != "0")) {
        eprintln!("morf: {}", line());
    }
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
        log(|| format!("density {density:?} at start"));
        let mut dense = Self {
            inner,
            density,
            own_ppi: None,
            layers: HashMap::new(),
            pending: VecDeque::new(),
        };
        dense.refresh();
        dense
    }

    /// The density it is seen at.
    pub fn density(&self) -> Density {
        self.density
    }

    /// Sees the backend at `density` from now on: every layer's geometry is
    /// sent again in the new pixels, and each window's new scale and size
    /// are queued for the host as the compositor's events would be.
    pub fn set_density(&mut self, density: Density) {
        if density == self.density {
            return;
        }
        log(|| format!("density {:?} -> {density:?}", self.density));
        self.density = density;
        self.refresh();
        self.follow_zoom();
        let ids: Vec<u64> = self.layers.keys().copied().collect();
        for id in ids {
            let window = WindowId::Layer(id);
            self.pending.push_back(Event::Scale {
                id,
                scale_120: self.scale_120(window),
            });
            if let Some((width, height)) = self.logical_size(window) {
                self.pending
                    .push_back(Event::Configure { id, width, height });
            }
        }
    }

    /// Takes a layer the inner backend opened before this one saw it (the
    /// shell's own, opened as the client connected), with its geometry in
    /// morf's pixels, so a later change of density sends it again.
    pub fn adopt_layer(&mut self, id: u64, config: LayerConfig) {
        let zoom = self.zoom(WindowId::Layer(id));
        self.layers.insert(id, (config, zoom));
    }

    /// The backend underneath, for what only it has.
    pub fn inner(&self) -> &B {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.inner
    }

    /// Reads the outputs again: they come and go, and change modes.
    /// Reads again how dense the shell's own panel is: outputs come and go,
    /// and change modes.
    fn refresh(&mut self) {
        self.own_ppi = self
            .inner
            .window_output(WindowId::Layer(PRIMARY_LAYER))
            .as_ref()
            .and_then(output_ppi);
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
            // Negative zones are words, not lengths: -1 is "over the others'".
            exclusive_zone: if config.exclusive_zone < 0 {
                config.exclusive_zone
            } else {
                zoom.length_out(config.exclusive_zone)
            },
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
            Event::Scale { id, scale_120 } => {
                let units = self
                    .density
                    .scale_120(self.ppi_of(WindowId::Layer(id)), scale_120);
                log(|| {
                    format!("layer {id}: compositor scale {scale_120}/120, drawn at {units}/120")
                });
                Event::Scale {
                    id,
                    scale_120: units,
                }
            }
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
            Event::TouchDown {
                surface,
                id,
                x,
                y,
                time_ms,
            } => {
                let (x, y) = point(self, surface, x, y);
                Event::TouchDown {
                    surface,
                    id,
                    x,
                    y,
                    time_ms,
                }
            }
            Event::TouchMotion {
                surface,
                id,
                x,
                y,
                time_ms,
            } => {
                let (x, y) = point(self, surface, x, y);
                Event::TouchMotion {
                    surface,
                    id,
                    x,
                    y,
                    time_ms,
                }
            }
            Event::TouchUp {
                surface,
                id,
                x,
                y,
                time_ms,
            } => {
                let (x, y) = point(self, surface, x, y);
                Event::TouchUp {
                    surface,
                    id,
                    x,
                    y,
                    time_ms,
                }
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
            // Outputs stay the compositor's: what is told of them (to a
            // configuration, as `morf.screens`) is converted where it is told,
            // and a size here that changed with the density would read as a
            // monitor that changed.
            Event::Screens(screens) => {
                self.refresh();
                Event::Screens(screens)
            }
            other => other,
        }
    }
}
