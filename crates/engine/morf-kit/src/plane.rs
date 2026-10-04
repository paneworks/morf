//! `Plane`: a value in two dimensions -- a colour plane, an XY pad, a
//! joystick, a crop handle.
//!
//! Settings: `x_from` (0), `x_to` (1), `y_from` (0), `y_to` (1), `x`, `y`,
//! `step_x`, `step_y` (0: continuous), `constraint` (`"free"`, `"circle"`
//! -- inside the inscribed circle --, `"square"`), `y_up` (y grows upward,
//! as on a chart; down, as on a screen, by default), `spring` (a
//! joystick: let go, it returns to `rest_x`, `rest_y` -- the middle by
//! default), `polar` (a hue wheel: x is the angle round the centre, 0..1
//! clockwise from twelve o'clock, y the distance out, 0 at the centre and
//! 1 at the rim).
//!
//! State: `x`, `y`, `position_x`, `position_y` (0..1), `visual_x`,
//! `visual_y` (where to draw it, 0..1 across and down the box: right to
//! left and `y_up` applied, a polar value placed on its circle),
//! `dragging`.
//!
//! Events: the base's, `"pressed"` and `"dragged"` (local x, y, width,
//! height), `"released"`, `"wheel"` (steps x, y), `"key"` (name,
//! modifiers). Signals: `moved` (x, y) for a change by the user,
//! `value_changed` (x, y) for any.

use morf_value::IpcValue;

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Constraint {
    Free,
    Circle,
    Square,
}

pub(crate) struct Plane {
    pub(crate) base: ControlState,
    from: [f64; 2],
    to: [f64; 2],
    value: [f64; 2],
    step: [f64; 2],
    constraint: Constraint,
    y_up: bool,
    dragging: bool,
    spring: bool,
    rest: Option<[f64; 2]>,
    polar: bool,
}

impl Plane {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            from: [0.0, 0.0],
            to: [1.0, 1.0],
            value: [0.0, 0.0],
            step: [0.0, 0.0],
            constraint: Constraint::Free,
            y_up: false,
            dragging: false,
            spring: false,
            rest: None,
            polar: false,
        }
    }

    fn position(&self, axis: usize) -> f64 {
        let span = self.to[axis] - self.from[axis];
        if span == 0.0 {
            0.0
        } else {
            ((self.value[axis] - self.from[axis]) / span).clamp(0.0, 1.0)
        }
    }

    fn visual(&self, axis: usize) -> f64 {
        if self.polar {
            let angle = self.position(0) * std::f64::consts::TAU;
            let r = self.position(1) * 0.5;
            let angle = if self.base.mirrored { -angle } else { angle };
            return if axis == 0 {
                0.5 + r * angle.sin()
            } else {
                0.5 - r * angle.cos()
            };
        }
        let p = self.position(axis);
        let flip = if axis == 0 {
            self.base.mirrored
        } else {
            self.y_up
        };
        if flip { 1.0 - p } else { p }
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        vec![
            ("x".into(), self.value[0].into()),
            ("y".into(), self.value[1].into()),
            ("position_x".into(), self.position(0).into()),
            ("position_y".into(), self.position(1).into()),
            ("visual_x".into(), self.visual(0).into()),
            ("visual_y".into(), self.visual(1).into()),
            ("dragging".into(), self.dragging.into()),
        ]
    }

    fn snapped(&self, axis: usize, v: f64) -> f64 {
        let step = self.step[axis];
        let (low, high) = (
            self.from[axis].min(self.to[axis]),
            self.from[axis].max(self.to[axis]),
        );
        let v = if step > 0.0 {
            self.from[axis] + ((v - self.from[axis]) / step).round() * step
        } else {
            v
        };
        v.clamp(low, high)
    }

    /// Positions (0..1) kept inside the constraint.
    fn constrained(&self, mut p: [f64; 2]) -> [f64; 2] {
        match self.constraint {
            Constraint::Free | Constraint::Square => {}
            Constraint::Circle => {
                let (dx, dy) = (p[0] - 0.5, p[1] - 0.5);
                let r = dx.hypot(dy);
                if r > 0.5 {
                    p = [0.5 + dx / r * 0.5, 0.5 + dy / r * 0.5];
                }
            }
        }
        p
    }

    fn set(&mut self, value: [f64; 2], by_user: bool) -> Effects {
        let mut effects = Effects::default();
        let next = [self.snapped(0, value[0]), self.snapped(1, value[1])];
        let changed = next != self.value;
        self.value = next;
        for (field, v) in self.fields() {
            effects.set(&field, v);
        }
        if changed {
            let arguments = vec![next[0].into(), next[1].into()];
            if by_user {
                effects.raise("moved", arguments.clone());
            }
            effects.raise("value_changed", arguments);
        }
        effects
    }

    fn value_at_point(&self, arguments: &[IpcValue]) -> Option<[f64; 2]> {
        let (x, y) = (number(arguments.first())?, number(arguments.get(1))?);
        let (w, h) = (
            number(arguments.get(2))?.max(1.0),
            number(arguments.get(3))?.max(1.0),
        );
        if self.polar {
            let (dx, dy) = (x / w - 0.5, y / h - 0.5);
            let dx = if self.base.mirrored { -dx } else { dx };
            let angle = dx.atan2(-dy).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
            let r = (dx.hypot(dy) * 2.0).min(1.0);
            return Some([
                self.from[0] + (self.to[0] - self.from[0]) * angle,
                self.from[1] + (self.to[1] - self.from[1]) * r,
            ]);
        }
        let mut p = [(x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0)];
        // Undo the drawing's flips: `visual` is its own inverse.
        if self.base.mirrored {
            p[0] = 1.0 - p[0];
        }
        if self.y_up {
            p[1] = 1.0 - p[1];
        }
        let p = self.constrained(p);
        Some([
            self.from[0] + (self.to[0] - self.from[0]) * p[0],
            self.from[1] + (self.to[1] - self.from[1]) * p[1],
        ])
    }

    fn unit(&self, axis: usize) -> f64 {
        if self.step[axis] > 0.0 {
            self.step[axis]
        } else {
            (self.to[axis] - self.from[axis]).abs() / 100.0
        }
    }
}

impl Archetype for Plane {
    fn name(&self) -> &'static str {
        "Plane"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        match event {
            "pressed" | "dragged" => {
                let mut effects = if event == "pressed" {
                    self.base.handle(event, arguments).unwrap_or_default()
                } else {
                    Effects::default()
                };
                if !self.base.enabled || (event == "dragged" && !self.dragging && !self.base.down) {
                    return Ok(effects);
                }
                self.dragging = true;
                if let Some(value) = self.value_at_point(arguments) {
                    effects.extend(self.set(value, true));
                }
                Ok(effects)
            }
            "released" | "canceled" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                if self.dragging {
                    self.dragging = false;
                    effects.set("dragging", false);
                    // A spring takes it home.
                    if self.spring {
                        let rest = self.rest.unwrap_or([
                            (self.from[0] + self.to[0]) / 2.0,
                            (self.from[1] + self.to[1]) / 2.0,
                        ]);
                        effects.extend(self.set(rest, true));
                    }
                }
                Ok(effects)
            }
            "wheel" => {
                let (sx, sy) = (
                    number(arguments.first()).unwrap_or(0.0),
                    number(arguments.get(1)).unwrap_or(0.0),
                );
                if (sx == 0.0 && sy == 0.0) || !self.base.enabled {
                    return Ok(Effects::default());
                }
                let value = [
                    self.value[0] - sx * self.unit(0) * 5.0,
                    self.value[1] - sy * self.unit(1) * 5.0,
                ];
                Ok(self.set(value, true).handled())
            }
            "key" => {
                if !self.base.enabled {
                    return Ok(Effects::default());
                }
                let name = text(arguments.first()).unwrap_or("");
                let big = |axis: usize| self.unit(axis) * 10.0;
                let right = if self.base.mirrored { -1.0 } else { 1.0 };
                let down = if self.y_up { -1.0 } else { 1.0 };
                let delta = match name {
                    "Left" => Some([-self.unit(0) * right, 0.0]),
                    "Right" => Some([self.unit(0) * right, 0.0]),
                    "Up" => Some([0.0, -self.unit(1) * down]),
                    "Down" => Some([0.0, self.unit(1) * down]),
                    "Page_Up" => Some([0.0, -big(1) * down]),
                    "Page_Down" => Some([0.0, big(1) * down]),
                    _ => None,
                };
                match delta {
                    Some([dx, dy]) => {
                        let value = [self.value[0] + dx, self.value[1] + dy];
                        Ok(self.set(value, true).handled())
                    }
                    None => Ok(Effects::default()),
                }
            }
            "clicked" => Ok(Effects::default()),
            _ => self
                .base
                .handle(event, arguments)
                .ok_or_else(|| format!("Plane has no event `{event}`")),
        }
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            let mut effects = result?;
            for (f, v) in self.fields() {
                effects.set(&f, v);
            }
            return Ok(effects);
        }
        let n = || expect_number(Some(value), field);
        match field {
            "x_from" => self.from[0] = n()?,
            "x_to" => self.to[0] = n()?,
            "y_from" => self.from[1] = n()?,
            "y_to" => self.to[1] = n()?,
            "step_x" => self.step[0] = n()?.max(0.0),
            "step_y" => self.step[1] = n()?.max(0.0),
            "x" => return Ok(self.set([n()?, self.value[1]], false)),
            "y" => return Ok(self.set([self.value[0], n()?], false)),
            "y_up" => self.y_up = expect_boolean(Some(value), field)?,
            "spring" => self.spring = expect_boolean(Some(value), field)?,
            "polar" => self.polar = expect_boolean(Some(value), field)?,
            "rest_x" => {
                let r = self.rest.unwrap_or([0.0, 0.0]);
                self.rest = Some([n()?, r[1]]);
            }
            "rest_y" => {
                let r = self.rest.unwrap_or([0.0, 0.0]);
                self.rest = Some([r[0], n()?]);
            }
            "constraint" => {
                self.constraint = match text(Some(value)) {
                    Some("free") => Constraint::Free,
                    Some("circle") => Constraint::Circle,
                    Some("square") => Constraint::Square,
                    _ => return Err("constraint is free, circle or square".into()),
                }
            }
            _ => return Err(format!("Plane has no setting `{field}`")),
        }
        let current = self.value;
        Ok(self.set(current, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(settings: &[(&str, IpcValue)]) -> Plane {
        let mut plane = Plane::new();
        for (field, value) in settings {
            plane.configure(field, value).unwrap();
        }
        plane
    }

    #[test]
    fn a_joystick_springs_home_and_a_wheel_is_polar() {
        let mut stick = plane(&[
            ("x_from", (-1.0).into()),
            ("y_from", (-1.0).into()),
            ("spring", true.into()),
        ]);
        stick
            .handle(
                "pressed",
                &[90.0.into(), 50.0.into(), 100.0.into(), 100.0.into()],
            )
            .unwrap();
        assert!(stick.value[0] > 0.5);
        stick.handle("released", &[]).unwrap();
        assert_eq!(stick.value, [0.0, 0.0]);
        let mut wheel = plane(&[("polar", true.into())]);
        // Right of the centre at the rim: a quarter turn, all the way out.
        wheel
            .handle(
                "pressed",
                &[100.0.into(), 50.0.into(), 100.0.into(), 100.0.into()],
            )
            .unwrap();
        assert!((wheel.value[0] - 0.25).abs() < 1e-9 && (wheel.value[1] - 1.0).abs() < 1e-9);
        assert!((wheel.visual(0) - 1.0).abs() < 1e-9 && (wheel.visual(1) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_press_sets_both_values() {
        let mut pad = plane(&[("x_to", 100.0.into()), ("y_to", 50.0.into())]);
        let effects = pad
            .handle(
                "pressed",
                &[30.0.into(), 20.0.into(), 100.0.into(), 100.0.into()],
            )
            .unwrap();
        assert_eq!(
            effects.signals[1],
            ("moved".into(), vec![30.0.into(), 10.0.into()])
        );
    }

    #[test]
    fn y_up_runs_bottom_to_top() {
        let mut chart = plane(&[("y_up", true.into())]);
        chart
            .handle(
                "pressed",
                &[0.0.into(), 25.0.into(), 100.0.into(), 100.0.into()],
            )
            .unwrap();
        assert!((chart.value[1] - 0.75).abs() < 1e-9);
        chart.handle("key", &["Up".into(), "".into()]).unwrap();
        assert!((chart.value[1] - 0.76).abs() < 1e-9);
    }

    #[test]
    fn a_circle_keeps_the_handle_inside() {
        let mut wheel = plane(&[("constraint", "circle".into())]);
        wheel
            .handle(
                "pressed",
                &[100.0.into(), 100.0.into(), 100.0.into(), 100.0.into()],
            )
            .unwrap();
        let (dx, dy) = (wheel.value[0] - 0.5, wheel.value[1] - 0.5);
        assert!((dx.hypot(dy) - 0.5).abs() < 1e-9);
    }
}
