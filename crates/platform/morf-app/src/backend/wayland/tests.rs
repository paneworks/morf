use crate::backend::wayland::client_layer::layer_interactivity;
use crate::backend::wayland::client_surface::next_reposition_token;
use smithay_client_toolkit::shell::wlr_layer::Anchor;
use smithay_client_toolkit::shell::wlr_layer::KeyboardInteractivity as WlrKeyboardInteractivity;
use std::collections::HashMap;

use crate::backend::wayland::client_layer::{PRIMARY_LAYER, layer_anchor_mask};
use wayland_client::protocol::wl_output;

use super::*;

#[test]
fn monitor_sizes_use_logical_pixels_and_only_transform_mode_fallbacks() {
    use crate::backend::wayland::protocol_handlers::output_logical_size as size;
    use wl_output::Transform::*;
    // Fractional scaling is reflected in xdg-output's logical size, which
    // must not be divided by the integer buffer scale a second time.
    assert_eq!(
        size(Some((2560, 1440)), Some((3840, 2160)), Normal, 2),
        Some((2560, 1440))
    );
    assert_eq!(
        size(Some((1080, 1920)), Some((3840, 2160)), _90, 2),
        Some((1080, 1920))
    );
    assert_eq!(
        size(None, Some((3840, 2160)), Normal, 2),
        Some((1920, 1080))
    );
    assert_eq!(size(None, Some((3840, 2160)), _90, 2), Some((1080, 1920)));
    assert_eq!(
        size(Some((0, 0)), Some((1920, 1080)), Flipped270, 1),
        Some((1080, 1920))
    );
    assert_eq!(size(None, None, Normal, 1), None);
}

#[test]
fn physical_size_rounds_fractional_scale_upward() {
    assert_eq!(physical_size((101, 31), 150), (127, 39));
}

#[test]
fn output_transforms_have_stable_public_names() {
    assert_eq!(
        output_transform_name(wl_output::Transform::Normal),
        "normal"
    );
    assert_eq!(output_transform_name(wl_output::Transform::_90), "90");
    assert_eq!(
        output_transform_name(wl_output::Transform::Flipped270),
        "flipped_270"
    );
}

#[test]
fn popup_defaults_preserve_general_constraint_policy() {
    let popup = PopupConfig::default();
    assert_eq!(popup.anchor_edge, PopupAnchor::BottomLeft);
    assert_eq!(popup.gravity, PopupGravity::BottomRight);
    assert!(popup.constraints.slide_x);
    assert!(popup.constraints.slide_y);
    assert!(popup.constraints.flip_x);
    assert!(popup.constraints.flip_y);
    assert!(!popup.constraints.resize_x);
    assert!(!popup.constraints.resize_y);
}

#[test]
fn default_virtual_keymap_round_trips() {
    let keymap = default_keymap().unwrap();
    let context = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
    assert!(
        xkbcommon::xkb::Keymap::new_from_string(
            &context,
            keymap,
            xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
            xkbcommon::xkb::COMPILE_NO_FLAGS,
        )
        .is_some()
    );
}

#[test]
fn layer_roles_are_distinct_per_surface_identifier() {
    assert_eq!(PRIMARY_LAYER, 0);
    assert_ne!(WindowId::Layer(0), WindowId::Layer(1));
    assert_ne!(WindowId::Layer(1), WindowId::Popup(1));
    let events = [
        Event::Configure {
            id: 3,
            width: 8,
            height: 4,
        },
        Event::Scale {
            id: 3,
            scale_120: 180,
        },
        Event::Frame { id: 3, time_ms: 1 },
        Event::Closed { id: 3 },
    ];
    assert!(events.iter().all(|event| match event {
        Event::Configure { id, .. }
        | Event::Scale { id, .. }
        | Event::Frame { id, .. }
        | Event::Closed { id } => *id == 3,
        _ => false,
    }));
}

#[test]
fn one_anchor_conversion_serves_creation_and_reconfiguration() {
    assert_eq!(
        layer_anchor_mask(LayerAnchors {
            top: true,
            right: true,
            bottom: false,
            left: true,
        }),
        Anchor::TOP | Anchor::RIGHT | Anchor::LEFT
    );
    assert_eq!(
        layer_anchor_mask(LayerAnchors {
            top: false,
            right: false,
            bottom: true,
            left: false,
        }),
        Anchor::BOTTOM
    );
    assert_eq!(
        layer_anchor_mask(LayerAnchors {
            top: false,
            right: false,
            bottom: false,
            left: false,
        }),
        Anchor::empty()
    );
    assert_eq!(
        layer_interactivity(KeyboardFocus::Exclusive),
        WlrKeyboardInteractivity::Exclusive
    );
    assert_eq!(
        layer_interactivity(KeyboardFocus::None),
        WlrKeyboardInteractivity::None
    );
}

#[test]
fn reposition_tokens_count_per_popup_and_record_the_echo() {
    let mut repositions = HashMap::new();

    assert_eq!(next_reposition_token(&mut repositions, 4), 1);
    assert_eq!(next_reposition_token(&mut repositions, 4), 2);
    // A second popup counts on its own, so an echo identifies one request.
    assert_eq!(next_reposition_token(&mut repositions, 9), 1);

    // Zero stays unused: a wrapped counter must not look like a fresh one.
    repositions.get_mut(&4).unwrap().sent = u32::MAX;
    assert_eq!(next_reposition_token(&mut repositions, 4), 1);
}

#[test]
fn fallback_keys_go_to_the_latest_surface_asking_for_them() {
    use crate::backend::wayland::client_layer::fallback_key_target;
    use KeyboardFocus::{Exclusive, None as NoFocus, OnDemand};
    // Nobody asks: keys stay where the compositor sent them.
    assert_eq!(
        fallback_key_target(&[(0, NoFocus, 1), (5, NoFocus, 9)]),
        None
    );
    // The latest on-demand asker wins over an older one and the primary.
    assert_eq!(
        fallback_key_target(&[(0, OnDemand, 1), (5, OnDemand, 7), (6, OnDemand, 3)]),
        Some(5)
    );
    // An exclusive asker wins over a later on-demand one.
    assert_eq!(
        fallback_key_target(&[(0, Exclusive, 2), (5, OnDemand, 9)]),
        Some(0)
    );
    // And the latest of two exclusive ones wins.
    assert_eq!(
        fallback_key_target(&[(0, Exclusive, 2), (5, Exclusive, 4), (6, NoFocus, 8)]),
        Some(5)
    );
}
