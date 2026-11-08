//! Gestures made of ordinary presses -- a long press, a double click and a
//! swipe -- recognised by morf-runtime's `Gestures` and delivered here.
//!
//! - `on_long_pressed(x, y, local_x, local_y)`: the click its release would
//!   make is not delivered.
//! - `on_double_clicked(x, y, local_x, local_y)`: after the second click's
//!   own `on_clicked`.
//! - `on_swiped(direction, velocity_x, velocity_y)`: `direction` is
//!   `"left"`, `"right"`, `"up"` or `"down"`.

use morf_runtime::gestures::{Completed, clock};
use morf_scene::NodeHandle;

use crate::{EventPoint, IpcValue, Runtime, UiEvent};

impl Runtime {
    /// Offers a continuous gesture to a handler. Only an explicit false
    /// declines it; failures never capture a contact.
    pub fn offer_pan(&mut self, node: NodeHandle, event: UiEvent, args: &[IpcValue]) -> bool {
        use morf_runtime::events::EventHost;
        let handler = self.reactive.borrow().events().handler(node, event);
        let Some(handler) = handler else {
            return false;
        };
        match self.run_key_handler(&handler, args) {
            Ok(values) => values.first() != Some(&IpcValue::Boolean(false)),
            Err(message) => {
                self.warn(format!("{node:?}.{}: {message}", event.property()));
                false
            }
        }
    }

    /// Forget the gesture when its contact is canceled or claimed elsewhere.
    pub fn cancel_gesture(&mut self) {
        self.reactive.borrow_mut().gestures_mut().cancel();
    }

    /// Whether the press held on `node` became a long press: what is left of
    /// that contact is the node's, and a pan must not take it.
    pub fn long_pressed(&self, node: NodeHandle) -> bool {
        self.reactive.borrow().gestures().long_pressed(node)
    }

    /// While `on`, a due long press waits: a host with input queued says so
    /// before [`Runtime::poll_services`] and lets go once it has handled it,
    /// then calls [`Runtime::poll_long_press`], so a move that came before
    /// the long press's time is seen first.
    pub fn hold_long_press(&mut self, on: bool) {
        self.reactive.borrow_mut().gestures_mut().hold(on);
    }

    /// Fires a long press whose time has come. Returns whether one ran.
    pub fn poll_long_press(&mut self) -> bool {
        self.poll_gestures()
    }

    /// The gestures' clock: the virtual one under a test runner, the wall's
    /// otherwise. What a host holding a tap back times it on.
    pub fn gesture_clock(&self) -> std::time::Duration {
        clock(self.reactive.borrow().timers().virtual_now())
    }

    /// The finger of the press under way lifted, and the host holds its
    /// release back until `until` on [`Runtime::gesture_clock`] (none: no
    /// longer): the press can no longer become a long press, and the loop
    /// wakes when the tap is due.
    pub fn hold_touch(&mut self, until: Option<std::time::Duration>) {
        let mut state = self.reactive.borrow_mut();
        if until.is_some() {
            state.gestures_mut().lift();
        }
        state.gestures_mut().hold_touch(until);
    }

    /// Watches one pointer event. Returns false for a click a long press
    /// took, which is then not delivered.
    pub(crate) fn watch_gesture(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
    ) -> bool {
        let mut state = self.reactive.borrow_mut();
        let now = clock(state.timers().virtual_now());
        state.gestures_mut().watch(now, node, event, point)
    }

    /// The events a pointer event completes, delivered after it: a double
    /// click after a click, a swipe after a release.
    pub(crate) fn finish_gesture(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
    ) -> bool {
        let completed = {
            let mut state = self.reactive.borrow_mut();
            let now = clock(state.timers().virtual_now());
            state.gestures_mut().finish(now, node, event, point)
        };
        let (event, args) = match completed {
            Some(Completed::DoubleClicked(point)) => (
                UiEvent::DoubleClicked,
                point.args(crate::runtime::input::held()),
            ),
            Some(Completed::Swiped {
                direction,
                velocity_x,
                velocity_y,
            }) => (
                UiEvent::Swiped,
                vec![
                    IpcValue::String(direction.to_owned()),
                    IpcValue::Number(velocity_x),
                    IpcValue::Number(velocity_y),
                ],
            ),
            None => return false,
        };
        if event != UiEvent::Swiped {
            return self.dispatch_ui_event_with_args(node, event, &args);
        }
        // A panel can own navigation without an input overlay stealing its
        // buttons. Stop at a child that owns a swipe or a drag along this
        // axis (a slider, selection or notification); otherwise find the
        // nearest container.
        let horizontal =
            matches!(args.first(), Some(IpcValue::String(d)) if d == "left" || d == "right");
        let mut target = Some(node);
        while let Some(current) = target {
            let along = self.along_drag_axis(current, horizontal);
            if along && self.handles(current, UiEvent::Swiped) {
                return self.dispatch_ui_event_with_args(current, event, &args);
            }
            if (along && self.handles(current, UiEvent::Dragged))
                || self.is_text_input(current)
                || self.is_terminal(current)
            {
                return false;
            }
            target = self.scene().parent(current).ok().flatten();
        }
        false
    }

    /// Fires a long press whose time has come. Returns whether one ran.
    pub(crate) fn poll_gestures(&mut self) -> bool {
        let due = {
            let mut state = self.reactive.borrow_mut();
            let now = clock(state.timers().virtual_now());
            state.take_long_press(now)
        };
        match due {
            Some((node, point)) => self.dispatch_ui_event_with_args(
                node,
                UiEvent::LongPressed,
                &point.args(crate::runtime::input::held()),
            ),
            None => false,
        }
    }
}

impl Runtime {
    /// Whether `node` has a handler for `event`.
    pub fn handles(&self, node: NodeHandle, event: UiEvent) -> bool {
        self.reactive.borrow().events().has(node, event)
    }

    /// Whether a drag along this axis is `node`'s own: it has a drag or swipe
    /// handler, and its `drag_axis` is this axis or both.
    pub fn drags_along(&self, node: NodeHandle, horizontal: bool) -> bool {
        (self.handles(node, UiEvent::Dragged) || self.handles(node, UiEvent::Swiped))
            && self.along_drag_axis(node, horizontal)
    }

    fn along_drag_axis(&self, node: NodeHandle, horizontal: bool) -> bool {
        match self.scene().string_value(node, "drag_axis") {
            Ok("x") => horizontal,
            Ok("y") => !horizontal,
            _ => true,
        }
    }

    /// Calls `node`'s handler for a gesture the host recognised (a pinch, an
    /// edge swipe) with `args`. Returns whether anything changed.
    pub fn dispatch_gesture(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        args: &[IpcValue],
    ) -> bool {
        self.dispatch_ui_event_with_args(node, event, args)
    }
}
