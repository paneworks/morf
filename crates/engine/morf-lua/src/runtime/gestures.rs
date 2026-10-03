//! Gestures made of ordinary presses: a long press, a double click and a
//! swipe. Every pointer and touch event already passes through
//! `Runtime::dispatch_pointer`; this watches them and adds the events they
//! make up.
//!
//! - `on_long_pressed(x, y, local_x, local_y)`: a press held within 8 px of
//!   where it began for half a second. The click its release would make is
//!   not delivered: the press was the long press's.
//! - `on_double_clicked(x, y, local_x, local_y)`: a second click on the same
//!   node within 400 ms and 8 px of the first, after that click's own
//!   `on_clicked`.
//! - `on_swiped(direction, velocity_x, velocity_y)`: a press that moved more
//!   than 24 px and was let go moving faster than 400 px/s, measured over
//!   its last 100 ms; `direction` is `"left"`, `"right"`, `"up"` or `"down"`.

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use morf_scene::NodeHandle;

use crate::state::ReactiveState;
use crate::{EventPoint, IpcValue, Runtime, UiEvent};

const LONG_PRESS: Duration = Duration::from_millis(500);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const SLOP: f64 = 8.0;
const SWIPE_DISTANCE: f64 = 24.0;
const SWIPE_SPEED: f64 = 400.0;
const SWIPE_WINDOW: Duration = Duration::from_millis(100);

/// The gestures' clock: the runtime's virtual time under `morf test`, the
/// wall's otherwise -- so a test's `advance` can hold a press long enough.
fn clock(state: &ReactiveState) -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    state
        .virtual_now
        .unwrap_or_else(|| START.get_or_init(Instant::now).elapsed())
}

/// A time on the gestures' clock as a wall instant, for the loop's wake.
fn wall(state: &ReactiveState, at: Duration) -> Instant {
    let now = clock(state);
    Instant::now() + at.saturating_sub(now)
}

struct Press {
    node: NodeHandle,
    at: Duration,
    point: EventPoint,
    samples: VecDeque<(Duration, f64, f64)>,
    moved: bool,
    long_fired: bool,
}

/// What the gestures under way remember.
#[derive(Default)]
pub(crate) struct GestureState {
    press: Option<Press>,
    last_click: Option<(NodeHandle, Duration, f64, f64)>,
    /// The node whose next click a long press took.
    swallow_click: Option<NodeHandle>,
}

fn has(state: &ReactiveState, node: NodeHandle, event: UiEvent) -> bool {
    state.handlers.contains_key(&(node, event))
}

/// When the press being held becomes a long press, if one is held where a
/// long press is wanted.
pub(crate) fn long_press_due(state: &ReactiveState) -> Option<Instant> {
    let press = state.gestures.press.as_ref()?;
    (!press.moved && !press.long_fired && has(state, press.node, UiEvent::LongPressed))
        .then(|| wall(state, press.at + LONG_PRESS))
}

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
        let now = clock(&state);
        let gestures = &mut state.gestures;
        match event {
            UiEvent::Pressed => {
                let mut samples = VecDeque::new();
                samples.push_back((now, point.surface_x, point.surface_y));
                gestures.press = Some(Press {
                    node,
                    at: now,
                    point,
                    samples,
                    moved: false,
                    long_fired: false,
                });
                gestures.swallow_click = None;
            }
            UiEvent::PointerMoved | UiEvent::DragStarted | UiEvent::Dragged => {
                if let Some(press) = gestures.press.as_mut().filter(|press| press.node == node) {
                    let (dx, dy) = (
                        point.surface_x - press.point.surface_x,
                        point.surface_y - press.point.surface_y,
                    );
                    press.moved |= dx.hypot(dy) > SLOP;
                    press
                        .samples
                        .push_back((now, point.surface_x, point.surface_y));
                    while press.samples.len() > 2
                        && press
                            .samples
                            .front()
                            .is_some_and(|(at, _, _)| now - *at > SWIPE_WINDOW)
                    {
                        press.samples.pop_front();
                    }
                }
            }
            UiEvent::Clicked => {
                if gestures.swallow_click.take() == Some(node) {
                    return false;
                }
            }
            _ => {}
        }
        true
    }

    /// The events a pointer event completes, delivered after it: a double
    /// click after a click, a swipe after a release.
    pub(crate) fn finish_gesture(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
    ) -> bool {
        let follow = {
            let mut state = self.reactive.borrow_mut();
            let now = clock(&state);
            match event {
                UiEvent::Clicked => {
                    let last = state.gestures.last_click.take();
                    let double = last.is_some_and(|(last, at, x, y)| {
                        last == node
                            && now - at <= DOUBLE_CLICK
                            && (point.surface_x - x).hypot(point.surface_y - y) <= SLOP
                    });
                    if double {
                        Some((UiEvent::DoubleClicked, point.args()))
                    } else {
                        state.gestures.last_click =
                            Some((node, now, point.surface_x, point.surface_y));
                        None
                    }
                }
                UiEvent::Released => {
                    let press = state
                        .gestures
                        .press
                        .take()
                        .filter(|press| press.node == node);
                    press.and_then(|press| {
                        let (first, last) = (press.samples.front()?, press.samples.back()?);
                        let travelled = (point.surface_x - press.point.surface_x)
                            .hypot(point.surface_y - press.point.surface_y);
                        let dt = (last.0 - first.0).as_secs_f64().max(0.008);
                        let (vx, vy) = ((last.1 - first.1) / dt, (last.2 - first.2) / dt);
                        let fresh = now - last.0 <= SWIPE_WINDOW;
                        (fresh && travelled > SWIPE_DISTANCE && vx.hypot(vy) > SWIPE_SPEED).then(
                            || {
                                let direction = if vx.abs() >= vy.abs() {
                                    if vx > 0.0 { "right" } else { "left" }
                                } else if vy > 0.0 {
                                    "down"
                                } else {
                                    "up"
                                };
                                (
                                    UiEvent::Swiped,
                                    vec![
                                        IpcValue::String(direction.to_owned()),
                                        IpcValue::Number(vx),
                                        IpcValue::Number(vy),
                                    ],
                                )
                            },
                        )
                    })
                }
                _ => None,
            }
        };
        match follow {
            Some((event, args)) => self.dispatch_ui_event_with_args(node, event, &args),
            None => false,
        }
    }

    /// Fires a long press whose time has come. Returns whether one ran.
    pub(crate) fn poll_gestures(&mut self) -> bool {
        let due = {
            let mut state = self.reactive.borrow_mut();
            let now = clock(&state);
            let ready = state.gestures.press.as_ref().is_some_and(|press| {
                !press.moved
                    && !press.long_fired
                    && has(&state, press.node, UiEvent::LongPressed)
                    && now >= press.at + LONG_PRESS
            });
            match state.gestures.press.as_mut().filter(|_| ready) {
                Some(press) => {
                    press.long_fired = true;
                    let (node, point) = (press.node, press.point);
                    state.gestures.swallow_click = Some(node);
                    Some((node, point))
                }
                None => None,
            }
        };
        match due {
            Some((node, point)) => {
                self.dispatch_ui_event_with_args(node, UiEvent::LongPressed, &point.args())
            }
            None => false,
        }
    }
}

impl Runtime {
    /// Whether `node` has a handler for `event`.
    pub fn handles(&self, node: NodeHandle, event: UiEvent) -> bool {
        has(&self.reactive.borrow(), node, event)
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
