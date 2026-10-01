use morf_region::Region;

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

use crate::layer_placement::{LayerRequest, arrange, stacking, stacks_below_primary};
use crate::{state_types::*, surface_types::*, types::*};

wayland_client::delegate_noop!(LayerState: ignore wl_subcompositor::WlSubcompositor);
wayland_client::delegate_noop!(LayerState: ignore wl_subsurface::WlSubsurface);

/// Identifier of the layer surface every client creates first.
///
/// The role is plural, but one surface is still the shell's own: it is the one
/// `connect` opens, the one whose size and scale the bare accessors report, and
/// the parent an unqualified popup attaches to.
pub const PRIMARY_LAYER: u64 = 0;

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
    pub fn open_layer(&mut self, id: u64, config: BarConfig) -> Result<(), WaylandError> {
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
                .inhibit_surface_shortcuts(SurfaceRole::Layer(id), &surface, &qh);
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
    pub fn set_layer_geometry(&mut self, id: u64, config: &BarConfig) -> Result<(), WaylandError> {
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
        self.state.release_surface_shortcuts(SurfaceRole::Layer(id));
        let Some(record) = self.state.layers.remove(&id) else {
            return;
        };
        self.state
            .frames_outstanding
            .borrow_mut()
            .remove(&record.surface.wl_surface().id());
        let subsurface = record.surface.as_subsurface().is_some();
        drop(record);
        self.forget_surface(SurfaceRole::Layer(id));
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
    pub(crate) fn forget_surface(&mut self, role: SurfaceRole) {
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

    /// Tells the compositor the whole surface is opaque, or stops claiming so.
    ///
    /// A hint, and one worth giving: a compositor blends every pixel of a
    /// surface it cannot prove opaque, and a full-width bar is a few hundred
    /// thousand of them every frame. Sized to the surface's current logical
    /// size, so it has to be re-applied after a configure.
    pub fn set_layer_opaque(&self, id: u64, opaque: bool) {
        let Some(surface) = self.layer_surface(id) else {
            return;
        };
        if !opaque {
            surface.set_opaque_region(None);
            return;
        }
        let Some((width, height)) = self.layer_logical_size(id) else {
            return;
        };
        let qh = self.queue.handle();
        let region = self.state.compositor.wl_compositor().create_region(&qh, ());
        region.add(0, 0, width as i32, height as i32);
        surface.set_opaque_region(Some(&region));
        region.destroy();
    }

    /// Changes what keyboard focus one open layer surface asks for, so a
    /// shell can take the keyboard while a page of it is open and give it
    /// back after. The request rides the next frame's commit: a commit of
    /// its own here, with no buffer, made the compositor reconfigure the
    /// surface and stalled the frames the shell was in the middle of.
    /// Hyprland focuses a surface that becomes exclusive and returns focus
    /// when it stops being so. False when the surface is not a layer
    /// surface.
    ///
    /// Without layer-shell the request is recorded instead, and keys typed
    /// into the fallback toplevel go to the latest surface that asked for
    /// them (see `LayerState::key_target`).
    pub fn set_layer_keyboard_focus(&self, id: u64, focus: KeyboardFocus) -> bool {
        let Some(record) = self.state.layers.get(&id) else {
            return false;
        };
        if record.keyboard.get().0 != focus {
            note_keyboard_request(record, focus, self.state.next_layer_sequence());
        }
        let Some(layer) = record.surface.as_layer() else {
            return false;
        };
        layer.set_keyboard_interactivity(layer_interactivity(focus));
        true
    }

    /// Applies the default, empty, or rectangular input region to one surface.
    pub fn set_layer_input_region(&self, id: u64, rectangles: Option<&[InputRect]>) {
        let Some(surface) = self.layer_surface(id) else {
            return;
        };
        let Some(rectangles) = rectangles else {
            surface.set_input_region(None);
            return;
        };
        let qh = self.queue.handle();
        let region = self.state.compositor.wl_compositor().create_region(&qh, ());
        for rectangle in rectangles {
            if rectangle.width > 0 && rectangle.height > 0 {
                region.add(rectangle.x, rectangle.y, rectangle.width, rectangle.height);
            }
        }
        surface.set_input_region(Some(&region));
        region.destroy();
    }

    /// Builds and applies a composable logical input region to one surface.
    pub fn set_layer_composed_input_region(
        &self,
        id: u64,
        regions: &[Region],
    ) -> Result<(), WaylandError> {
        let (width, height) = self
            .layer_logical_size(id)
            .ok_or_else(|| WaylandError("layer surface is not open".into()))?;
        let rectangles = morf_region::build(width, height, regions)
            .map_err(|error| WaylandError(error.to_string()))?;
        self.set_layer_input_region(id, Some(&rectangles));
        Ok(())
    }
}

/// Records a keyboard-focus request on a layer surface, stamped with `serial`
/// when it changes, so the latest surface to ask can be found.
fn note_keyboard_request(record: &LayerRecord, focus: KeyboardFocus, serial: u64) {
    if record.keyboard.get().0 != focus {
        record.keyboard.set((focus, serial));
    }
}

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
    /// (`layer_placement::arrange`). One that moves gets a new position; one
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
            self.events.push_back(LayerEvent::Configure {
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
    pub(crate) fn key_target(&self) -> SurfaceRole {
        let held = self
            .keyboard_surface
            .unwrap_or(SurfaceRole::Layer(PRIMARY_LAYER));
        if self.layer_shell.is_some() || held != SurfaceRole::Layer(PRIMARY_LAYER) {
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
        fallback_key_target(&askers).map_or(held, SurfaceRole::Layer)
    }

    /// A press on a layer surface that takes the keyboard on demand makes it
    /// the latest to ask, under the layer-shell fallback — the click that
    /// would have focused it on a compositor with layer-shell.
    pub(crate) fn note_press(&self, role: SurfaceRole) {
        if self.layer_shell.is_some() {
            return;
        }
        let SurfaceRole::Layer(id) = role else {
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
