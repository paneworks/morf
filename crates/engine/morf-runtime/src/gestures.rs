//! Gestures made of ordinary presses: a long press, a double click and a
//! swipe. Every pointer and touch event is shown to [`Gestures`], which says
//! what they add up to; the caller delivers it.
//!
//! - long press: a press held within 8 px of where it began for half a
//!   second. The click its release would make is not delivered: the press
//!   was the long press's.
//! - double click: a second click on the same node within 400 ms and 8 px
//!   of the first, after that click's own click.
//! - swipe: a press that moved more than 24 px and was let go moving faster
//!   than 400 px/s, measured over its last 100 ms, in one of four
//!   directions.

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use morf_scene::NodeHandle;

use crate::events::{EventPoint, UiEvent};

const LONG_PRESS: Duration = Duration::from_millis(500);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const SLOP: f64 = 8.0;
const SWIPE_DISTANCE: f64 = 24.0;
const SWIPE_SPEED: f64 = 400.0;
const SWIPE_WINDOW: Duration = Duration::from_millis(100);

/// The gestures' clock: the virtual clock when there is one (under a test
/// runner), the wall's otherwise -- so a test's `advance` can hold a press
/// long enough.
pub fn clock(virtual_now: Option<Duration>) -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    virtual_now.unwrap_or_else(|| START.get_or_init(Instant::now).elapsed())
}

/// A time on the gestures' clock as a wall instant, for the loop's wake.
pub fn wall(virtual_now: Option<Duration>, at: Duration) -> Instant {
    let now = clock(virtual_now);
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

/// What a pointer event completed, to be delivered after it.
#[derive(Clone, Debug, PartialEq)]
pub enum Completed {
    DoubleClicked(EventPoint),
    /// `direction` is `"left"`, `"right"`, `"up"` or `"down"`.
    Swiped {
        direction: &'static str,
        velocity_x: f64,
        velocity_y: f64,
    },
}

/// What the gestures under way remember.
#[derive(Default)]
pub struct Gestures {
    press: Option<Press>,
    last_click: Option<(NodeHandle, Duration, f64, f64)>,
    /// The node whose next click a long press took.
    swallow_click: Option<NodeHandle>,
}

impl Gestures {
    /// When the press being held becomes a long press (on the gestures'
    /// clock), if one is held on a node that `wants_long_press`.
    pub fn long_press_due(
        &self,
        wants_long_press: impl Fn(NodeHandle) -> bool,
    ) -> Option<Duration> {
        let press = self.press.as_ref()?;
        (!press.moved && !press.long_fired && wants_long_press(press.node))
            .then_some(press.at + LONG_PRESS)
    }

    /// Watches one pointer event at `now`. Returns false for a click a long
    /// press took, which is then not to be delivered.
    pub fn watch(
        &mut self,
        now: Duration,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
    ) -> bool {
        match event {
            UiEvent::Pressed => {
                let mut samples = VecDeque::new();
                samples.push_back((now, point.surface_x, point.surface_y));
                self.press = Some(Press {
                    node,
                    at: now,
                    point,
                    samples,
                    moved: false,
                    long_fired: false,
                });
                self.swallow_click = None;
            }
            UiEvent::PointerMoved | UiEvent::DragStarted | UiEvent::Dragged => {
                if let Some(press) = self.press.as_mut().filter(|press| press.node == node) {
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
                if self.swallow_click.take() == Some(node) {
                    return false;
                }
            }
            _ => {}
        }
        true
    }

    /// What a pointer event at `now` completes: a double click after a
    /// click, a swipe after a release.
    pub fn finish(
        &mut self,
        now: Duration,
        node: NodeHandle,
        event: UiEvent,
        point: EventPoint,
    ) -> Option<Completed> {
        match event {
            UiEvent::Clicked => {
                let last = self.last_click.take();
                let double = last.is_some_and(|(last, at, x, y)| {
                    last == node
                        && now - at <= DOUBLE_CLICK
                        && (point.surface_x - x).hypot(point.surface_y - y) <= SLOP
                });
                if double {
                    Some(Completed::DoubleClicked(point))
                } else {
                    self.last_click = Some((node, now, point.surface_x, point.surface_y));
                    None
                }
            }
            UiEvent::Released => {
                let press = self.press.take().filter(|press| press.node == node)?;
                let (first, last) = (press.samples.front()?, press.samples.back()?);
                let travelled = (point.surface_x - press.point.surface_x)
                    .hypot(point.surface_y - press.point.surface_y);
                let dt = (last.0 - first.0).as_secs_f64().max(0.008);
                let (vx, vy) = ((last.1 - first.1) / dt, (last.2 - first.2) / dt);
                let fresh = now - last.0 <= SWIPE_WINDOW;
                (fresh && travelled > SWIPE_DISTANCE && vx.hypot(vy) > SWIPE_SPEED).then(|| {
                    let direction = if vx.abs() >= vy.abs() {
                        if vx > 0.0 { "right" } else { "left" }
                    } else if vy > 0.0 {
                        "down"
                    } else {
                        "up"
                    };
                    Completed::Swiped {
                        direction,
                        velocity_x: vx,
                        velocity_y: vy,
                    }
                })
            }
            _ => None,
        }
    }

    /// The long press whose time came by `now`, on a node that
    /// `wants_long_press`: its node and where it was pressed. Its release's
    /// click will be swallowed.
    pub fn take_long_press(
        &mut self,
        now: Duration,
        wants_long_press: impl Fn(NodeHandle) -> bool,
    ) -> Option<(NodeHandle, EventPoint)> {
        let press = self.press.as_mut()?;
        if press.moved
            || press.long_fired
            || !wants_long_press(press.node)
            || now < press.at + LONG_PRESS
        {
            return None;
        }
        press.long_fired = true;
        self.swallow_click = Some(press.node);
        Some((press.node, press.point))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> NodeHandle {
        let mut scene = morf_scene::Scene::default();
        scene.create(morf_scene::Element::MouseArea)
    }

    fn at(x: f64, y: f64) -> EventPoint {
        EventPoint::new((x, y), (x, y))
    }

    #[test]
    fn a_held_press_becomes_a_long_press_and_takes_its_click() {
        let mut gestures = Gestures::default();
        let area = node();
        let ms = Duration::from_millis;
        gestures.watch(ms(0), area, UiEvent::Pressed, at(10.0, 10.0));
        assert_eq!(gestures.long_press_due(|_| true), Some(ms(500)));
        assert!(gestures.take_long_press(ms(499), |_| true).is_none());
        assert!(gestures.take_long_press(ms(500), |_| true).is_some());
        assert!(
            gestures.take_long_press(ms(600), |_| true).is_none(),
            "once"
        );
        assert!(!gestures.watch(ms(700), area, UiEvent::Clicked, at(10.0, 10.0)));
    }

    #[test]
    fn two_quick_clicks_make_a_double_click_and_a_fast_drag_a_swipe() {
        let mut gestures = Gestures::default();
        let area = node();
        let ms = Duration::from_millis;
        assert_eq!(
            gestures.finish(ms(0), area, UiEvent::Clicked, at(5.0, 5.0)),
            None
        );
        assert!(matches!(
            gestures.finish(ms(300), area, UiEvent::Clicked, at(6.0, 5.0)),
            Some(Completed::DoubleClicked(_))
        ));
        gestures.watch(ms(1000), area, UiEvent::Pressed, at(0.0, 0.0));
        gestures.watch(ms(1050), area, UiEvent::Dragged, at(60.0, 0.0));
        assert!(matches!(
            gestures.finish(ms(1060), area, UiEvent::Released, at(60.0, 0.0)),
            Some(Completed::Swiped {
                direction: "right",
                ..
            })
        ));
    }
}
