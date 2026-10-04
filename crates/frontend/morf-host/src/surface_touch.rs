use morf_lua::{EventPoint, FocusReason, Runtime, UiEvent};
use morf_scene::NodeHandle;
use morf_app::Event;

use crate::surfaces::*;

// Touch handling, split out of `surface_events.rs` to keep each file inside
// the repository's 500-line limit. Touch is self-contained: it owns the
// `touches` map and shares nothing with the pointer path but the hit test.

/// How far a finger may wander and still be a tap, in logical pixels.
const TAP_TRAVEL: f64 = 10.0;
/// A finger is the primary button: it presses, releases and clicks as
/// `left`, and a MouseArea that takes only another button lets it through.
const TOUCH_BUTTON: u32 = 0x110;

pub fn handle_touch_event(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    event: Event,
) -> Result<bool, String> {
    let mut repaint = false;
    match event {
        Event::TouchDown { surface, id, x, y } => {
            let Some(hit_layout) = layouts.layout_of(surface) else {
                return Ok(false);
            };
            let hit = hit_layout
                .hit_test_accepting(&runtime.scene(), x, y, &|node| {
                    runtime.accepts_pointer_button(node, TOUCH_BUTTON)
                })
                .map_err(|error| error.to_string())?;
            for root in runtime.overlay_roots() {
                if hit_layout.geometry(root).is_some() {
                    let inside: Vec<NodeHandle> = runtime
                        .overlay_nodes(root)
                        .into_iter()
                        .filter(|node| hit_layout.contains_point(&runtime.scene(), *node, x, y))
                        .collect();
                    repaint |= runtime.overlay_press(root, hit.map(|hit| hit.node), &inside);
                }
            }
            if let Some(hit) = hit {
                let point =
                    EventPoint::new((x, y), (hit.local_x, hit.local_y)).with_button(TOUCH_BUTTON);
                input.touches.insert(id, (surface, hit, x, y, 0.0));
                crate::surface_gesture::finger_down(runtime, input, layouts, id);
                if let Some(target) = runtime.click_focus_target(hit.node) {
                    input.focused.insert(surface, target);
                    let root = runtime.scene().root_of(target);
                    if let Some(root) = root {
                        repaint |= runtime.set_focus(root, Some(target), FocusReason::Click);
                    }
                }
                repaint |= runtime.dispatch_pointer(hit.node, UiEvent::Pressed, point, (0.0, 0.0));
                repaint |= runtime.dispatch_touch_event(hit.node, UiEvent::TouchPressed, id, point);
            }
        }
        Event::TouchMotion { id, x, y, .. } => {
            if let Some((touch_surface, hit, last_x, last_y, travel)) = input.touches.get_mut(&id) {
                let delta = (x - *last_x, y - *last_y);
                *travel += delta.0.abs() + delta.1.abs();
                *last_x = x;
                *last_y = y;
                let node = hit.node;
                let role = *touch_surface;
                let local = layouts
                    .layout_of(role)
                    .map(|layout| layout.local_point(&runtime.scene(), node, x, y))
                    .unwrap_or((x, y));
                let point = EventPoint::new((x, y), local);
                repaint |= runtime.dispatch_touch_event(node, UiEvent::TouchMoved, id, point);
                // A finger moving is a drag, as a held button moving is: the
                // same handler, the same deltas, so a list scrolls under a
                // finger as under a wheel.
                repaint |= runtime.dispatch_pointer(node, UiEvent::Dragged, point, delta);
                repaint |= crate::surface_gesture::finger_moved(runtime, input, layouts, id)?;
            }
        }
        Event::TouchUp { surface, id, x, y } => {
            repaint |= crate::surface_gesture::finger_up(runtime, input, Some(id));
            if let Some((touch_surface, pressed_hit, _, _, travel)) = input.touches.remove(&id) {
                let layout = layouts.layout_of(surface);
                let local = layout
                    .map(|layout| layout.local_point(&runtime.scene(), pressed_hit.node, x, y))
                    .unwrap_or((x, y));
                let point = EventPoint::new((x, y), local).with_button(TOUCH_BUTTON);
                repaint |= runtime.dispatch_touch_event(
                    pressed_hit.node,
                    UiEvent::TouchReleased,
                    id,
                    point,
                );
                repaint |= runtime.dispatch_pointer(
                    pressed_hit.node,
                    UiEvent::Released,
                    point,
                    (0.0, 0.0),
                );
                let hit = layout
                    .filter(|_| touch_surface == surface)
                    .map(|layout| {
                        layout.hit_test_accepting(&runtime.scene(), x, y, &|node| {
                            runtime.accepts_pointer_button(node, TOUCH_BUTTON)
                        })
                    })
                    .transpose()
                    .map_err(|error| error.to_string())?
                    .flatten();
                // A click is a release over the node the press landed on, so it
                // is compared by node rather than by the whole hit -- and a
                // finger that travelled was a swipe, not a tap.
                if travel > TAP_TRAVEL {
                    repaint |= runtime.dispatch_pointer(
                        pressed_hit.node,
                        UiEvent::DragFinished,
                        point,
                        (0.0, 0.0),
                    );
                } else if hit.map(|hit| hit.node) == Some(pressed_hit.node) {
                    repaint |= runtime.dispatch_pointer(
                        pressed_hit.node,
                        UiEvent::Clicked,
                        point,
                        (0.0, 0.0),
                    );
                }
            }
        }
        Event::TouchCancel => {
            repaint |= crate::surface_gesture::finger_up(runtime, input, None);
            for (id, (_, hit, x, y, _)) in input.touches.drain() {
                let point =
                    EventPoint::new((x, y), (hit.local_x, hit.local_y)).with_button(TOUCH_BUTTON);
                repaint |=
                    runtime.dispatch_touch_event(hit.node, UiEvent::TouchCanceled, id, point);
                repaint |= runtime.dispatch_pointer(hit.node, UiEvent::Released, point, (0.0, 0.0));
            }
        }
        _ => {}
    }
    Ok(repaint)
}
