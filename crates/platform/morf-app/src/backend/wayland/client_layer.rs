mod layer_state;
mod regions;

use smithay_client_toolkit::compositor::FrameCallbackData;
use smithay_client_toolkit::globals::ProvidesBoundGlobal;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity as WlrKeyboardInteractivity, Layer, SurfaceKind,
};
use smithay_client_toolkit::shell::xdg::window::WindowDecorations;
use wayland_client::Proxy;
use wayland_client::protocol::{wl_output, wl_subcompositor, wl_subsurface, wl_surface};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;

use crate::backend::wayland::{state_types::*, surface_types::*};
use crate::placement::LayerRequest;

#[cfg(test)]
pub(crate) use layer_state::fallback_key_target;
use regions::note_keyboard_request;

wayland_client::delegate_noop!(LayerState: ignore wl_subcompositor::WlSubcompositor);
wayland_client::delegate_noop!(LayerState: ignore wl_subsurface::WlSubsurface);

/// Identifier of the layer surface every client creates first.
///
/// The role is plural, but one surface is still the shell's own: it is the one
/// `connect` opens, the one whose size and scale the bare accessors report, and
/// the parent an unqualified popup attaches to.
pub use crate::backend::PRIMARY_LAYER;

/// Converts configured anchor edges into the layer-shell bitmask.
///
/// `open_layer` and `set_layer_geometry` issue `set_anchor` on the same object
/// with the same meaning, so they share one conversion: two copies would let a
/// runtime update silently re-anchor a surface the creation path had pinned
/// somewhere else.
pub(crate) fn layer_anchor_mask(anchors: LayerAnchors) -> Anchor {
    let mut mask = Anchor::empty();
    if anchors.top {
        mask |= Anchor::TOP;
    }
    if anchors.right {
        mask |= Anchor::RIGHT;
    }
    if anchors.bottom {
        mask |= Anchor::BOTTOM;
    }
    if anchors.left {
        mask |= Anchor::LEFT;
    }
    mask
}

/// Converts a keyboard focus policy into its layer-shell interactivity.
pub(crate) fn layer_interactivity(focus: KeyboardFocus) -> WlrKeyboardInteractivity {
    match focus {
        KeyboardFocus::None => WlrKeyboardInteractivity::None,
        KeyboardFocus::Exclusive => WlrKeyboardInteractivity::Exclusive,
        KeyboardFocus::OnDemand => WlrKeyboardInteractivity::OnDemand,
    }
}

/// Converts a configured stacking layer into its layer-shell value.
pub(crate) fn shell_layer(layer: ShellLayer) -> Layer {
    match layer {
        ShellLayer::Background => Layer::Background,
        ShellLayer::Bottom => Layer::Bottom,
        ShellLayer::Top => Layer::Top,
        ShellLayer::Overlay => Layer::Overlay,
    }
}

/// The zwlr_layer_surface_v1 version that added `set_layer`.
const LIVE_LAYER_VERSION: u32 = 2;

impl LayerClient {
    /// Whether a mapped layer surface can move to another stacking layer in
    /// place (zwlr_layer_surface_v1 version 2's `set_layer`), rather than
    /// being destroyed and recreated there.
    ///
    /// Without layer-shell a subsurface restacks in place, so it always can.
    pub fn supports_live_layer_change(&self) -> bool {
        let Some(shell) = self.state.layer_shell.as_ref() else {
            return true;
        };
        ProvidesBoundGlobal::<zwlr_layer_shell_v1::ZwlrLayerShellV1, 1>::bound_global(shell)
            .ok()
            .is_some_and(|shell| shell.version() >= LIVE_LAYER_VERSION)
    }

    /// Resolves a configured output name against the compositor's current set.
    pub(crate) fn layer_output(
        &self,
        name: Option<&str>,
    ) -> Result<Option<wl_output::WlOutput>, WaylandError> {
        let Some(name) = name else {
            return Ok(None);
        };
        self.state
            .outputs
            .outputs()
            .find(|output| {
                self.state
                    .outputs
                    .info(output)
                    .and_then(|info| info.name)
                    .as_deref()
                    == Some(name)
            })
            .map(Some)
            .ok_or_else(|| WaylandError(format!("Wayland output `{name}` is unavailable")))
    }

    /// Creates or replaces one wlr-layer-shell surface under a client-local id.
    ///
    /// Without layer-shell the primary surface becomes a fullscreen toplevel
    /// and every other one a subsurface of it, placed where layer-shell would
    /// have put it; see `ShellSurface`.
    pub fn open_layer(&mut self, id: u64, config: LayerConfig) -> Result<(), WaylandError> {
        self.close_layer(id);
        let qh = self.queue.handle();
        let output = self.layer_output(config.output.as_deref())?;
        let surface = self.state.compositor.create_surface(&qh);
        surface.set_buffer_scale(1);
        let parent = self.state.subsurface_parent(id);
        let layer = match (&self.state.layer_shell, parent) {
            (Some(shell), _) => {
                let layer = shell.create_layer_surface(
                    &qh,
                    surface,
                    shell_layer(config.layer),
                    Some(config.namespace.clone()),
                    output.as_ref(),
                );
                layer.set_anchor(layer_anchor_mask(config.anchors));
                layer.set_keyboard_interactivity(layer_interactivity(config.keyboard_focus));
                layer.set_size(config.width, config.height);
                layer.set_margin(
                    config.margin_top,
                    config.margin_right,
                    config.margin_bottom,
                    config.margin_left,
                );
                layer.set_exclusive_zone(config.exclusive_zone);
                ShellSurface::Layer(layer)
            }
            // No layer-shell, and a fallback toplevel to hang this one from:
            // it becomes a subsurface of the primary, which covers the output,
            // and is placed inside it by its anchors, margins and size once
            // the primary knows how big it is (`arrange_subsurfaces`).
            // Desynchronised, so it paints on its own frames as a layer
            // surface would rather than waiting on the primary's commits.
            (None, Some((parent, subcompositor))) => {
                let subsurface = subcompositor.get_subsurface(&surface, &parent, &qh, ());
                subsurface.set_desync();
                ShellSurface::Subsurface(SubsurfaceShell {
                    surface,
                    subsurface,
                })
            }
            // No layer-shell: stand the primary surface up as a fullscreen
            // toplevel instead. Fullscreen is asked for explicitly instead of
            // assumed: a compositor that honours it gives the whole output,
            // which is what a shell surface covers, and one that refuses still
            // maps the window at its requested size. Its own anchors, margins
            // and exclusive zone are dropped; the layer surfaces opened after
            // it are placed inside it instead.
            (None, None) => {
                let window =
                    self.state
                        .xdg_shell
                        .create_window(surface, WindowDecorations::None, &qh);
                window.set_title(config.namespace.clone());
                window.set_app_id(config.namespace.clone());
                // Match the output selected for this worker. Without this,
                // every greeter window can land on the compositor's default
                // monitor, leaving the other monitor without its controls.
                window.set_fullscreen(output.as_ref());
                ShellSurface::Window(Box::new(window))
            }
        };
        let fractional_scale = self
            .state
            .fractional_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(layer.wl_surface(), &qh, id));
        let viewport = self
            .state
            .viewporter
            .as_ref()
            .map(|manager| manager.get_viewport(layer.wl_surface(), &qh, ()));
        // One background-effect object for the life of the surface. Asking a
        // second time is a protocol error, and the object is only a handle to
        // call `set_blur_region` on — so it is made here, once, and the paint
        // path only ever uses it. A surface that never asks for a blur has paid
        // for one small object it does not use, which is cheaper than the
        // interior mutability that creating it lazily would need.
        let backdrop = self
            .state
            .background_effect
            .as_ref()
            .map(|manager| manager.get_background_effect(layer.wl_surface(), &qh, ()));
        // A surface with no input region set accepts the pointer over the whole
        // of itself. That is the wrong default for a shell: between this commit
        // and the first paint — which is where the real region is derived from
        // live interactive geometry — the surface would silently swallow every
        // click over its own area, and a fullscreen overlay would swallow the
        // desktop. So it starts claiming nothing and the first paint opens up
        // whatever the configuration actually asked for.
        let empty = self.state.compositor.wl_compositor().create_region(&qh, ());
        layer.wl_surface().set_input_region(Some(&empty));
        empty.destroy();
        layer.commit();
        let sequence = self.state.next_layer_sequence();
        self.state.layers.insert(
            id,
            LayerRecord {
                surface: layer,
                backdrop,
                fractional_scale,
                viewport,
                width: config.width.max(1),
                height: config.height.max(1),
                scale_120: 120,
                wants_blank: false,
                blank_color: [0; 4],
                configured: false,
                blank: None,
                request: LayerRequest::from_config(&config),
                sequence,
                keyboard: std::cell::Cell::new((config.keyboard_focus, sequence)),
                placed: None,
            },
        );
        if id == PRIMARY_LAYER
            && let Some(surface) = self
                .state
                .layers
                .get(&id)
                .map(|record| record.surface.wl_surface().clone())
        {
            self.state
                .inhibit_surface_shortcuts(WindowId::Layer(id), &surface, &qh);
        }
        if self.state.layer_shell.is_none() {
            if id == PRIMARY_LAYER {
                self.state.reparent_subsurfaces(&qh);
            }
            self.state.arrange_subsurfaces();
        }
        self.connection
            .flush()
            .map_err(|error| WaylandError(format!("Wayland flush failed: {error}")))
    }

    /// Re-issues the geometry of a layer surface that is already open.
    ///
    /// wlr-layer-shell permits size, anchors, margins, exclusive zone and
    /// keyboard interactivity to change on a mapped surface, and from version
    /// 2 the stacking layer; namespace and output do not, and stay the
    /// business of [`LayerClient::open_layer`].
    /// Nothing here destroys an object, so the zwlr surface, the wl_surface, the
    /// fractional scale, the viewport and whatever renders into them all
    /// survive: the compositor answers with a configure, and the surface
    /// resizes in place instead of unmapping and coming back.
    ///
    /// Without layer-shell the same change re-places the subsurfaces standing
    /// in for layer surfaces, and one whose size moved hears a configure.
    pub fn set_layer_geometry(
        &mut self,
        id: u64,
        config: &LayerConfig,
    ) -> Result<(), WaylandError> {
        let serial = self.state.next_layer_sequence();
        let record = self
            .state
            .layers
            .get_mut(&id)
            .ok_or_else(|| WaylandError("layer surface is not open".into()))?;
        record.request = LayerRequest::from_config(config);
        note_keyboard_request(record, config.keyboard_focus, serial);
        // Nothing to re-anchor on a toplevel or a subsurface: neither has
        // anchors, margins or an exclusive zone to set. The subsurfaces are
        // placed by morf instead, from the request just recorded.
        let Some(layer) = record.surface.as_layer() else {
            if self.state.layer_shell.is_none() {
                self.state.arrange_subsurfaces();
                self.connection
                    .flush()
                    .map_err(|error| WaylandError(format!("Wayland flush failed: {error}")))?;
            }
            return Ok(());
        };
        layer.set_size(config.width, config.height);
        layer.set_anchor(layer_anchor_mask(config.anchors));
        layer.set_margin(
            config.margin_top,
            config.margin_right,
            config.margin_bottom,
            config.margin_left,
        );
        layer.set_exclusive_zone(config.exclusive_zone);
        layer.set_keyboard_interactivity(layer_interactivity(config.keyboard_focus));
        if let SurfaceKind::Wlr(wlr) = layer.kind()
            && wlr.version() >= LIVE_LAYER_VERSION
        {
            layer.set_layer(shell_layer(config.layer));
        }
        layer.commit();
        self.connection
            .flush()
            .map_err(|error| WaylandError(format!("Wayland flush failed: {error}")))
    }

    /// Asks a layer surface to map itself with a single transparent pixel.
    ///
    /// A surface that never attaches a buffer stays unmapped, and a compositor
    /// derives an output's usable area only from the layer surfaces it actually
    /// arranges — so an unmapped reserver reserves nothing at all. The protocol
    /// requires the first commit to carry no buffer and the configure that
    /// follows to be acknowledged before one may be attached, so this records
    /// the intent and the configure handler completes it.
    /// Colours a blank surface's one pixel black at `alpha`, so a backdrop
    /// can dim what it covers; the compositor stretches it over the output.
    pub fn set_layer_blank_color(&mut self, id: u64, alpha: u8) -> Result<(), WaylandError> {
        let Some(record) = self.state.layers.get_mut(&id) else {
            return Ok(());
        };
        let color = [0, 0, 0, alpha];
        if record.blank_color == color {
            return Ok(());
        }
        record.blank_color = color;
        if record.blank.is_none() {
            return Ok(());
        }
        // A new pixel: the old buffer goes, and the next attach makes one.
        record.blank = None;
        self.state.attach_blank_buffer(id);
        self.connection
            .flush()
            .map_err(|error| WaylandError(format!("Wayland flush failed: {error}")))
    }

    pub fn map_layer_blank(&mut self, id: u64) -> Result<(), WaylandError> {
        let Some(record) = self.state.layers.get_mut(&id) else {
            return Ok(());
        };
        if record.blank.is_some() {
            return Ok(());
        }
        record.wants_blank = true;
        self.state.attach_blank_buffer(id);
        self.connection
            .flush()
            .map_err(|error| WaylandError(format!("Wayland flush failed: {error}")))
    }

    /// Destroys one layer surface when it is open.
    pub fn close_layer(&mut self, id: u64) {
        self.state.release_surface_shortcuts(WindowId::Layer(id));
        let Some(record) = self.state.layers.remove(&id) else {
            return;
        };
        self.state
            .frames_outstanding
            .borrow_mut()
            .remove(&record.surface.wl_surface().id());
        let subsurface = record.surface.as_subsurface().is_some();
        drop(record);
        self.forget_surface(WindowId::Layer(id));
        // A subsurface that reserved an edge gave it back: the rest move.
        if subsurface {
            self.state.arrange_subsurfaces();
        }
    }

    /// Drops the input state pointing at a surface that has gone away.
    ///
    /// Every close path needs this and only one of the three did it, so a
    /// finger still down on a popup as it closed left a touch point addressed
    /// to a surface that no longer existed — and the next motion for that
    /// finger was delivered against it.
    pub(crate) fn forget_surface(&mut self, role: WindowId) {
        if self.state.keyboard_surface == Some(role) {
            self.state.keyboard_surface = None;
        }
        self.state
            .touch_points
            .retain(|_, (_, holder)| *holder != role);
    }

    /// Returns the wl_surface backing one layer surface.
    pub fn layer_surface(&self, id: u64) -> Option<&wl_surface::WlSurface> {
        self.state
            .layers
            .get(&id)
            .map(|layer| layer.surface.wl_surface())
    }

    /// Returns the configured logical dimensions of one layer surface.
    pub fn layer_logical_size(&self, id: u64) -> Option<(u32, u32)> {
        self.state
            .layers
            .get(&id)
            .map(|layer| (layer.width, layer.height))
    }

    /// Returns the preferred scale of one layer surface in 120ths.
    pub fn layer_scale_120(&self, id: u64) -> Option<u32> {
        self.state.layers.get(&id).map(|layer| layer.scale_120)
    }

    /// Requests a compositor callback for one layer surface's next frame.
    pub fn request_layer_frame(&self, id: u64) {
        let Some(surface) = self.layer_surface(id) else {
            return;
        };
        self.request_frame_on(surface);
    }

    /// Asks `surface` for a frame callback and notes when, unless one is
    /// already outstanding (the compositor answers every callback on the
    /// same presentation, so the older request is the one that matters).
    pub(crate) fn request_frame_on(&self, surface: &wl_surface::WlSurface) {
        let qh = self.queue.handle();
        surface.frame(&qh, FrameCallbackData(surface.clone()));
        self.state
            .frames_outstanding
            .borrow_mut()
            .entry(surface.id())
            .or_insert_with(std::time::Instant::now);
    }

    /// How long `surface` has been waiting for a frame callback, if it is.
    fn frame_wait(&self, surface: Option<&wl_surface::WlSurface>) -> Option<std::time::Duration> {
        let surface = surface?;
        self.state
            .frames_outstanding
            .borrow()
            .get(&surface.id())
            .map(std::time::Instant::elapsed)
    }

    /// How long a layer surface has waited for its frame callback, or
    /// `None` when it is not waiting. A surface the compositor does not show
    /// never gets one; painting it again then blocks a FIFO swapchain.
    pub fn layer_frame_wait(&self, id: u64) -> Option<std::time::Duration> {
        self.frame_wait(self.layer_surface(id))
    }

    /// [`Self::layer_frame_wait`] for a popup.
    pub fn popup_frame_wait(&self, id: u64) -> Option<std::time::Duration> {
        self.frame_wait(self.popup_surface(id))
    }

    /// [`Self::layer_frame_wait`] for a floating window.
    pub fn floating_frame_wait(&self, id: u64) -> Option<std::time::Duration> {
        self.frame_wait(self.floating_surface(id))
    }

    /// Commits pending state on one layer surface without attaching a buffer.
    pub fn commit_layer(&self, id: u64) {
        if let Some(surface) = self.layer_surface(id) {
            surface.commit();
        }
    }

    /// Returns an owned raw-window target for one layer surface.
    pub fn layer_window_target(&self, id: u64) -> Option<WaylandWindowTarget> {
        self.layer_surface(id).map(|surface| WaylandWindowTarget {
            backend: self.connection.backend(),
            surface: surface.clone(),
        })
    }
}
