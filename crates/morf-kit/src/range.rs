//! `Range`: a value between two ends -- sliders, spin buttons, scroll bars,
//! seek bars, knobs, progress.
//!
//! Settings: `from` (0), `to` (1), `value`, `step` (0: continuous),
//! `page_step` (a tenth of the span), `snap` (`"none"`, `"always"`,
//! `"on_release"`), `live` (true: the value follows a drag; false: only
//! the position does until release), `orientation` (`"horizontal"`,
//! `"vertical"`), `inverted`, `logarithmic`, `wrap` (an angle: past one
//! end is the other), `range` (two values, `first` and `second`), and
//! `handle_size` (px: the travel a pointer maps onto is the length less
//! the handle).
//!
//! State: `value`, `position` (0..1 along the range), `visual_position`
//! (where to draw it: right to left and inverted taken into account),
//! `first`, `second` and their positions for a pair, `dragging`.
//!
//! Events: the base's, plus `"pressed"` and `"dragged"` (local x, y,
//! width, height, modifiers -- Shift drags finely), `"released"`,
//! `"wheel"` (steps x, y: a step each, or a twentieth of a continuous
//! range), `"key"` (name, modifiers), `"increase"`,
//! `"decrease"`. Signals: `moved` (the user changed it) and
//! `value_changed` (anything did), each with the value (and for a pair
//! both values).

use morf_lua::IpcValue;

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Snap {
    None,
    Always,
    OnRelease,
}

pub(crate) struct Range {
    pub(crate) base: ControlState,
    from: f64,
    to: f64,
    /// The values: one, or a pair's first and second.
    values: [f64; 2],
    pair: bool,
    step: f64,
    page_step: Option<f64>,
    snap: Snap,
    live: bool,
    vertical: bool,
    inverted: bool,
    logarithmic: bool,
    wrap: bool,
    handle_size: f64,
    /// The handle a drag moves, and the position it has reached.
    dragging: Option<(usize, f64)>,
    /// Where the last drag event was, along the axis, for a fine drag.
    last_along: f64,
}

impl Range {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            from: 0.0,
            to: 1.0,
            values: [0.0, 1.0],
            pair: false,
            step: 0.0,
            page_step: None,
            snap: Snap::None,
            live: true,
            vertical: false,
            inverted: false,
            logarithmic: false,
            wrap: false,
            handle_size: 0.0,
            dragging: None,
            last_along: 0.0,
        }
    }

    fn low(&self) -> f64 {
        self.from.min(self.to)
    }

    fn high(&self) -> f64 {
        self.from.max(self.to)
    }

    fn log_usable(&self) -> bool {
        self.logarithmic && self.from > 0.0 && self.to > 0.0
    }

    /// A value's position, 0 at `from` and 1 at `to`.
    fn position_of(&self, value: f64) -> f64 {
        if self.to == self.from {
            return 0.0;
        }
        let p = if self.log_usable() {
            (value / self.from).ln() / (self.to / self.from).ln()
        } else {
            (value - self.from) / (self.to - self.from)
        };
        p.clamp(0.0, 1.0)
    }

    /// The value at a position.
    fn value_at(&self, position: f64) -> f64 {
        let p = position.clamp(0.0, 1.0);
        if self.log_usable() {
            self.from * (self.to / self.from).powf(p)
        } else {
            self.from + (self.to - self.from) * p
        }
    }

    fn snapped(&self, value: f64) -> f64 {
        if self.step <= 0.0 {
            return value;
        }
        let steps = ((value - self.from) / self.step).round();
        (self.from + steps * self.step).clamp(self.low(), self.high())
    }

    /// A value brought into range: wrapped round for an angle, clamped
    /// otherwise.
    fn bounded(&self, value: f64) -> f64 {
        if self.wrap && self.high() > self.low() {
            let span = self.high() - self.low();
            self.low() + (value - self.low()).rem_euclid(span)
        } else {
            value.clamp(self.low(), self.high())
        }
    }

    fn page(&self) -> f64 {
        self.page_step.unwrap_or((self.to - self.from).abs() / 10.0)
    }

    fn unit(&self) -> f64 {
        if self.step > 0.0 {
            self.step
        } else {
            (self.to - self.from).abs() / 100.0
        }
    }

    /// Where to draw a position: right to left flips a horizontal range,
    /// and a vertical one runs bottom to top unless inverted.
    fn visual(&self, position: f64) -> f64 {
        let flipped = if self.vertical {
            !self.inverted
        } else {
            self.inverted != self.base.mirrored
        };
        if flipped { 1.0 - position } else { position }
    }

    fn fields_into(&self, effects: &mut Effects) {
        for (field, value) in self.value_fields() {
            effects.set(&field, value);
        }
    }

    fn value_fields(&self) -> Vec<(String, IpcValue)> {
        let shown = |index: usize| match self.dragging {
            Some((handle, position)) if handle == index && !self.live => position,
            _ => self.position_of(self.values[index]),
        };
        let mut fields = vec![
            ("value".into(), self.values[0].into()),
            ("position".into(), shown(0).into()),
            ("visual_position".into(), self.visual(shown(0)).into()),
            ("dragging".into(), self.dragging.is_some().into()),
        ];
        if self.pair {
            fields.extend([
                ("first".into(), self.values[0].into()),
                ("second".into(), self.values[1].into()),
                ("first_position".into(), shown(0).into()),
                ("second_position".into(), shown(1).into()),
                ("first_visual_position".into(), self.visual(shown(0)).into()),
                (
                    "second_visual_position".into(),
                    self.visual(shown(1)).into(),
                ),
            ]);
        }
        fields
    }

    /// Sets handle `index` to `value` and answers what changed, raising
    /// `moved` when `by_user`.
    fn set_value(&mut self, index: usize, value: f64, by_user: bool) -> Effects {
        let mut effects = Effects::default();
        let mut value = self.bounded(value);
        if self.snap == Snap::Always || !by_user {
            value = self.snapped(value);
        }
        // A pair's handles do not cross.
        if self.pair {
            value = if index == 0 {
                value.min(self.values[1])
            } else {
                value.max(self.values[0])
            };
        }
        let changed = (value - self.values[index]).abs() > f64::EPSILON * 16.0;
        self.values[index] = value;
        self.fields_into(&mut effects);
        if changed {
            let arguments: Vec<IpcValue> = if self.pair {
                vec![self.values[0].into(), self.values[1].into()]
            } else {
                vec![value.into()]
            };
            if by_user {
                effects.raise("moved", arguments.clone());
            }
            effects.raise("value_changed", arguments);
        }
        effects
    }

    /// The position along the axis a local point is at, 0..1 in value
    /// order.
    fn position_from_point(&self, arguments: &[IpcValue]) -> Option<(f64, f64)> {
        let (x, y) = (number(arguments.first())?, number(arguments.get(1))?);
        let (w, h) = (number(arguments.get(2))?, number(arguments.get(3))?);
        let (along, length) = if self.vertical { (y, h) } else { (x, w) };
        let travel = (length - self.handle_size).max(1.0);
        let visual = ((along - self.handle_size / 2.0) / travel).clamp(0.0, 1.0);
        // Undo `visual`: it is its own inverse.
        Some((self.visual(visual), along))
    }

    fn nearest_handle(&self, position: f64) -> usize {
        if !self.pair {
            return 0;
        }
        let d0 = (self.position_of(self.values[0]) - position).abs();
        let d1 = (self.position_of(self.values[1]) - position).abs();
        if d1 < d0 || (d1 == d0 && position > self.position_of(self.values[1])) {
            1
        } else {
            0
        }
    }

    fn drag_to(&mut self, index: usize, position: f64) -> Effects {
        self.dragging = Some((index, position));
        if self.live {
            self.set_value(index, self.value_at(position), true)
        } else {
            let mut effects = Effects::default();
            self.fields_into(&mut effects);
            effects
        }
    }

    fn by(&mut self, amount: f64) -> Effects {
        let value = self.values[0] + amount;
        self.set_value(0, value, true)
    }
}

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
                if let Some((position, along)) = self.position_from_point(arguments) {
                    self.last_along = along;
                    let index = self.nearest_handle(position);
                    effects.extend(self.drag_to(index, position));
                }
                Ok(effects)
            }
            "dragged" => {
                let Some((index, reached)) = self.dragging else {
                    return Ok(Effects::default());
                };
                let Some((position, along)) = self.position_from_point(arguments) else {
                    return Ok(Effects::default());
                };
                let fine = text(arguments.get(4)).is_some_and(|m| m.contains("shift"));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn range(settings: &[(&str, IpcValue)]) -> Range {
        let mut range = Range::new();
        for (field, value) in settings {
            range.configure(field, value).unwrap();
        }
        range
    }

    fn signal<'a>(effects: &'a Effects, name: &str) -> Option<&'a Vec<IpcValue>> {
        effects
            .signals
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a)
    }

    #[test]
    fn a_press_jumps_and_a_drag_follows() {
        let mut slider = range(&[("from", 0.0.into()), ("to", 100.0.into())]);
        let effects = slider
            .handle(
                "pressed",
                &[25.0.into(), 5.0.into(), 100.0.into(), 10.0.into()],
            )
            .unwrap();
        assert_eq!(signal(&effects, "moved"), Some(&vec![25.0.into()]));
        let effects = slider
            .handle(
                "dragged",
                &[75.0.into(), 5.0.into(), 100.0.into(), 10.0.into()],
            )
            .unwrap();
        assert_eq!(signal(&effects, "value_changed"), Some(&vec![75.0.into()]));
        slider.handle("released", &[]).unwrap();
        assert!(slider.dragging.is_none());
    }

    #[test]
    fn steps_snap_and_keys_move_by_them() {
        let mut slider = range(&[
            ("to", 10.0.into()),
            ("step", 1.0.into()),
            ("snap", "always".into()),
        ]);
        slider
            .handle(
                "pressed",
                &[33.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
            )
            .unwrap();
        assert_eq!(slider.values[0], 3.0);
        assert!(
            slider
                .handle("key", &["Right".into(), "".into()])
                .unwrap()
                .handled
        );
        assert_eq!(slider.values[0], 4.0);
        slider.handle("key", &["End".into(), "".into()]).unwrap();
        assert_eq!(slider.values[0], 10.0);
        slider
            .handle("key", &["Page_Down".into(), "".into()])
            .unwrap();
        assert_eq!(slider.values[0], 9.0);
        assert!(
            !slider
                .handle("key", &["a".into(), "".into()])
                .unwrap()
                .handled
        );
    }

    #[test]
    fn right_to_left_flips_the_drawing_and_the_arrows() {
        let mut slider = range(&[
            ("to", 10.0.into()),
            ("step", 1.0.into()),
            ("mirrored", true.into()),
        ]);
        slider.configure("value", &2.0.into()).unwrap();
        let fields = slider.value_fields();
        assert!(fields.contains(&("visual_position".into(), 0.8.into())));
        slider.handle("key", &["Left".into(), "".into()]).unwrap();
        assert_eq!(slider.values[0], 3.0);
    }

    #[test]
    fn a_vertical_range_runs_bottom_to_top() {
        let mut fader = range(&[("orientation", "vertical".into())]);
        fader
            .handle(
                "pressed",
                &[5.0.into(), 25.0.into(), 10.0.into(), 100.0.into()],
            )
            .unwrap();
        assert!((fader.values[0] - 0.75).abs() < 1e-9);
    }

    #[test]
    fn a_seek_bar_moves_only_its_position_until_release() {
        let mut seek = range(&[("live", false.into())]);
        let effects = seek
            .handle(
                "pressed",
                &[50.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
            )
            .unwrap();
        assert!(signal(&effects, "moved").is_none());
        assert!(effects.changed.contains(&("position".into(), 0.5.into())));
        let effects = seek.handle("released", &[]).unwrap();
        assert_eq!(signal(&effects, "moved"), Some(&vec![0.5.into()]));
    }

    #[test]
    fn a_logarithmic_range_spaces_decades_evenly() {
        let mut volume = range(&[
            ("from", 1.0.into()),
            ("to", 1000.0.into()),
            ("logarithmic", true.into()),
        ]);
        volume
            .handle(
                "pressed",
                &[50.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
            )
            .unwrap();
        assert!((volume.values[0] - 1000f64.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn a_pair_moves_the_nearer_handle_and_never_crosses() {
        let mut pair = range(&[
            ("range", true.into()),
            ("first", 0.2.into()),
            ("second", 0.6.into()),
        ]);
        pair.handle(
            "pressed",
            &[70.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
        )
        .unwrap();
        assert!((pair.values[1] - 0.7).abs() < 1e-9);
        pair.handle(
            "dragged",
            &[10.0.into(), 0.0.into(), 100.0.into(), 10.0.into()],
        )
        .unwrap();
        assert!((pair.values[1] - 0.2).abs() < 1e-9, "{:?}", pair.values);
    }

    #[test]
    fn an_angle_wraps_round() {
        let mut angle = range(&[
            ("to", 360.0.into()),
            ("wrap", true.into()),
            ("step", 10.0.into()),
        ]);
        angle.configure("value", &350.0.into()).unwrap();
        angle.handle("key", &["Right".into(), "".into()]).unwrap();
        assert_eq!(angle.values[0], 0.0);
    }

    #[test]
    fn the_wheel_steps_down_when_turned_down() {
        let mut slider = range(&[("to", 10.0.into()), ("step", 1.0.into())]);
        slider.configure("value", &5.0.into()).unwrap();
        slider.handle("wheel", &[0.0.into(), 1.0.into()]).unwrap();
        assert_eq!(slider.values[0], 4.0);
    }
}
