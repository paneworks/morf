//! The pointer and the fingers: hit-tested against the layout a surface was
//! last drawn with, and dispatched to whatever they land on.
//!
//! Split from `surface_events.rs` so the lock screen can use it too. A lock
//! surface is hit-tested and handled exactly as a layer surface is; the two
//! loops differ only in how they find a surface's layout, which is what
//! [`SurfaceLayouts`] abstracts.

use morf_layout::Hit;
use morf_lua::{EventPoint, Runtime, UiEvent};
use morf_wayland::{LayerEvent, PRIMARY_LAYER, SurfaceRole};

use crate::{backdrop::*, pointer_cursor::CursorShapes, surface_touch::*, surfaces::*};

/// Routes one pointer or touch event, or hands it back when it is neither.
pub(crate) fn handle_pointer_event(
    runtime: &mut Runtime,
    client: &mut dyn CursorShapes,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    event: LayerEvent,
) -> Result<Result<bool, LayerEvent>, String> {
    let mut repaint = false;
    match event {
        LayerEvent::PointerMotion { surface, x, y } => {
            if surface == SurfaceRole::Layer(BACKDROP_LAYER) {
                client.set_cursor_shape("default");
            }
            let Some(hit_layout) = layouts.layout_of(surface) else {
                return Ok(Ok(false));
            };
            let hit = hit_layout
                .hit_test(&runtime.scene(), x, y)
                .map_err(|error| error.to_string())?;
            // Hover is compared by node, not by hit: the same node under a
            // moving pointer is still the same hover, even though its local
            // coordinates change with every motion event.
            let next_hovered = hit.map(|hit| (surface, hit));
            let entered = next_hovered.map(|(role, hit)| (role, hit.node));
            let left = input
                .hovered
                .map(|(role, hit): (SurfaceRole, Hit)| (role, hit.node));
            if entered != left {
                if let Some((_, node)) = left {
                    repaint |= runtime.dispatch_ui_event(node, UiEvent::PointerExited);
                }
                if let Some(hit) = hit {
                    repaint |= runtime.dispatch_ui_event(hit.node, UiEvent::PointerEntered);
                }
            }
            input.hovered = next_hovered;
            crate::pointer_cursor::hover_changed(runtime, client, entered, left);
            if let Some(hit) = hit {
                repaint |= runtime.dispatch_pointer(
                    hit.node,
                    UiEvent::PointerMoved,
                    EventPoint::new((x, y), (hit.local_x, hit.local_y)),
                    (0.0, 0.0),
                );
            }
            if let Some((pressed_surface, pressed_hit, start_x, start_y, dragging)) =
                &mut input.pressed
                && *pressed_surface == surface
            {
                let delta_x = x - *start_x;
                let delta_y = y - *start_y;
                // A drag that has pulled off its handle still reports where the
                // pointer is relative to that handle, so the node keeps its own
                // frame of reference for the whole gesture.
                let local = hit_layout.local_point(&runtime.scene(), pressed_hit.node, x, y);
                let point = EventPoint::new((x, y), local);
                if !*dragging && delta_x.hypot(delta_y) >= 8.0 {
                    *dragging = true;
                    repaint |= runtime.dispatch_pointer(
                        pressed_hit.node,
                        UiEvent::DragStarted,
                        point,
                        (delta_x, delta_y),
                    );
                }
                if *dragging {
                    repaint |= runtime.dispatch_pointer(
                        pressed_hit.node,
                        UiEvent::Dragged,
                        point,
                        (delta_x, delta_y),
                    );
                }
            }
        }
        LayerEvent::PointerLeave { surface } => {
            if input
                .hovered
                .is_some_and(|(hovered_surface, _)| hovered_surface == surface)
                && let Some((_, hit)) = input.hovered.take()
            {
                repaint |= runtime.dispatch_ui_event(hit.node, UiEvent::PointerExited);
            }
        }
        LayerEvent::PointerAxis {
            surface,
            x,
            y,
            horizontal,
            vertical,
            horizontal_steps,
            vertical_steps,
        } => {
            let Some(hit_layout) = layouts.layout_of(surface) else {
                return Ok(Ok(false));
            };
            // The wheel bubbles: it goes to the topmost area that would do
            // something with it — one with an `on_wheel`, or a Flickable with
            // room to scroll that way — passing over any that would not.
            let scroll_room = |node| -> Option<(f64, f64)> {
                let scene = runtime.scene();
                if scene.element(node).ok()? != morf_scene::Element::Flickable {
                    return None;
                }
                let (content_width, content_height) = hit_layout.content_extent(&scene, node)?;
                let viewport = hit_layout.geometry(node)?;
                Some((
                    (content_width - viewport.width).max(0.0),
                    (content_height - viewport.height).max(0.0),
                ))
            };
            let hit = hit_layout
                .wheel_hit_test(&runtime.scene(), x, y, &|node| {
                    runtime.takes_wheel(node)
                        && scroll_room(node).is_none_or(|(room_x, room_y)| {
                            (horizontal != 0.0 && room_x > 0.0) || (vertical != 0.0 && room_y > 0.0)
                        })
                })
                .map_err(|error| error.to_string())?;
            if let Some(hit) = hit {
                if let Some(room) = scroll_room(hit.node) {
                    repaint |= runtime.scroll_flickable(hit.node, (horizontal, vertical), room);
                } else {
                    repaint |= runtime.dispatch_wheel_event(
                        hit.node,
                        EventPoint::new((x, y), (hit.local_x, hit.local_y)),
                        (horizontal, vertical),
                        (horizontal_steps, vertical_steps),
                    );
                }
            }
        }
        LayerEvent::PointerButton {
            surface: SurfaceRole::Layer(BACKDROP_LAYER),
            pressed: true,
            ..
        } => {
            repaint |= runtime.dispatch_backdrop_click();
        }
        LayerEvent::PointerButton {
            surface,
            button,
            pressed: true,
            x,
            y,
        } => {
            let Some(hit_layout) = layouts.layout_of(surface) else {
                return Ok(Ok(false));
            };
            let hit = hit_layout
                .hit_test_accepting(&runtime.scene(), x, y, &|node| {
                    runtime.accepts_pointer_button(node, button)
                })
                .map_err(|error| error.to_string())?;
            // A compositor that has given this surface the keyboard may send
            // it every press, wherever the pointer is; one that lands on
            // nothing is the click beside the shell the backdrop exists for.
            if hit.is_none()
                && surface == SurfaceRole::Layer(PRIMARY_LAYER)
                && runtime.layer_surface_config().backdrop == Some(true)
            {
                repaint |= runtime.dispatch_backdrop_click();
            }
            input.pressed = hit.map(|hit| (surface, hit, x, y, false));
            input.pressed_button = button;
            if let Some(target) = hit.and_then(|hit| runtime.key_target_for_node(hit.node)) {
                input.focused.insert(surface, target);
                // A click on something that takes keys takes them from a text
                // input; a click on anything else leaves the field typing.
                repaint |= runtime.set_key_focus(Some(target));
            } else {
                input.focused.remove(&surface);
            }
            if let Some(hit) = hit {
                // The press carries its position now, so a handler can act on
                // where it landed without waiting for a motion event first.
                repaint |= runtime.dispatch_pointer(
                    hit.node,
                    UiEvent::Pressed,
                    EventPoint::new((x, y), (hit.local_x, hit.local_y)).with_button(button),
                    (0.0, 0.0),
                );
            }
        }
        LayerEvent::TouchDown { .. }
        | LayerEvent::TouchMotion { .. }
        | LayerEvent::TouchUp { .. }
        | LayerEvent::TouchCancel => {
            repaint |= handle_touch_event(runtime, input, layouts, event)?;
        }
        LayerEvent::PointerButton {
            surface,
            pressed: false,
            x,
            y,
            ..
        } => {
            let hit = layouts
                .layout_of(surface)
                .map(|layout| {
                    let button = input.pressed_button;
                    layout.hit_test_accepting(&runtime.scene(), x, y, &|node| {
                        runtime.accepts_pointer_button(node, button)
                    })
                })
                .transpose()
                .map_err(|error| error.to_string())?
                .flatten();
            if let Some((pressed_surface, pressed_hit, start_x, start_y, dragging)) =
                input.pressed.take()
            {
                let local = layouts
                    .layout_of(pressed_surface)
                    .map(|layout| layout.local_point(&runtime.scene(), pressed_hit.node, x, y))
                    .unwrap_or((x, y));
                let point = EventPoint::new((x, y), local);
                let clicked = point.with_button(input.pressed_button);
                repaint |= runtime.dispatch_pointer(
                    pressed_hit.node,
                    UiEvent::Released,
                    clicked,
                    (0.0, 0.0),
                );
                if dragging {
                    repaint |= runtime.dispatch_pointer(
                        pressed_hit.node,
                        UiEvent::DragFinished,
                        point,
                        (x - start_x, y - start_y),
                    );
                // A click is a release over the node the press landed on, so the
                // comparison is by node rather than by the whole hit.
                } else if pressed_surface == surface
                    && hit.map(|hit| hit.node) == Some(pressed_hit.node)
                {
                    repaint |= runtime.dispatch_pointer(
                        pressed_hit.node,
                        UiEvent::Clicked,
                        clicked,
                        (0.0, 0.0),
                    );
                }
            }
        }
        other => return Ok(Err(other)),
    }
    Ok(Ok(repaint))
}
