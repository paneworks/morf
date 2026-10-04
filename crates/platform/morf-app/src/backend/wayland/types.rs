use wayland_protocols::xdg::shell::client::xdg_positioner;

use crate::{PopupAnchor, PopupConstraints, PopupGravity};

pub(crate) fn popup_anchor(anchor: PopupAnchor) -> xdg_positioner::Anchor {
    match anchor {
        PopupAnchor::None => xdg_positioner::Anchor::None,
        PopupAnchor::Top => xdg_positioner::Anchor::Top,
        PopupAnchor::Bottom => xdg_positioner::Anchor::Bottom,
        PopupAnchor::Left => xdg_positioner::Anchor::Left,
        PopupAnchor::Right => xdg_positioner::Anchor::Right,
        PopupAnchor::TopLeft => xdg_positioner::Anchor::TopLeft,
        PopupAnchor::TopRight => xdg_positioner::Anchor::TopRight,
        PopupAnchor::BottomLeft => xdg_positioner::Anchor::BottomLeft,
        PopupAnchor::BottomRight => xdg_positioner::Anchor::BottomRight,
    }
}

pub(crate) fn popup_gravity(gravity: PopupGravity) -> xdg_positioner::Gravity {
    match gravity {
        PopupGravity::None => xdg_positioner::Gravity::None,
        PopupGravity::Top => xdg_positioner::Gravity::Top,
        PopupGravity::Bottom => xdg_positioner::Gravity::Bottom,
        PopupGravity::Left => xdg_positioner::Gravity::Left,
        PopupGravity::Right => xdg_positioner::Gravity::Right,
        PopupGravity::TopLeft => xdg_positioner::Gravity::TopLeft,
        PopupGravity::TopRight => xdg_positioner::Gravity::TopRight,
        PopupGravity::BottomLeft => xdg_positioner::Gravity::BottomLeft,
        PopupGravity::BottomRight => xdg_positioner::Gravity::BottomRight,
    }
}

pub(crate) fn popup_constraints(
    constraints: PopupConstraints,
) -> xdg_positioner::ConstraintAdjustment {
    let mut value = xdg_positioner::ConstraintAdjustment::empty();
    if constraints.slide_x {
        value |= xdg_positioner::ConstraintAdjustment::SlideX;
    }
    if constraints.slide_y {
        value |= xdg_positioner::ConstraintAdjustment::SlideY;
    }
    if constraints.flip_x {
        value |= xdg_positioner::ConstraintAdjustment::FlipX;
    }
    if constraints.flip_y {
        value |= xdg_positioner::ConstraintAdjustment::FlipY;
    }
    if constraints.resize_x {
        value |= xdg_positioner::ConstraintAdjustment::ResizeX;
    }
    if constraints.resize_y {
        value |= xdg_positioner::ConstraintAdjustment::ResizeY;
    }
    value
}

