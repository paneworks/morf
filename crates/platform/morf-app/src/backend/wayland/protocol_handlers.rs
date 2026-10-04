use crate::backend::wayland::client_layer::PRIMARY_LAYER;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::seat::keyboard::KeyEvent;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_client::protocol::{wl_output, wl_region, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{self, ZwpInputMethodV2},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use crate::backend::wayland::{helpers::*, state_types::*, surface_types::*};

/// One output, in the shape the rest of morf describes outputs in.
pub(crate) fn screen_info(info: smithay_client_toolkit::output::OutputInfo) -> Output {
    let mode = info
        .modes
        .iter()
        .find(|mode| mode.current)
        .map(|mode| mode.dimensions);
    let size = output_logical_size(info.logical_size, mode, info.transform, info.scale_factor);
    Output {
        id: info.id,
        name: info.name,
        make: info.make,
        model: info.model,
        description: info.description,
        position: Some(info.logical_position.unwrap_or(info.location)),
        size,
        physical_size: (info.physical_size.0 > 0 && info.physical_size.1 > 0)
            .then_some(info.physical_size),
        scale: info.scale_factor,
        transform: output_transform_name(info.transform),
        subpixel: output_subpixel_name(info.subpixel),
    }
}

/// xdg-output reports dimensions already transformed and scaled. Only use
/// mode pixels as a fallback, and rotate/divide those exactly once.
pub(crate) fn output_logical_size(
    logical: Option<(i32, i32)>,
    mode: Option<(i32, i32)>,
    transform: wl_output::Transform,
    scale: i32,
) -> Option<(i32, i32)> {
    if let Some(size) = logical.filter(|(w, h)| *w > 0 && *h > 0) {
        return Some(size);
    }
    let (mut width, mut height) = mode.filter(|(w, h)| *w > 0 && *h > 0)?;
    if matches!(
        transform,
        wl_output::Transform::_90
            | wl_output::Transform::_270
            | wl_output::Transform::Flipped90
            | wl_output::Transform::Flipped270
    ) {
        std::mem::swap(&mut width, &mut height);
    }
    let scale = i64::from(scale.max(1));
    Some((
        ((i64::from(width) + scale - 1) / scale) as i32,
        ((i64::from(height) + scale - 1) / scale) as i32,
    ))
}

impl LayerState {
    pub(crate) fn layer(&self) -> &ShellSurface {
        &self
            .layers
            .get(&PRIMARY_LAYER)
            .expect("layer surface is initialized before client use")
            .surface
    }

    /// Identifies which of this client's layer surfaces owns a wl_surface.
    pub(crate) fn layer_id(&self, surface: &wl_surface::WlSurface) -> Option<u64> {
        self.layers
            .iter()
            .find_map(|(id, layer)| (surface == layer.surface.wl_surface()).then_some(*id))
    }

    /// Re-reads the output list, leaving out `gone` (an output the toolkit
    /// is about to forget), and queues `Screens` when it changed.
    pub(crate) fn refresh_screens(&mut self, gone: Option<&wl_output::WlOutput>) {
        let screens = self
            .outputs
            .outputs()
            .filter(|output| Some(output) != gone)
            .filter_map(|output| self.outputs.info(&output))
            .map(screen_info)
            .collect::<Vec<_>>();
        if screens != self.screens {
            self.screens = screens.clone();
            self.events.push_back(Event::Screens(screens));
        }
    }

    pub(crate) fn surface_role(&self, surface: &wl_surface::WlSurface) -> Option<WindowId> {
        if let Some(id) = self.layer_id(surface) {
            Some(WindowId::Layer(id))
        } else if let Some(id) = self
            .popups
            .iter()
            .find_map(|(id, popup)| (surface == popup.wl_surface()).then_some(*id))
        {
            Some(WindowId::Popup(id))
        } else if let Some(id) = self
            .floatings
            .iter()
            .find_map(|(id, floating)| (surface == floating.wl_surface()).then_some(*id))
        {
            Some(WindowId::Toplevel(id))
        } else {
            // A lock surface is an input target like any other: the pointer,
            // a finger and the keyboard all arrive on it while the session is
            // locked, and until this was here every one of them was dropped.
            self.lock_surfaces
                .iter()
                .position(|lock| surface == lock.surface.wl_surface())
                .map(WindowId::Lock)
        }
    }

    /// Forgets input held by the lock surface at `index`, which has just gone,
    /// and renumbers what the ones after it held: lock surfaces are addressed
    /// by position, so every later surface has moved down one.
    pub(crate) fn forget_lock_surface(&mut self, index: usize) {
        let shift = |role: WindowId| match role {
            WindowId::Lock(at) if at == index => None,
            WindowId::Lock(at) if at > index => Some(WindowId::Lock(at - 1)),
            other => Some(other),
        };
        self.keyboard_surface = self.keyboard_surface.and_then(shift);
        self.touch_points = std::mem::take(&mut self.touch_points)
            .into_iter()
            .filter_map(|(id, (point, role))| shift(role).map(|role| (id, (point, role))))
            .collect();
    }

    /// Forgets input held by every lock surface: the lock has ended.
    pub(crate) fn forget_lock_surfaces(&mut self) {
        let is_lock = |role: &WindowId| matches!(role, WindowId::Lock(_));
        if self.keyboard_surface.as_ref().is_some_and(is_lock) {
            self.keyboard_surface = None;
        }
        self.touch_points.retain(|_, (_, role)| !is_lock(role));
    }

    pub(crate) fn push_key(&mut self, event: KeyEvent, pressed: bool, repeat: bool) {
        self.events.push_back(Event::Key {
            surface: self.key_target(),
            keysym: event.keysym.raw(),
            text: event.utf8,
            pressed,
            repeat,
            modifiers: self.modifiers,
        });
    }
}

impl Dispatch<WpFractionalScaleV1, u64> for LayerState {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        id: &u64,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let Some(layer) = state.layers.get_mut(id) else {
            return;
        };
        layer.scale_120 = scale.max(1);
        let scale_120 = layer.scale_120;
        state
            .events
            .push_back(Event::Scale { id: *id, scale_120 });
    }
}

/// The same event for a popup or a floating window.
///
/// A second impl rather than one keyed on something both could share, because
/// the identifiers do not share a space: a layer surface and a popup may both
/// be `1`, and folding them into one map would have a popup's scale change
/// resize a bar. The role carries the kind along with the number and so cannot
/// be confused.
impl Dispatch<WpFractionalScaleV1, WindowId> for LayerState {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        role: &WindowId,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let Some(entry) = state.aux_scales.get_mut(role) else {
            return;
        };
        entry.scale_120 = scale.max(1);
        let scale_120 = entry.scale_120;
        state.events.push_back(Event::AuxScale {
            role: *role,
            scale_120,
        });
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for LayerState {
    fn event(
        state: &mut Self,
        proxy: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v2::Event::Activate => {
                state.input_method_pending = InputMethodState {
                    active: true,
                    serial: state.input_method_state.serial,
                    ..InputMethodState::default()
                };
            }
            zwp_input_method_v2::Event::Deactivate => {
                state.input_method_pending.active = false;
            }
            zwp_input_method_v2::Event::SurroundingText {
                text,
                cursor,
                anchor,
            } => {
                state.input_method_pending.surrounding_text = Some(text);
                state.input_method_pending.cursor = cursor;
                state.input_method_pending.anchor = anchor;
            }
            zwp_input_method_v2::Event::Done => {
                state.input_method_pending.serial = state.input_method_state.serial.wrapping_add(1);
                state.input_method_state = state.input_method_pending.clone();
                state
                    .events
                    .push_back(Event::InputMethod(state.input_method_state.clone()));
            }
            zwp_input_method_v2::Event::Unavailable => {
                if state.input_method.as_ref() == Some(proxy) {
                    state.input_method = None;
                }
                state.input_method_state.active = false;
                state
                    .events
                    .push_back(Event::InputMethod(state.input_method_state.clone()));
                proxy.destroy();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpTextInputV3, ()> for LayerState {
    fn event(
        state: &mut Self,
        proxy: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                state.text_input_pending.focused = true;
                if state.text_input_requested {
                    proxy.enable();
                    proxy.commit();
                }
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                state.text_input_pending = TextInputState::default();
                state
                    .events
                    .push_back(Event::TextInput(state.text_input_pending.clone()));
            }
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                state.text_input_pending.preedit = text;
                state.text_input_pending.preedit_begin = cursor_begin;
                state.text_input_pending.preedit_end = cursor_end;
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                state.text_input_pending.commit = text;
            }
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                state.text_input_pending.delete_before = before_length;
                state.text_input_pending.delete_after = after_length;
            }
            zwp_text_input_v3::Event::Done { serial } => {
                state.text_input_pending.serial = serial;
                state
                    .events
                    .push_back(Event::TextInput(state.text_input_pending.clone()));
                state.text_input_pending.preedit = None;
                state.text_input_pending.commit = None;
                state.text_input_pending.delete_before = 0;
                state.text_input_pending.delete_after = 0;
            }
            _ => {}
        }
    }
}

impl ProvidesRegistryState for LayerState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

impl ShmHandler for LayerState {
    fn shm_state(&mut self) -> &mut Shm {
        self.shm
            .as_mut()
            .expect("wl_shm handler requires bound state")
    }
}

delegate_registry!(LayerState);
smithay_client_toolkit::delegate_dispatch2!(LayerState);
wayland_client::delegate_noop!(LayerState: ignore WpFractionalScaleManagerV1);
wayland_client::delegate_noop!(LayerState: ignore WpViewporter);
wayland_client::delegate_noop!(LayerState: ignore ZwpVirtualKeyboardManagerV1);
wayland_client::delegate_noop!(LayerState: ignore ZwpVirtualKeyboardV1);
wayland_client::delegate_noop!(LayerState: ignore ZwpInputMethodManagerV2);
wayland_client::delegate_noop!(LayerState: ignore ZwpTextInputManagerV3);
wayland_client::delegate_noop!(LayerState: ignore WpViewport);
wayland_client::delegate_noop!(LayerState: ignore wl_region::WlRegion);
// The per-surface object is inert: it is only ever a handle to call
// `set_blur_region` on, and it sends nothing back.
wayland_client::delegate_noop!(LayerState: ignore ExtBackgroundEffectSurfaceV1);

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for LayerState {
    /// The manager's one event: what the compositor is currently willing to do.
    ///
    /// It arrives when the manager is bound and again whenever it changes, so
    /// blur can be withdrawn while a session is running — and when it is, the
    /// compositor stops applying it even to regions that were already set.
    /// Tracking it means a configuration can be told the truth rather than
    /// being left to wonder why its panel is sharp.
    fn event(
        state: &mut Self,
        _manager: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            state.blur_capable = flags.into_result().is_ok_and(|capabilities| {
                capabilities.contains(ext_background_effect_manager_v1::Capability::Blur)
            });
        }
    }
}
