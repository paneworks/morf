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
    /// Watches one pointer event. Returns false for a click a long press
    /// took, which is then not delivered.
    pub(crate) fn watch_gesture(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
    ) -> bool {
        let mut state = self.reactive.borrow_mut();
        let now = clock(state.timers.virtual_now());
        state.gestures.watch(now, node, event, point)
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
            let now = clock(state.timers.virtual_now());
            state.gestures.finish(now, node, event, point)
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
        self.dispatch_ui_event_with_args(node, event, &args)
    }

    /// Fires a long press whose time has come. Returns whether one ran.
    pub(crate) fn poll_gestures(&mut self) -> bool {
        let due = {
            let mut state = self.reactive.borrow_mut();
            let now = clock(state.timers.virtual_now());
            let morf_runtime::Engine {
                gestures, events, ..
            } = &mut state.engine;
            gestures.take_long_press(now, |node| events.has(node, UiEvent::LongPressed))
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
        self.reactive.borrow().engine.events.has(node, event)
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
