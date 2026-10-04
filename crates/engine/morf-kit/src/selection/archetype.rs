//! `Selection` as an archetype: its state, its events, and its settings.

use std::collections::BTreeSet;

use morf_value::{IpcTable, IpcValue};

use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

use super::{Mode, Orientation, Selection, indices};

impl Archetype for Selection {
    fn name(&self) -> &'static str {
        "Selection"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend([
            ("current".into(), self.current.into()),
            ("selected".into(), indices(&self.selected)),
            ("count".into(), self.count.into()),
        ]);
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let mut effects = Effects::default();
        match event {
            "item_pressed" => {
                let index = number(arguments.first()).unwrap_or(0.0) as i64;
                let modifiers = text(arguments.get(1)).unwrap_or("");
                if !self.base.enabled || !self.usable(index) || self.mode == Mode::None {
                    return Ok(effects);
                }
                self.go(index, &mut effects);
                match self.mode {
                    Mode::Multi | Mode::Range if modifiers.contains("ctrl") => {
                        let mut next = self.selected.clone();
                        if !next.remove(&index) {
                            next.insert(index);
                        }
                        self.set_selected(next, &mut effects);
                        self.anchor = index;
                    }
                    Mode::Range if modifiers.contains("shift") => {
                        self.extend_to(index, &mut effects)
                    }
                    Mode::Multi => {
                        let mut next = self.selected.clone();
                        if !next.remove(&index) {
                            next.insert(index);
                        }
                        self.set_selected(next, &mut effects);
                        self.anchor = index;
                    }
                    _ => {
                        self.select_only(index, &mut effects);
                        self.anchor = index;
                    }
                }
            }
            // A radial menu: item 1 at twelve o'clock, the rest clockwise,
            // each a sector of the circle.
            "point" | "point_release" => {
                let (dx, dy) = (number(arguments.first()).unwrap_or(0.0), number(arguments.get(1)).unwrap_or(0.0));
                let dead = number(arguments.get(2)).unwrap_or(16.0);
                if !self.base.enabled || self.count == 0 || dx.hypot(dy) < dead {
                    return Ok(effects);
                }
                let n = self.count as f64;
                let dx = if self.base.mirrored { -dx } else { dx };
                let angle = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
                let index = (((angle + 180.0 / n) / (360.0 / n)).floor() as i64).rem_euclid(self.count as i64) + 1;
                if self.usable(index) {
                    self.go(index, &mut effects);
                    if event == "point_release" {
                        effects.raise("activated", vec![index.into()]);
                    }
                }
            }
            "item_activated" => {
                let index = number(arguments.first()).unwrap_or(0.0) as i64;
                if self.base.enabled && self.usable(index) {
                    self.go(index, &mut effects);
                    effects.raise("activated", vec![index.into()]);
                }
            }
            "key" => {
                if !self.base.enabled || self.count == 0 {
                    return Ok(effects);
                }
                let name = text(arguments.first()).unwrap_or("");
                let modifiers = text(arguments.get(1)).unwrap_or("");
                let typed = text(arguments.get(2)).unwrap_or("");
                let now = number(arguments.get(3)).unwrap_or(0.0);
                let extend = modifiers.contains("shift");
                let ctrl = modifiers.contains("ctrl");
                // Alt with an arrow: move the current entry, not the choice.
                if self.reorderable && modifiers.contains("alt") && self.current > 0 {
                    let forward = if self.base.mirrored { -1 } else { 1 };
                    let step = match (self.orientation, name) {
                        (Orientation::Vertical, "Up") | (Orientation::Horizontal | Orientation::Grid, "Left") => Some(-forward),
                        (Orientation::Vertical, "Down") | (Orientation::Horizontal | Orientation::Grid, "Right") => Some(forward),
                        _ => None,
                    };
                    if let Some(step) = step {
                        let to = self.current + step;
                        if to >= 1 && to <= self.count {
                            effects.raise("reorder", vec![self.current.into(), step.into()]);
                            effects.handled = true;
                        }
                        return Ok(effects);
                    }
                }
                let forward = if self.base.mirrored { -1 } else { 1 };
                let columns = self.columns.max(1);
                let step = match (self.orientation, name) {
                    (Orientation::Horizontal, "Left") => Some(-forward),
                    (Orientation::Horizontal, "Right") => Some(forward),
                    (Orientation::Vertical, "Up") => Some(-1),
                    (Orientation::Vertical, "Down") => Some(1),
                    (Orientation::Grid, "Left") => Some(-forward),
                    (Orientation::Grid, "Right") => Some(forward),
                    (Orientation::Grid, "Up") => Some(-columns),
                    (Orientation::Grid, "Down") => Some(columns),
                    (_, "Page_Up") => Some(
                        -self.page
                            * if self.orientation == Orientation::Grid {
                                columns
                            } else {
                                1
                            },
                    ),
                    (_, "Page_Down") => Some(
                        self.page
                            * if self.orientation == Orientation::Grid {
                                columns
                            } else {
                                1
                            },
                    ),
                    _ => None,
                };
                let handled = if let Some(step) = step {
                    // A page goes as far as it can rather than nowhere.
                    if name.starts_with("Page") && self.step(step).is_none() {
                        let edge = self.first_usable(step > 0);
                        edge.is_some_and(|edge| {
                            self.go(edge, &mut effects);
                            true
                        })
                    } else {
                        self.move_by(step, extend, &mut effects)
                    }
                } else {
                    match name {
                        "Home" | "End" => self.first_usable(name == "End").is_some_and(|edge| {
                            self.go(edge, &mut effects);
                            if extend && self.mode == Mode::Range {
                                self.extend_to(edge, &mut effects);
                            }
                            true
                        }),
                        "space" if self.mode == Mode::Multi || self.mode == Mode::Range => {
                            let mut next = self.selected.clone();
                            if !next.remove(&self.current) && self.current > 0 {
                                next.insert(self.current);
                            }
                            self.set_selected(next, &mut effects);
                            true
                        }
                        "a" if ctrl && (self.mode == Mode::Multi || self.mode == Mode::Range) => {
                            let next = (1..=self.count).filter(|i| self.usable(*i)).collect();
                            self.set_selected(next, &mut effects);
                            true
                        }
                        "Return" | "KP_Enter" if self.current > 0 => {
                            effects.raise("activated", vec![self.current.into()]);
                            true
                        }
                        _ if !ctrl
                            && !modifiers.contains("alt")
                            && !typed.is_empty()
                            && typed.chars().all(|c| !c.is_control())
                            && !self.labels.is_empty() =>
                        {
                            self.typeahead(typed, now, &mut effects)
                        }
                        _ => false,
                    }
                };
                effects.handled = handled;
            }
            "clicked" => {}
            _ => {
                return self
                    .base
                    .handle(event, arguments)
                    .ok_or_else(|| format!("Selection has no event `{event}`"));
            }
        }
        Ok(effects)
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let mut effects = Effects::default();
        let list = |value: &IpcValue| -> Vec<IpcValue> {
            match value {
                IpcValue::Table(t) => match t.as_ref() {
                    IpcTable::List(items) => items.clone(),
                    IpcTable::Map(map) if map.is_empty() => Vec::new(),
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            }
        };
        match field {
            "count" => {
                self.count = expect_number(Some(value), field)?.max(0.0) as i64;
                effects.set("count", self.count);
                if self.current > self.count {
                    self.current = if self.count > 0 { self.count } else { 0 };
                    effects.set("current", self.current);
                }
                let kept = self
                    .selected
                    .iter()
                    .copied()
                    .filter(|i| *i <= self.count)
                    .collect();
                self.set_selected(kept, &mut effects);
                // Raising a signal for a change the configuration made is
                // left to `current`; this one is quiet.
                effects.signals.clear();
            }
            "labels" => {
                self.labels = list(value)
                    .into_iter()
                    .map(|v| match v {
                        IpcValue::String(s) => s,
                        other => format!("{other:?}"),
                    })
                    .collect();
            }
            "current" => {
                let index = expect_number(Some(value), field)? as i64;
                if index == 0 {
                    if self.current != 0 {
                        self.current = 0;
                        effects.set("current", 0i64);
                    }
                } else if index != self.current || self.mode == Mode::Single {
                    // Taken as given: the count it fits in may be written
                    // after it.
                    self.current = index;
                    self.anchor = index;
                    effects.set("current", index);
                    if self.mode == Mode::Single && self.follow_focus {
                        let next = [index].into_iter().collect();
                        if next != self.selected {
                            self.selected = next;
                            effects.set("selected", indices(&self.selected));
                        }
                    }
                }
            }
            "selected" => {
                let next: BTreeSet<i64> = list(value)
                    .iter()
                    .filter_map(|v| number(Some(v)))
                    .map(|n| n as i64)
                    .collect();
                if next != self.selected {
                    self.selected = next;
                    effects.set("selected", indices(&self.selected));
                }
            }
            "mode" => {
                self.mode = match text(Some(value)) {
                    Some("single") => Mode::Single,
                    Some("multi") => Mode::Multi,
                    Some("range") => Mode::Range,
                    Some("none") => Mode::None,
                    _ => return Err("mode is single, multi, range or none".into()),
                }
            }
            "orientation" => {
                self.orientation = match text(Some(value)) {
                    Some("horizontal") => Orientation::Horizontal,
                    Some("vertical") => Orientation::Vertical,
                    Some("grid") => Orientation::Grid,
                    _ => return Err("orientation is horizontal, vertical or grid".into()),
                }
            }
            "wrap" => self.wrap = expect_boolean(Some(value), field)?,
            "follow_focus" => self.follow_focus = expect_boolean(Some(value), field)?,
            "reorderable" => self.reorderable = expect_boolean(Some(value), field)?,
            "columns" => self.columns = expect_number(Some(value), field)?.max(1.0) as i64,
            "page" => self.page = expect_number(Some(value), field)?.max(1.0) as i64,
            "disabled" => {
                self.disabled = list(value)
                    .iter()
                    .filter_map(|v| number(Some(v)))
                    .map(|n| n as i64)
                    .collect();
            }
            _ => return Err(format!("Selection has no setting `{field}`")),
        }
        Ok(effects)
    }
}
