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
//! the handle), `drag_mode` (`"linear"` along the track; `"vertical"`:
//! a knob turned by dragging up and down `drag_travel` pixels (200) for
//! the whole range, without jumping on the press; `"angular"`: by the
//! angle round its centre), `angle_from` (-135: degrees clockwise from
//! twelve o'clock where the range starts) and `angle_sweep` (270).
//!
//! State: `value`, `position` (0..1 along the range), `visual_position`
//! (where to draw it: right to left and inverted taken into account),
//! `first`, `second` and their positions for a pair, `dragging`, `angle`
//! (where a knob's pointer stands, in degrees from twelve o'clock).
//!
//! Events: the base's, plus `"pressed"` and `"dragged"` (local x, y,
//! width, height, modifiers -- Shift drags finely), `"released"`,
//! `"wheel"` (steps x, y: a step each, or a twentieth of a continuous
//! range), `"key"` (name, modifiers), `"increase"`,
//! `"decrease"`. Signals: `moved` (the user changed it) and
//! `value_changed` (anything did), each with the value (and for a pair
//! both values).

mod archetype;

use morf_value::IpcValue;

use crate::Effects;
use crate::control::ControlState;
use crate::value::number;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragMode {
    Linear,
    Vertical,
    Angular,
}

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
    drag_mode: DragMode,
    drag_travel: f64,
    angle_from: f64,
    angle_sweep: f64,
    /// A vertical drag's press: its y and the position then.
    press: (f64, f64),
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
            drag_mode: DragMode::Linear,
            drag_travel: 200.0,
            angle_from: -135.0,
            angle_sweep: 270.0,
            press: (0.0, 0.0),
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

    /// A position as a knob turns: inverted runs it backwards.
    fn visual_angle(&self, position: f64) -> f64 {
        if self.inverted {
            1.0 - position
        } else {
            position
        }
    }

    /// The position a point at an angle round the centre stands for; past
    /// the sweep, the nearer end.
    fn angular_position(&self, x: f64, y: f64, w: f64, h: f64) -> f64 {
        let angle = (x - w / 2.0).atan2(h / 2.0 - y).to_degrees();
        let sweep = self.angle_sweep.abs().max(1.0);
        let along = (angle - self.angle_from).rem_euclid(360.0);
        let p = if along <= sweep {
            along / sweep
        } else if along - sweep < 360.0 - along {
            1.0
        } else {
            0.0
        };
        self.visual_angle(p)
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
            (
                "angle".into(),
                (self.angle_from + self.visual_angle(shown(0)) * self.angle_sweep).into(),
            ),
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

#[cfg(test)]
mod tests;
