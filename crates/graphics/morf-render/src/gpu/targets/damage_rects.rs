//! Rectangle arithmetic for damage: scissors clamped to a target or placed
//! on an attachment, unions, intersections, and what a quad reaches.

use crate::DamageRect;

use super::super::glyphs::GlyphInstance;

pub(crate) fn clamp_scissor(
    damage: DamageRect,
    target_width: u32,
    target_height: u32,
) -> Option<(u32, u32, u32, u32)> {
    let x = damage.x.min(target_width);
    let y = damage.y.min(target_height);
    let right = damage.x.saturating_add(damage.width).min(target_width);
    let bottom = damage.y.saturating_add(damage.height).min(target_height);
    let width = right.saturating_sub(x);
    let height = bottom.saturating_sub(y);
    (width > 0 && height > 0).then_some((x, y, width, height))
}

/// The smallest rectangle holding both.
pub(crate) fn union_damage(left: DamageRect, right: DamageRect) -> DamageRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = (left.x + left.width).max(right.x + right.width);
    let bottom = (left.y + left.height).max(right.y + right.height);
    DamageRect {
        x,
        y,
        width: right_edge - x,
        height: bottom - y,
    }
}

/// A rectangle a pixel larger on every side.
pub(crate) fn grow_damage(rect: DamageRect) -> DamageRect {
    let x = rect.x.saturating_sub(1);
    let y = rect.y.saturating_sub(1);
    DamageRect {
        x,
        y,
        width: rect.x + rect.width + 1 - x,
        height: rect.y + rect.height + 1 - y,
    }
}

/// The surface pixels a textured quad covers, with a pixel of margin.
pub(crate) fn quad_reach(instance: &GlyphInstance, (width, height): (u32, u32)) -> DamageRect {
    let [x, y] = instance.origin;
    let [ax, ay, bx, by] = instance.axes;
    let corners = [
        (x, y),
        (x + ax, y + ay),
        (x + bx, y + by),
        (x + ax + bx, y + ay + by),
    ];
    let (mut left, mut top, mut right, mut bottom) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (cx, cy) in corners {
        let px = (cx + 1.0) * 0.5 * width as f32;
        let py = (1.0 - cy) * 0.5 * height as f32;
        left = left.min(px);
        top = top.min(py);
        right = right.max(px);
        bottom = bottom.max(py);
    }
    let left = (left.floor() - 1.0).max(0.0) as u32;
    let top = (top.floor() - 1.0).max(0.0) as u32;
    let right = (right.ceil() + 1.0).max(0.0) as u32;
    let bottom = (bottom.ceil() + 1.0).max(0.0) as u32;
    DamageRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

/// A scissor for `damage`, in surface pixels, on an attachment holding the
/// `frame` part of the surface at its corner.
pub(crate) fn local_scissor(damage: DamageRect, frame: DamageRect) -> Option<(u32, u32, u32, u32)> {
    let inside = intersect_damage(damage, frame)?;
    Some((
        inside.x - frame.x,
        inside.y - frame.y,
        inside.width,
        inside.height,
    ))
}

/// A scissor for `damage`, in surface pixels, on an attachment holding the
/// `frame` part of the surface at `origin`.
pub(crate) fn placed_scissor(
    damage: DamageRect,
    (frame, origin): (DamageRect, (u32, u32)),
) -> Option<(u32, u32, u32, u32)> {
    let (x, y, width, height) = local_scissor(damage, frame)?;
    Some((x + origin.0, y + origin.1, width, height))
}

pub(crate) fn intersect_damage(left: DamageRect, right: DamageRect) -> Option<DamageRect> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = left
        .x
        .saturating_add(left.width)
        .min(right.x.saturating_add(right.width));
    let bottom_edge = left
        .y
        .saturating_add(left.height)
        .min(right.y.saturating_add(right.height));
    if right_edge <= x || bottom_edge <= y {
        return None;
    }
    Some(DamageRect {
        x,
        y,
        width: right_edge - x,
        height: bottom_edge - y,
    })
}
