//! What each kind of pointer, wheel and touch handler is called with.

use morf_value::IpcValue;

use super::{EventPoint, held};

/// A press, release or click: `(surface_x, surface_y, local_x, local_y,
/// button, modifiers)`.
pub fn button_args(point: EventPoint) -> Vec<IpcValue> {
    point.args(held())
}

/// A motion or drag: `(surface_x, surface_y, delta_x, delta_y, local_x,
/// local_y, modifiers)`.
///
/// The displacement stays measured in surface space: it is the distance the
/// pointer has travelled since the press, and a drag is free to leave the
/// node it started on -- in which case the local pair runs past the node's
/// own bounds rather than clamping.
pub fn motion_args(point: EventPoint, delta: (f64, f64)) -> Vec<IpcValue> {
    vec![
        IpcValue::Number(point.surface_x),
        IpcValue::Number(point.surface_y),
        IpcValue::Number(delta.0),
        IpcValue::Number(delta.1),
        IpcValue::Number(point.local_x),
        IpcValue::Number(point.local_y),
        held(),
    ]
}

/// A wheel or touchpad-axis turn: `(surface_x, surface_y, pixel_x, pixel_y,
/// step_x, step_y, local_x, local_y, modifiers)`.
pub fn wheel_args(point: EventPoint, pixels: (f64, f64), steps: (i32, i32)) -> Vec<IpcValue> {
    vec![
        IpcValue::Number(point.surface_x),
        IpcValue::Number(point.surface_y),
        IpcValue::Number(pixels.0),
        IpcValue::Number(pixels.1),
        IpcValue::Integer(i64::from(steps.0)),
        IpcValue::Integer(i64::from(steps.1)),
        IpcValue::Number(point.local_x),
        IpcValue::Number(point.local_y),
        held(),
    ]
}

/// A touch contact, with its identity: `(id, surface_x, surface_y, local_x,
/// local_y)`.
pub fn touch_args(id: i32, point: EventPoint) -> Vec<IpcValue> {
    vec![
        IpcValue::Integer(i64::from(id)),
        IpcValue::Number(point.surface_x),
        IpcValue::Number(point.surface_y),
        IpcValue::Number(point.local_x),
        IpcValue::Number(point.local_y),
    ]
}
