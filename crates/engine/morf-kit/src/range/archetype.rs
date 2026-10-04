//! `Range` as an archetype: its state, its events, and its settings.

use morf_value::IpcValue;

use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

use super::{DragMode, Range, Snap};

impl Archetype for Range {
    fn name(&self) -> &'static str {
        "Range"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.value_fields());
        if !self.pair {
            fields.extend([
                ("first".into(), self.values[0].into()),
                ("second".into(), self.values[1].into()),
                ("first_position".into(), 0.0.into()),
                ("second_position".into(), 0.0.into()),
                ("first_visual_position".into(), 0.0.into()),
                ("second_visual_position".into(), 0.0.into()),
            ]);
        }
        fields.push(("from".into(), self.from.into()));
        fields.push(("to".into(), self.to.into()));
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        match event {
            "pressed" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                if !self.base.enabled {
                    return Ok(effects);
                }
                match self.drag_mode {
                    // A knob turned by dragging stays where it was pressed.
                    DragMode::Vertical => {
                        let start = self.position_of(self.values[0]);
                        self.press = (number(arguments.get(1)).unwrap_or(0.0), start);
                        self.dragging = Some((0, start));
                        self.fields_into(&mut effects);
                    }
                    DragMode::Angular => {
                        if let (Some(x), Some(y), Some(w), Some(h)) = (
                            number(arguments.first()),
                            number(arguments.get(1)),
                            number(arguments.get(2)),
                            number(arguments.get(3)),
                        ) {
                            let p = self.angular_position(x, y, w, h);
                            effects.extend(self.drag_to(0, p));
                        }
                    }
                    DragMode::Linear => {
                        if let Some((position, along)) = self.position_from_point(arguments) {
                            self.last_along = along;
                            let index = self.nearest_handle(position);
                            effects.extend(self.drag_to(index, position));
                        }
                    }
                }
                Ok(effects)
            }
            "dragged" => {
                let Some((index, reached)) = self.dragging else {
                    return Ok(Effects::default());
                };
                let fine = text(arguments.get(4)).is_some_and(|m| m.contains("shift"));
                match self.drag_mode {
                    DragMode::Vertical => {
                        let y = number(arguments.get(1)).unwrap_or(self.press.0);
                        let travel = self.drag_travel.max(1.0) * if fine { 10.0 } else { 1.0 };
                        let moved = (self.press.0 - y) / travel;
                        let moved = if self.inverted { -moved } else { moved };
                        return Ok(self.drag_to(0, (self.press.1 + moved).clamp(0.0, 1.0)));
                    }
                    DragMode::Angular => {
                        if let (Some(x), Some(y), Some(w), Some(h)) = (
                            number(arguments.first()),
                            number(arguments.get(1)),
                            number(arguments.get(2)),
                            number(arguments.get(3)),
                        ) {
                            let p = self.angular_position(x, y, w, h);
                            return Ok(self.drag_to(0, p));
                        }
                        return Ok(Effects::default());
                    }
                    DragMode::Linear => {}
                }
                let Some((position, along)) = self.position_from_point(arguments) else {
                    return Ok(Effects::default());
                };
                let target = if fine {
                    // A tenth of the motion, from where the drag had got to.
                    let length = if self.vertical {
                        number(arguments.get(3))
                    } else {
                        number(arguments.get(2))
                    };
                    let travel = (length.unwrap_or(1.0) - self.handle_size).max(1.0);
                    let moved = (along - self.last_along) / travel / 10.0;
                    reached
                        + if self.visual(1.0) < 0.5 {
                            -moved
                        } else {
                            moved
                        }
                } else {
                    position
                };
                self.last_along = along;
                Ok(self.drag_to(index, target.clamp(0.0, 1.0)))
            }
            "released" | "canceled" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                if let Some((index, position)) = self.dragging.take() {
                    let value = if self.live {
                        self.values[index]
                    } else {
                        self.value_at(position)
                    };
                    let value = if self.snap == Snap::None {
                        value
                    } else {
                        self.snapped(value)
                    };
                    effects.extend(self.set_value(index, value, true));
                }
                Ok(effects)
            }
            "wheel" => {
                let steps = number(arguments.get(1)).unwrap_or(0.0)
                    + number(arguments.first()).unwrap_or(0.0);
                if steps == 0.0 || !self.base.enabled {
                    return Ok(Effects::default());
                }
                // A wheel turned down lowers the value: by a step, or a
                // twentieth of the span for a continuous range.
                let notch = if self.step > 0.0 {
                    self.step
                } else {
                    (self.to - self.from).abs() / 20.0
                };
                Ok(self.by(-steps * notch).handled())
            }
            // A value asked for outright (a screen reader's): moved there as
            // the user would.
            "set" => {
                let v = crate::value::expect_number(arguments.first(), "value")?;
                Ok(self.set_value(0, v, true))
            }
            "increase" => Ok(self.by(self.unit())),
            "decrease" => Ok(self.by(-self.unit())),
            "key" => {
                if !self.base.enabled {
                    return Ok(Effects::default());
                }
                let name = text(arguments.first()).unwrap_or("");
                let backwards = self.visual(1.0) < 0.5;
                let horizontal = |more: bool| {
                    if backwards && !self.vertical {
                        !more
                    } else {
                        more
                    }
                };
                let amount = match name {
                    "Right" if !self.vertical => Some(if horizontal(true) {
                        self.unit()
                    } else {
                        -self.unit()
                    }),
                    "Left" if !self.vertical => Some(if horizontal(true) {
                        -self.unit()
                    } else {
                        self.unit()
                    }),
                    "Up" => Some(self.unit()),
                    "Down" => Some(-self.unit()),
                    "Page_Up" => Some(self.page()),
                    "Page_Down" => Some(-self.page()),
                    _ => None,
                };
                if let Some(amount) = amount {
                    let sign = if self.to < self.from { -1.0 } else { 1.0 };
                    return Ok(self.by(amount * sign).handled());
                }
                match name {
                    "Home" => Ok(self.set_value(0, self.from, true).handled()),
                    "End" => Ok(self.set_value(0, self.to, true).handled()),
                    _ => Ok(Effects::default()),
                }
            }
            "clicked" => Ok(Effects::default()),
            _ => self
                .base
                .handle(event, arguments)
                .ok_or_else(|| format!("Range has no event `{event}`")),
        }
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            let mut effects = result?;
            if field == "mirrored" {
                self.fields_into(&mut effects);
            }
            return Ok(effects);
        }
        let n = || expect_number(Some(value), field);
        let mut effects = Effects::default();
        match field {
            "from" | "to" => {
                if field == "from" {
                    self.from = n()?
                } else {
                    self.to = n()?
                }
                effects.set(field, n()?);
                for index in 0..2 {
                    let v = self.values[index];
                    effects.extend(self.set_value(index, v, false));
                }
            }
            "value" | "first" => return Ok(self.set_value(0, n()?, false)),
            "second" => return Ok(self.set_value(1, n()?, false)),
            "step" => self.step = n()?.max(0.0),
            "page_step" => self.page_step = Some(n()?.abs()),
            "handle_size" => self.handle_size = n()?.max(0.0),
            "drag_mode" => {
                self.drag_mode = match text(Some(value)) {
                    Some("linear") => DragMode::Linear,
                    Some("vertical") => DragMode::Vertical,
                    Some("angular") => DragMode::Angular,
                    _ => return Err("drag_mode is linear, vertical or angular".into()),
                }
            }
            "drag_travel" => self.drag_travel = n()?.max(1.0),
            "angle_from" => {
                self.angle_from = n()?;
                self.fields_into(&mut effects);
            }
            "angle_sweep" => {
                self.angle_sweep = n()?;
                self.fields_into(&mut effects);
            }
            "snap" => {
                self.snap = match text(Some(value)) {
                    Some("none") => Snap::None,
                    Some("always") => Snap::Always,
                    Some("on_release") => Snap::OnRelease,
                    _ => return Err("snap is none, always or on_release".into()),
                }
            }
            "orientation" => {
                self.vertical = match text(Some(value)) {
                    Some("horizontal") => false,
                    Some("vertical") => true,
                    _ => return Err("orientation is horizontal or vertical".into()),
                };
                self.fields_into(&mut effects);
            }
            "live" => self.live = expect_boolean(Some(value), field)?,
            "inverted" => {
                self.inverted = expect_boolean(Some(value), field)?;
                self.fields_into(&mut effects);
            }
            "logarithmic" => {
                self.logarithmic = expect_boolean(Some(value), field)?;
                self.fields_into(&mut effects);
            }
            "wrap" => self.wrap = expect_boolean(Some(value), field)?,
            "range" => {
                self.pair = expect_boolean(Some(value), field)?;
                self.fields_into(&mut effects);
            }
            _ => return Err(format!("Range has no setting `{field}`")),
        }
        Ok(effects)
    }
}
