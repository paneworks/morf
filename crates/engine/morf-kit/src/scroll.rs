//! `Scroll`: a scrolled view around the engine's flickable (which already
//! has momentum, overshoot and the wheel).
//!
//! Settings: `scroll_policy_x`, `scroll_policy_y` (`"auto"`, `"always"`,
//! `"never"`: whether a scroll bar shows), `snap` (`"none"`, `"items"`,
//! `"pages"`), `item_size` (for snapping to items), `step` (px an arrow
//! scrolls, 40).
//!
//! State: `content_x`, `content_y`, `content_width`, `content_height`,
//! `viewport_width`, `viewport_height`, `at_start`, `at_end`, `position_x`,
//! `position_y` (0..1 of the way along), `size_x`, `size_y` (the share of
//! the content in view: a scroll bar's handle), `bar_x`, `bar_y` (whether
//! a scroll bar shows).
//!
//! Events: the base's, `"geometry"` (content x, y, width, height, viewport
//! width, height: the Lua side reports them), `"key"` (name, modifiers),
//! `"settle"` (a flick ended: snap). Signals: `scroll_to` (x, y: the Lua
//! side moves the flickable there), `scrolled` (x, y), `reached_start`,
//! `reached_end`.

use morf_value::IpcValue;

use crate::control::ControlState;
use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

pub(crate) struct Scroll {
    pub(crate) base: ControlState,
    content: [f64; 2],
    content_size: [f64; 2],
    viewport: [f64; 2],
    policy: [String; 2],
    snap: String,
    item_size: f64,
    step: f64,
    at_start: bool,
    at_end: bool,
}

impl Scroll {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            content: [0.0; 2],
            content_size: [0.0; 2],
            viewport: [0.0; 2],
            policy: ["auto".into(), "auto".into()],
            snap: "none".into(),
            item_size: 0.0,
            step: 40.0,
            at_start: true,
            at_end: true,
        }
    }

    fn room(&self, axis: usize) -> f64 {
        (self.content_size[axis] - self.viewport[axis]).max(0.0)
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        let position = |axis: usize| {
            if self.room(axis) > 0.0 {
                (self.content[axis] / self.room(axis)).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let size = |axis: usize| {
            if self.content_size[axis] > 0.0 {
                (self.viewport[axis] / self.content_size[axis]).clamp(0.0, 1.0)
            } else {
                1.0
            }
        };
        let bar = |axis: usize| match self.policy[axis].as_str() {
            "always" => true,
            "never" => false,
            _ => self.room(axis) > 0.5,
        };
        vec![
            ("content_x".into(), self.content[0].into()),
            ("content_y".into(), self.content[1].into()),
            ("content_width".into(), self.content_size[0].into()),
            ("content_height".into(), self.content_size[1].into()),
            ("viewport_width".into(), self.viewport[0].into()),
            ("viewport_height".into(), self.viewport[1].into()),
            ("at_start".into(), self.at_start.into()),
            ("at_end".into(), self.at_end.into()),
            ("position_x".into(), position(0).into()),
            ("position_y".into(), position(1).into()),
            ("size_x".into(), size(0).into()),
            ("size_y".into(), size(1).into()),
            ("bar_x".into(), bar(0).into()),
            ("bar_y".into(), bar(1).into()),
        ]
    }

    /// The main axis: down, unless only sideways scrolls.
    fn axis(&self) -> usize {
        if self.room(1) <= 0.0 && self.room(0) > 0.0 {
            0
        } else {
            1
        }
    }

    fn scroll_to(&self, axis: usize, to: f64, effects: &mut Effects) {
        let to = to.clamp(0.0, self.room(axis));
        let mut target = self.content;
        target[axis] = to;
        effects.raise("scroll_to", vec![target[0].into(), target[1].into()]);
    }

    fn snapped(&self, axis: usize, at: f64) -> f64 {
        let unit = match self.snap.as_str() {
            "items" if self.item_size > 0.0 => self.item_size,
            "pages" if self.viewport[axis] > 0.0 => self.viewport[axis],
            _ => return at,
        };
        (at / unit).round() * unit
    }
}

impl Archetype for Scroll {
    fn name(&self) -> &'static str {
        "Scroll"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "geometry" => {
                let n = |i: usize| number(arguments.get(i)).unwrap_or(0.0);
                let before = self.content;
                self.content = [n(0), n(1)];
                self.content_size = [n(2), n(3)];
                self.viewport = [n(4), n(5)];
                let axis = self.axis();
                let start = self.content[axis] <= 0.5;
                let end = self.content[axis] >= self.room(axis) - 0.5;
                if start && !self.at_start {
                    effects.raise("reached_start", Vec::new());
                }
                if end && !self.at_end && self.room(axis) > 0.0 {
                    effects.raise("reached_end", Vec::new());
                }
                self.at_start = start;
                self.at_end = end;
                for (field, value) in self.fields() {
                    effects.set(&field, value);
                }
                if before != self.content {
                    effects.raise(
                        "scrolled",
                        vec![self.content[0].into(), self.content[1].into()],
                    );
                }
            }
            "settle" => {
                let axis = self.axis();
                let target = self.snapped(axis, self.content[axis]);
                if (target - self.content[axis]).abs() > 0.5 {
                    self.scroll_to(axis, target, &mut effects);
                }
            }
            "key" => {
                if !self.base.enabled {
                    return Ok(effects);
                }
                let name = text(arguments.first()).unwrap_or("");
                let modifiers = text(arguments.get(1)).unwrap_or("");
                let axis = self.axis();
                let page = (self.viewport[axis] * 0.9).max(self.step);
                let at = self.content[axis];
                let target = match name {
                    "Up" if axis == 1 => Some(at - self.step),
                    "Down" if axis == 1 => Some(at + self.step),
                    "Left" if axis == 0 => Some(at - self.step),
                    "Right" if axis == 0 => Some(at + self.step),
                    "Page_Up" => Some(at - page),
                    "Page_Down" => Some(at + page),
                    "space" if modifiers.contains("shift") => Some(at - page),
                    "space" => Some(at + page),
                    "Home" => Some(0.0),
                    "End" => Some(self.room(axis)),
                    _ => None,
                };
                if let Some(target) = target {
                    let target = self.snapped(axis, target.clamp(0.0, self.room(axis)));
                    if (target - at).abs() > 0.1 {
                        self.scroll_to(axis, target, &mut effects);
                        effects.handled = true;
                    }
                }
            }
            "clicked" => {}
            _ => {
                return self
                    .base
                    .handle(event, arguments)
                    .ok_or_else(|| format!("Scroll has no event `{event}`"));
            }
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let mut effects = Effects::default();
        match field {
            "scroll_policy_x" | "scroll_policy_y" => {
                let policy = text(Some(value)).unwrap_or("auto");
                if !matches!(policy, "auto" | "always" | "never") {
                    return Err("a scroll policy is auto, always or never".into());
                }
                self.policy[if field.ends_with('x') { 0 } else { 1 }] = policy.into();
                for (f, v) in self.fields() {
                    effects.set(&f, v);
                }
            }
            "snap" => {
                let snap = text(Some(value)).unwrap_or("none");
                if !matches!(snap, "none" | "items" | "pages") {
                    return Err("snap is none, items or pages".into());
                }
                self.snap = snap.into();
            }
            "item_size" => self.item_size = expect_number(Some(value), field)?.max(0.0),
            "step" => self.step = expect_number(Some(value), field)?.max(1.0),
            _ => return Err(format!("Scroll has no setting `{field}`")),
        }
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrolled(height: f64) -> Scroll {
        let mut s = Scroll::new();
        s.handle(
            "geometry",
            &[
                0.0.into(),
                0.0.into(),
                300.0.into(),
                height.into(),
                300.0.into(),
                200.0.into(),
            ],
        )
        .unwrap();
        s
    }

    #[test]
    fn keys_scroll_by_steps_and_pages() {
        let mut s = scrolled(1000.0);
        let effects = s.handle("key", &["Down".into(), "".into()]).unwrap();
        assert_eq!(
            effects.signals[0],
            ("scroll_to".into(), vec![0.0.into(), 40.0.into()])
        );
        let effects = s.handle("key", &["End".into(), "".into()]).unwrap();
        assert_eq!(
            effects.signals[0],
            ("scroll_to".into(), vec![0.0.into(), 800.0.into()])
        );
        let effects = s.handle("key", &["Page_Down".into(), "".into()]).unwrap();
        assert_eq!(effects.signals[0].1[1], 180.0.into());
    }

    #[test]
    fn a_short_page_does_not_take_keys_or_show_a_bar() {
        let mut s = scrolled(150.0);
        assert!(
            !s.handle("key", &["Down".into(), "".into()])
                .unwrap()
                .handled
        );
        assert!(s.fields().contains(&("bar_y".into(), false.into())));
    }

    #[test]
    fn pages_snap() {
        let mut s = scrolled(1000.0);
        s.configure("snap", &"pages".into()).unwrap();
        s.handle(
            "geometry",
            &[
                0.0.into(),
                130.0.into(),
                300.0.into(),
                1000.0.into(),
                300.0.into(),
                200.0.into(),
            ],
        )
        .unwrap();
        let effects = s.handle("settle", &[]).unwrap();
        assert_eq!(effects.signals[0].1[1], 200.0.into());
    }

    #[test]
    fn reaching_the_end_is_said_once() {
        let mut s = scrolled(1000.0);
        let effects = s
            .handle(
                "geometry",
                &[
                    0.0.into(),
                    800.0.into(),
                    300.0.into(),
                    1000.0.into(),
                    300.0.into(),
                    200.0.into(),
                ],
            )
            .unwrap();
        assert!(effects.signals.iter().any(|(n, _)| n == "reached_end"));
        let effects = s
            .handle(
                "geometry",
                &[
                    0.0.into(),
                    800.0.into(),
                    300.0.into(),
                    1000.0.into(),
                    300.0.into(),
                    200.0.into(),
                ],
            )
            .unwrap();
        assert!(!effects.signals.iter().any(|(n, _)| n == "reached_end"));
    }
}
