//! `Drag`: grab and move, within limits -- split panes, resize grips,
//! reorderable rows, swipe-to-dismiss cards, pull to refresh, window move.
//!
//! Settings: `mode` (`"move"`, `"resize"`, `"split"`, `"reorder"`,
//! `"swipe"`, `"transfer"`, `"confirm"` -- slide to confirm: the value runs
//! 0..1 along `extent` and only all the way across counts), `axis` (`"x"`, `"y"`, `"both"`), `threshold`
//! (px before a press becomes a drag, 6), `minimum`, `maximum` (bounds on
//! the value: px for move and resize, 0..1 for a split), `value` (where it
//! is: an offset, a size, a ratio), `extent` (the length a split ratio is
//! of), `swipe_distance` (px, 80) and `swipe_speed` (px/s, 600).
//!
//! State: `active`, `delta_x`, `delta_y`, `value`, and the base's.
//!
//! Events: the base's, `"pressed"` (x, y), `"dragged"` (x, y),
//! `"released"` (velocity x, y), `"key"` (name, modifiers). Signals:
//! `drag_started`, `dragged` (value, dx, dy), `dropped` (value),
//! `swiped` (direction) for a swipe past its distance or speed, `reorder`
//! (step: -1 or 1, for Alt+arrows or a reorder dragged past a row --
//! `extent` is a row's length then), `moved` (value) for a change by the
//! user.

use morf_value::IpcValue;

use crate::control::ControlState;
use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

pub(crate) struct Drag {
    pub(crate) base: ControlState,
    mode: String,
    axis: String,
    threshold: f64,
    minimum: f64,
    maximum: f64,
    value: f64,
    extent: f64,
    swipe_distance: f64,
    swipe_speed: f64,
    origin: Option<(f64, f64)>,
    start_value: f64,
    delta: (f64, f64),
    active: bool,
    /// Rows a reorder drag has already passed.
    reordered: i64,
    /// The press under way has swiped: a fling after it says nothing more.
    swiped: bool,
}

impl Drag {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            mode: "move".into(),
            axis: "both".into(),
            threshold: 6.0,
            minimum: f64::NEG_INFINITY,
            maximum: f64::INFINITY,
            value: 0.0,
            extent: 1.0,
            swipe_distance: 80.0,
            swipe_speed: 600.0,
            origin: None,
            start_value: 0.0,
            delta: (0.0, 0.0),
            active: false,
            reordered: 0,
            swiped: false,
        }
    }

    /// The motion along the drag's axis.
    fn along(&self, dx: f64, dy: f64) -> f64 {
        match self.axis.as_str() {
            "x" => dx,
            "y" => dy,
            _ => {
                if dx.abs() >= dy.abs() {
                    dx
                } else {
                    dy
                }
            }
        }
    }

    fn bounded(&self, v: f64) -> f64 {
        v.max(self.minimum).min(self.maximum)
    }

    fn set_value(&mut self, v: f64, effects: &mut Effects) {
        let v = self.bounded(v);
        if (v - self.value).abs() > f64::EPSILON {
            self.value = v;
            effects.set("value", v);
            effects.raise("moved", vec![v.into()]);
        }
    }
}

impl Archetype for Drag {
    fn name(&self) -> &'static str {
        "Drag"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend([
            ("active".into(), self.active.into()),
            ("delta_x".into(), self.delta.0.into()),
            ("delta_y".into(), self.delta.1.into()),
            ("value".into(), self.value.into()),
        ]);
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "pressed" => {
                effects = self.base.handle(event, arguments).unwrap_or_default();
                if self.base.enabled {
                    self.origin = Some((
                        number(arguments.first()).unwrap_or(0.0),
                        number(arguments.get(1)).unwrap_or(0.0),
                    ));
                    self.start_value = self.value;
                    self.reordered = 0;
                    self.swiped = false;
                }
            }
            "dragged" => {
                let Some((ox, oy)) = self.origin else {
                    return Ok(effects);
                };
                let (x, y) = (
                    number(arguments.first()).unwrap_or(ox),
                    number(arguments.get(1)).unwrap_or(oy),
                );
                let (mut dx, mut dy) = (x - ox, y - oy);
                if self.axis == "x" {
                    dy = 0.0;
                }
                if self.axis == "y" {
                    dx = 0.0;
                }
                if !self.active && dx.hypot(dy) < self.threshold {
                    return Ok(effects);
                }
                if !self.active {
                    self.active = true;
                    effects.set("active", true);
                    effects.raise("drag_started", Vec::new());
                }
                self.delta = (dx, dy);
                effects.set("delta_x", dx);
                effects.set("delta_y", dy);
                let moved = self.along(dx, dy);
                match self.mode.as_str() {
                    "split" => {
                        let ratio = self.start_value + moved / self.extent.max(1.0);
                        self.set_value(ratio, &mut effects);
                    }
                    "move" | "resize" => self.set_value(self.start_value + moved, &mut effects),
                    "confirm" => {
                        let v = (moved / self.extent.max(1.0)).clamp(0.0, 1.0);
                        self.set_value(v, &mut effects);
                    }
                    "reorder" => {
                        let rows = (moved / self.extent.max(1.0)).trunc() as i64;
                        while self.reordered != rows {
                            let step = (rows - self.reordered).signum();
                            self.reordered += step;
                            effects.raise("reorder", vec![step.into()]);
                        }
                    }
                    _ => {}
                }
                effects.raise("dragged", vec![self.value.into(), dx.into(), dy.into()]);
            }
            "released" | "canceled" => {
                effects = self.base.handle(event, arguments).unwrap_or_default();
                if self.active {
                    if event == "released" && self.mode == "swipe" {
                        let (vx, vy) = (
                            number(arguments.first()).unwrap_or(0.0),
                            number(arguments.get(1)).unwrap_or(0.0),
                        );
                        let moved = self.along(self.delta.0, self.delta.1);
                        let speed = self.along(vx, vy);
                        if moved.abs() >= self.swipe_distance || speed.abs() >= self.swipe_speed {
                            let forward = if moved.abs() >= self.swipe_distance {
                                moved
                            } else {
                                speed
                            } > 0.0;
                            let direction = match (self.axis.as_str(), forward) {
                                ("y", true) => "down",
                                ("y", false) => "up",
                                (_, true) => "right",
                                (_, false) => "left",
                            };
                            effects.raise("swiped", vec![direction.into()]);
                            self.swiped = true;
                        }
                    }
                    effects.raise("dropped", vec![self.value.into()]);
                    // Slid all the way: confirmed; short of it, back to the start.
                    if self.mode == "confirm" {
                        if event == "released" && self.value >= 0.95 {
                            effects.raise("confirmed", Vec::new());
                        }
                        self.set_value(0.0, &mut effects);
                    }
                }
                self.active = false;
                self.origin = None;
                self.delta = (0.0, 0.0);
                effects.set("active", false);
                effects.set("delta_x", 0.0);
                effects.set("delta_y", 0.0);
            }
            // A fling the engine saw as the press let go (its velocity): a
            // swipe that was quick if not long.
            "fling" if self.mode == "swipe" && !self.swiped => {
                let (vx, vy) = (
                    number(arguments.first()).unwrap_or(0.0),
                    number(arguments.get(1)).unwrap_or(0.0),
                );
                let speed = self.along(vx, vy);
                if speed.abs() >= self.swipe_speed {
                    let direction = match (self.axis.as_str(), speed > 0.0) {
                        ("y", true) => "down",
                        ("y", false) => "up",
                        (_, true) => "right",
                        (_, false) => "left",
                    };
                    effects.raise("swiped", vec![direction.into()]);
                    self.swiped = true;
                }
            }
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("");
                let modifiers = text(arguments.get(1)).unwrap_or("");
                // From the keyboard a slide is a held deliberate key: Return.
                if self.mode == "confirm" && matches!(name, "Return" | "KP_Enter") {
                    effects.raise("confirmed", Vec::new());
                    effects.handled = true;
                    return Ok(effects);
                }
                let back = matches!(name, "Left" | "Up");
                let forth = matches!(name, "Right" | "Down");
                if !(back || forth) {
                    return Ok(effects);
                }
                let sign = if back { -1.0 } else { 1.0 };
                match self.mode.as_str() {
                    "reorder" if modifiers.contains("alt") => {
                        effects.raise("reorder", vec![(sign as i64).into()]);
                    }
                    "split" => {
                        let v = self.value + sign * 0.05;
                        self.set_value(v, &mut effects);
                    }
                    "move" | "resize" => {
                        let v = self.value
                            + sign
                                * if modifiers.contains("shift") {
                                    1.0
                                } else {
                                    10.0
                                };
                        self.set_value(v, &mut effects);
                    }
                    _ => return Ok(effects),
                }
                effects.handled = true;
            }
            "clicked" | "key" | "fling" => {}
            _ => {
                return self
                    .base
                    .handle(event, arguments)
                    .ok_or_else(|| format!("Drag has no event `{event}`"));
            }
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let n = || expect_number(Some(value), field);
        let mut effects = Effects::default();
        match field {
            "mode" => {
                let mode = text(Some(value)).unwrap_or("move");
                if !matches!(
                    mode,
                    "move" | "resize" | "split" | "reorder" | "swipe" | "transfer" | "confirm"
                ) {
                    return Err(
                        "mode is move, resize, split, reorder, swipe, transfer or confirm".into(),
                    );
                }
                self.mode = mode.into();
            }
            "axis" => {
                let axis = text(Some(value)).unwrap_or("both");
                if !matches!(axis, "x" | "y" | "both") {
                    return Err("axis is x, y or both".into());
                }
                self.axis = axis.into();
            }
            "threshold" => self.threshold = n()?.max(0.0),
            "minimum" => self.minimum = n()?,
            "maximum" => self.maximum = n()?,
            "extent" => self.extent = n()?.max(1.0),
            "swipe_distance" => self.swipe_distance = n()?.max(1.0),
            "swipe_speed" => self.swipe_speed = n()?.max(1.0),
            "value" => {
                let v = self.bounded(n()?);
                if v != self.value {
                    self.value = v;
                    effects.set("value", v);
                }
            }
            _ => return Err(format!("Drag has no setting `{field}`")),
        }
        Ok(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(settings: &[(&str, IpcValue)]) -> Drag {
        let mut d = Drag::new();
        for (k, v) in settings {
            d.configure(k, v).unwrap();
        }
        d
    }

    #[test]
    fn a_split_follows_the_pointer_within_its_bounds() {
        let mut split = drag(&[
            ("mode", "split".into()),
            ("axis", "x".into()),
            ("extent", 400.0.into()),
            ("minimum", 0.2.into()),
            ("maximum", 0.8.into()),
            ("value", 0.5.into()),
        ]);
        split
            .handle("pressed", &[200.0.into(), 10.0.into()])
            .unwrap();
        split
            .handle("dragged", &[240.0.into(), 50.0.into()])
            .unwrap();
        assert!((split.value - 0.6).abs() < 1e-9);
        split
            .handle("dragged", &[600.0.into(), 50.0.into()])
            .unwrap();
        assert_eq!(split.value, 0.8);
        assert!(
            split
                .handle("key", &["Left".into(), "".into()])
                .unwrap()
                .handled
        );
    }

    #[test]
    fn a_short_motion_is_not_a_drag() {
        let mut d = drag(&[]);
        d.handle("pressed", &[0.0.into(), 0.0.into()]).unwrap();
        let effects = d.handle("dragged", &[3.0.into(), 0.0.into()]).unwrap();
        assert!(effects.signals.is_empty());
    }

    #[test]
    fn a_swipe_past_its_distance_says_which_way() {
        let mut card = drag(&[("mode", "swipe".into()), ("axis", "x".into())]);
        card.handle("pressed", &[0.0.into(), 0.0.into()]).unwrap();
        card.handle("dragged", &[IpcValue::Number(-120.0), 0.0.into()])
            .unwrap();
        let effects = card.handle("released", &[0.0.into(), 0.0.into()]).unwrap();
        assert!(
            effects
                .signals
                .iter()
                .any(|(n, a)| n == "swiped" && a == &vec![IpcValue::from("left")])
        );
    }

    #[test]
    fn a_reorder_says_each_row_it_passes() {
        let mut row = drag(&[
            ("mode", "reorder".into()),
            ("axis", "y".into()),
            ("extent", 40.0.into()),
        ]);
        row.handle("pressed", &[0.0.into(), 0.0.into()]).unwrap();
        let effects = row.handle("dragged", &[0.0.into(), 90.0.into()]).unwrap();
        assert_eq!(
            effects
                .signals
                .iter()
                .filter(|(n, _)| n == "reorder")
                .count(),
            2
        );
        let effects = row.handle("key", &["Up".into(), "alt".into()]).unwrap();
        assert_eq!(
            effects.signals[0],
            ("reorder".into(), vec![IpcValue::Integer(-1)])
        );
    }
}
