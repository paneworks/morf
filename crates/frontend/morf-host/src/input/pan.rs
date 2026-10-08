//! Continuous, exclusively owned touch drags. Handlers can decline `begin`;
//! once accepted, motion stays with that owner even as its geometry moves.
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use morf_app::WindowId;
use morf_lua::{EventPoint, IpcValue, Runtime, UiEvent};
use morf_runtime::gestures::VelocityTracker;
use morf_scene::{Element, NodeHandle};

use crate::surfaces::{PointerInput, SurfaceLayouts};

const SLOP: f64 = 8.0;
const EDGE: f64 = 20.0;

enum Owner {
    Handler(NodeHandle, Option<&'static str>),
    Scroll(NodeHandle, (f64, f64), bool),
}

pub struct TouchPan {
    finger: i32,
    surface: WindowId,
    node: NodeHandle,
    origin: (f64, f64),
    last: (f64, f64),
    velocity: VelocityTracker,
    time: Duration,
    timestamp: Option<u32>,
    owner: Option<Owner>,
    decided: bool,
    claimed: bool,
}

fn now(runtime: &Runtime) -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    runtime
        .virtual_clock()
        .unwrap_or_else(|| START.get_or_init(Instant::now).elapsed())
}

impl TouchPan {
    fn advance_time(&mut self, runtime: &Runtime, timestamp: Option<u32>) {
        self.time = match (self.timestamp, timestamp) {
            // Wayland's millisecond clock wraps roughly every 49 days.
            (Some(previous), Some(current)) => {
                self.time + Duration::from_millis(u64::from(current.wrapping_sub(previous)))
            }
            _ => now(runtime),
        };
        self.timestamp = timestamp;
    }

    fn args(&self, phase: &str, edge: Option<&str>) -> Vec<IpcValue> {
        let velocity = self.velocity.velocity(self.time);
        let mut args = Vec::new();
        if let Some(edge) = edge {
            args.push(IpcValue::String(edge.into()));
        }
        args.push(IpcValue::String(phase.into()));
        for value in [
            self.last.0 - self.origin.0,
            self.last.1 - self.origin.1,
            velocity.0,
            velocity.1,
            self.origin.0,
            self.origin.1,
        ] {
            args.push(IpcValue::Number(value));
        }
        args
    }

    fn send(&self, runtime: &mut Runtime, phase: &str) -> bool {
        match self.owner {
            Some(Owner::Handler(node, edge)) => runtime.dispatch_gesture(
                node,
                if edge.is_some() {
                    UiEvent::EdgePanned
                } else {
                    UiEvent::Panned
                },
                &self.args(phase, edge),
            ),
            _ => false,
        }
    }

    fn choose(&self, runtime: &mut Runtime, layouts: &dyn SurfaceLayouts) -> Option<Owner> {
        let layout = layouts.layout_of(self.surface)?;
        let dx = self.last.0 - self.origin.0;
        let dy = self.last.1 - self.origin.1;
        let horizontal = dx.abs() > dy.abs();
        let root = runtime.scene().root_of(self.node)?;
        let edge = layout.surface_rect(&runtime.scene(), root).and_then(|r| {
            let (x, y) = self.origin;
            // The movement axis decides corners, rather than left always winning.
            if !horizontal && y - r.y <= EDGE {
                Some("top")
            } else if !horizontal && r.y + r.height - y <= EDGE {
                Some("bottom")
            } else if horizontal && x - r.x <= EDGE {
                Some("left")
            } else if horizontal && r.x + r.width - x <= EDGE {
                Some("right")
            } else {
                None
            }
        });
        if let Some(edge) = edge
            && runtime.offer_pan(root, UiEvent::EdgePanned, &self.args("begin", Some(edge)))
        {
            return Some(Owner::Handler(root, Some(edge)));
        }
        let mut candidate = Some(self.node);
        while let Some(node) = candidate {
            if runtime.offer_pan(node, UiEvent::Panned, &self.args("begin", None)) {
                return Some(Owner::Handler(node, None));
            }
            // Direct manipulation in a child keeps ownership. A passive button
            // can give its drag to a panel while retaining an ordinary tap.
            if runtime.handles(node, UiEvent::Dragged)
                || runtime.handles(node, UiEvent::Swiped)
                || runtime.is_text_input(node)
                || runtime.is_terminal(node)
            {
                return None;
            }
            let scene = runtime.scene();
            if scene.element(node).ok() == Some(Element::Flickable)
                && runtime.takes_wheel(node)
                && let Some(((w, h), rect)) = layout
                    .content_extent(&scene, node)
                    .zip(layout.geometry(node))
            {
                let room = ((w - rect.width).max(0.0), (h - rect.height).max(0.0));
                let (offset, limit, delta) = if horizontal {
                    (scene.number(node, "content_x").unwrap_or(0.0), room.0, dx)
                } else {
                    (scene.number(node, "content_y").unwrap_or(0.0), room.1, dy)
                };
                if (delta < 0.0 && offset < limit - 0.5) || (delta > 0.0 && offset > 0.5) {
                    return Some(Owner::Scroll(node, room, horizontal));
                }
            }
            candidate = scene.parent(node).ok().flatten();
        }
        None
    }
}

pub fn down(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    id: i32,
    time_ms: Option<u32>,
) -> bool {
    if input.touches.len() != 1 {
        input.suppressed_taps.extend(input.touches.keys().copied());
        // Another finger cancels the pan, never commits it. Keep swallowing
        // the original contact until it lifts, so it cannot become a click.
        if let Some(pan) = input.pan.as_mut() {
            let changed = pan.send(runtime, "cancel");
            pan.owner = None;
            pan.decided = true;
            runtime.cancel_gesture();
            return changed;
        }
        return false;
    }
    let Some((surface, hit, x, y, _)) = input.touches.get(&id) else {
        return false;
    };
    // Catch a coasting list on DOWN, before touch slop. A tap stops the
    // list without activating the button that happened to slide under it.
    let mut stopped = false;
    let mut node = Some(hit.node);
    while let Some(current) = node {
        let mut scene = runtime.scene_mut();
        if scene.element(current).ok() == Some(Element::Flickable) {
            for property in ["content_x", "content_y"] {
                stopped |= scene.stop_animation(current, property).unwrap_or(false);
            }
        }
        node = scene.parent(current).ok().flatten();
    }
    if stopped {
        input.suppressed_taps.insert(id);
    }
    let time = now(runtime);
    let mut velocity = VelocityTracker::default();
    velocity.add(time, *x, *y);
    input.pan = Some(TouchPan {
        finger: id,
        surface: *surface,
        node: hit.node,
        origin: (*x, *y),
        last: (*x, *y),
        velocity,
        time,
        timestamp: time_ms,
        owner: None,
        decided: false,
        claimed: false,
    });
    stopped
}

/// (contact consumed, repaint). Called before legacy drag/swipe delivery.
pub fn motion(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    layouts: &dyn SurfaceLayouts,
    id: i32,
    x: f64,
    y: f64,
    time_ms: Option<u32>,
) -> (bool, bool) {
    let Some(mut pan) = input.pan.take() else {
        return (false, false);
    };
    if pan.finger != id {
        input.pan = Some(pan);
        return (false, false);
    }
    let delta = (x - pan.last.0, y - pan.last.1);
    pan.last = (x, y);
    pan.advance_time(runtime, time_ms);
    pan.velocity.add(pan.time, x, y);
    let mut changed = false;
    let mut first = false;
    if !pan.decided && (x - pan.origin.0).hypot(y - pan.origin.1) >= SLOP {
        // Wait through ambiguous diagonals until an axis is clear.
        let dx = (x - pan.origin.0).abs();
        let dy = (y - pan.origin.1).abs();
        if dx.max(dy) >= dx.min(dy) * 1.2 {
            pan.decided = true;
            pan.owner = pan.choose(runtime, layouts);
            pan.claimed = pan.owner.is_some();
            first = pan.claimed;
            if first {
                runtime.cancel_gesture();
                crate::surface_gesture::cancel_edge(input);
                let point = EventPoint::new((x, y), (0.0, 0.0)).with_button(0x110);
                changed |=
                    runtime.dispatch_touch_event(pan.node, UiEvent::TouchCanceled, id, point);
                changed |= runtime.dispatch_pointer(pan.node, UiEvent::Released, point, (0.0, 0.0));
            }
        }
    }
    match pan.owner {
        Some(Owner::Scroll(node, room, horizontal)) => {
            let (dx, dy) = if first {
                (x - pan.origin.0, y - pan.origin.1)
            } else {
                delta
            };
            changed |= runtime.scroll_flickable(
                node,
                if horizontal { (-dx, 0.0) } else { (0.0, -dy) },
                room,
            );
        }
        Some(Owner::Handler(..)) => changed |= pan.send(runtime, "update"),
        None => {}
    }
    let claimed = pan.claimed;
    input.pan = Some(pan);
    (claimed, changed)
}

pub fn up(
    runtime: &mut Runtime,
    input: &mut PointerInput,
    id: Option<i32>,
    time_ms: Option<u32>,
) -> (bool, bool) {
    let Some(mut pan) = input.pan.take() else {
        return (false, false);
    };
    if id.is_some_and(|id| id != pan.finger) {
        input.pan = Some(pan);
        return (false, false);
    }
    pan.advance_time(runtime, time_ms);
    let mut changed = pan.send(runtime, if id.is_some() { "end" } else { "cancel" });
    if let Some(Owner::Scroll(node, room, horizontal)) = pan.owner
        && id.is_some()
    {
        let (vx, vy) = pan.velocity.velocity(pan.time);
        let (property, velocity, limit) = if horizontal {
            ("content_x", -vx, room.0)
        } else {
            ("content_y", -vy, room.1)
        };
        if velocity.abs() >= 80.0 {
            changed |= runtime
                .scene_mut()
                .fling_scroll(node, property, velocity, (0.0, limit))
                .is_ok();
        }
    }
    (pan.claimed, changed)
}
