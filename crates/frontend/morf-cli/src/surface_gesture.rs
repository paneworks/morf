//! Gestures that need more than one finger, or the surface's edge: a pinch,
//! a two-finger scroll and an edge swipe. Each finger still presses, drags
//! and releases the node under it as usual; these are told about the same
//! touches and add their own events.
//!
//! - `on_pinched(scale, phase, x, y)` on the nearest node, under the first
//!   finger, that has it: `scale` against the fingers' first spread, `phase`
//!   `"update"` while they move and `"end"` when one lifts, `(x, y)` their
//!   midpoint on the surface.
//! - Two fingers moving together where nothing takes a pinch scroll what is
//!   under their midpoint, as a touchpad's two fingers do.
//! - `on_edge_swiped(edge)` on a surface's root: a finger that lands within
//!   20 px of an edge and moves 48 px inward; `edge` is `"left"`, `"right"`,
//!   `"top"` or `"bottom"`.

use morf_layout::Hit;
use morf_lua::{Runtime, UiEvent};
use morf_value::IpcValue;
use morf_scene::NodeHandle;
use morf_wayland::SurfaceRole;

use crate::surfaces::{PointerInput, SurfaceLayouts};

const EDGE: f64 = 20.0;
const EDGE_TRAVEL: f64 = 48.0;

struct Pinch {
    surface: SurfaceRole,
    fingers: (i32, i32),
    target: Option<NodeHandle>,
    spread: f64,
    midpoint: (f64, f64),
    scale: f64,
}

struct EdgeSwipe {
    finger: i32,
    root: NodeHandle,
    edge: &'static str,
    origin: (f64, f64),
}

/// The multi-finger and edge gestures under way.
#[derive(Default)]
pub(crate) struct TouchGestures {
    pinch: Option<Pinch>,
    edge: Option<EdgeSwipe>,
}

fn finger(input: &PointerInput, id: i32) -> Option<(SurfaceRole, Hit, f64, f64)> {
    input
        .touches
        .get(&id)
        .map(|(surface, hit, x, y, _)| (*surface, *hit, *x, *y))
}

fn spread(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1).max(1.0)
}

fn midpoint(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

/// A finger came down (already in `input.touches`).
pub(crate) fn finger_down(
    runtime: &Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    id: i32,
) {
    let Some((surface, hit, x, y)) = finger(input, id) else {
        return;
    };
    if input.touches.len() == 1 {
        input.gestures.edge = None;
        let root = runtime.scene().root_of(hit.node);
        let rect = root.and_then(|root| {
            layouts
                .layout_of(surface)?
                .surface_rect(&runtime.scene(), root)
                .map(|rect| (root, rect))
        });
        if let Some((root, rect)) =
            rect.filter(|(root, _)| runtime.handles(*root, UiEvent::EdgeSwiped))
        {
            let edge = if x - rect.x <= EDGE {
                Some("left")
            } else if rect.x + rect.width - x <= EDGE {
                Some("right")
            } else if y - rect.y <= EDGE {
                Some("top")
            } else if rect.y + rect.height - y <= EDGE {
                Some("bottom")
            } else {
                None
            };
            input.gestures.edge = edge.map(|edge| EdgeSwipe {
                finger: id,
                root,
                edge,
                origin: (x, y),
            });
        }
        return;
    }
    // A second finger on the same surface starts a pinch; a third changes
    // nothing.
    let other = input.touches.keys().copied().find(|other| *other != id);
    let (Some(other), None) = (other, input.gestures.pinch.as_ref()) else {
        return;
    };
    let Some((other_surface, other_hit, ox, oy)) = finger(input, other) else {
        return;
    };
    if other_surface != surface || input.touches.len() != 2 {
        return;
    }
    let scene = runtime.scene();
    let mut target = Some(other_hit.node);
    while let Some(node) = target {
        if runtime.handles(node, UiEvent::Pinched) {
            break;
        }
        target = scene.parent(node).ok().flatten();
    }
    input.gestures.edge = None;
    input.gestures.pinch = Some(Pinch {
        surface,
        fingers: (other, id),
        target,
        spread: spread((ox, oy), (x, y)),
        midpoint: midpoint((ox, oy), (x, y)),
        scale: 1.0,
    });
}

/// A finger moved (its new place already in `input.touches`). Returns
/// whether anything changed.
pub(crate) fn finger_moved(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    id: i32,
) -> Result<bool, String> {
    let mut changed = false;
    if let Some(edge) = input
        .gestures
        .edge
        .as_ref()
        .filter(|edge| edge.finger == id)
        && let Some((_, _, x, y)) = finger(input, id)
    {
        let inward = match edge.edge {
            "left" => x - edge.origin.0,
            "right" => edge.origin.0 - x,
            "top" => y - edge.origin.1,
            _ => edge.origin.1 - y,
        };
        if inward >= EDGE_TRAVEL {
            let (root, name) = (edge.root, edge.edge);
            input.gestures.edge = None;
            changed |= runtime.dispatch_gesture(
                root,
                UiEvent::EdgeSwiped,
                &[IpcValue::String(name.to_owned())],
            );
        }
    }
    let Some(pinch) = input
        .gestures
        .pinch
        .as_mut()
        .filter(|p| p.fingers.0 == id || p.fingers.1 == id)
    else {
        return Ok(changed);
    };
    let (Some(a), Some(b)) = (
        input.touches.get(&pinch.fingers.0).map(|t| (t.2, t.3)),
        input.touches.get(&pinch.fingers.1).map(|t| (t.2, t.3)),
    ) else {
        return Ok(changed);
    };
    let mid = midpoint(a, b);
    let moved = (mid.0 - pinch.midpoint.0, mid.1 - pinch.midpoint.1);
    pinch.midpoint = mid;
    pinch.scale = spread(a, b) / pinch.spread;
    match pinch.target {
        Some(target) => {
            let args = [
                IpcValue::Number(pinch.scale),
                IpcValue::String("update".to_owned()),
                IpcValue::Number(mid.0),
                IpcValue::Number(mid.1),
            ];
            changed |= runtime.dispatch_gesture(target, UiEvent::Pinched, &args);
        }
        None => {
            // Content follows the fingers: moving up scrolls down.
            if let Some(layout) = layouts.layout_of(pinch.surface) {
                changed |= crate::surface_pointer::wheel_at(
                    runtime,
                    layout,
                    mid,
                    (-moved.0, -moved.1),
                    (0, 0),
                )?;
            }
        }
    }
    Ok(changed)
}

/// A finger lifted, or every finger was cancelled (`None`). Returns whether
/// anything changed.
pub(crate) fn finger_up(runtime: &mut Runtime, input: &mut PointerInput, id: Option<i32>) -> bool {
    if input
        .gestures
        .edge
        .as_ref()
        .is_some_and(|edge| id.is_none_or(|id| edge.finger == id))
    {
        input.gestures.edge = None;
    }
    let ends = input
        .gestures
        .pinch
        .as_ref()
        .is_some_and(|p| id.is_none_or(|id| p.fingers.0 == id || p.fingers.1 == id));
    if !ends {
        return false;
    }
    let pinch = input.gestures.pinch.take().expect("checked above");
    match pinch.target {
        Some(target) => runtime.dispatch_gesture(
            target,
            UiEvent::Pinched,
            &[
                IpcValue::Number(pinch.scale),
                IpcValue::String("end".to_owned()),
                IpcValue::Number(pinch.midpoint.0),
                IpcValue::Number(pinch.midpoint.1),
            ],
        ),
        None => false,
    }
}
