//! `Selection`: a current item, or a set, among many -- tabs, segmented
//! choices, list and grid selection, radio groups, swatch grids.
//!
//! Settings: `count` (items), `labels` (their text, for typeahead),
//! `current` (index from 1; 0 for none), `mode` (`"single"`, `"multi"`,
//! `"range"`, `"none"`), `wrap`, `orientation` (`"horizontal"`,
//! `"vertical"`, `"grid"`), `columns` (a grid's), `page` (rows a Page key
//! moves), `disabled` (indices that cannot be current), `follow_focus`
//! (in single mode the current item is the selected one).
//!
//! State: `current`, `selected` (indices), `count`.
//!
//! Events: the base's, `"item_pressed"` (index, modifiers: Ctrl toggles,
//! Shift extends), `"item_activated"` (index: a double press), `"key"`
//! (name, modifiers, text, now in ms -- for typeahead). Signals:
//! `current_changed` (index), `selection_changed` (indices), `activated`
//! (index).

use std::collections::BTreeSet;

use morf_lua::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Single,
    Multi,
    Range,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Orientation {
    Horizontal,
    Vertical,
    Grid,
}

/// How long typed letters gather into one search.
const TYPEAHEAD_MS: f64 = 1000.0;

pub(crate) struct Selection {
    pub(crate) base: ControlState,
    count: i64,
    labels: Vec<String>,
    current: i64,
    selected: BTreeSet<i64>,
    /// Where a Shift extension is measured from.
    anchor: i64,
    mode: Mode,
    wrap: bool,
    orientation: Orientation,
    columns: i64,
    page: i64,
    disabled: BTreeSet<i64>,
    follow_focus: bool,
    typed: String,
    typed_at: f64,
}

fn indices(set: &BTreeSet<i64>) -> IpcValue {
    IpcValue::Table(std::sync::Arc::new(IpcTable::List(
        set.iter().map(|i| IpcValue::Integer(*i)).collect(),
    )))
}

impl Selection {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            count: 0,
            labels: Vec::new(),
            current: 0,
            selected: BTreeSet::new(),
            anchor: 0,
            mode: Mode::Single,
            wrap: false,
            orientation: Orientation::Horizontal,
            columns: 1,
            page: 10,
            disabled: BTreeSet::new(),
            follow_focus: true,
            typed: String::new(),
            typed_at: f64::NEG_INFINITY,
        }
    }

    fn usable(&self, index: i64) -> bool {
        index >= 1 && index <= self.count && !self.disabled.contains(&index)
    }

    /// Makes `index` current; in single mode following focus, selected
    /// too. Raises what changed.
    fn go(&mut self, index: i64, effects: &mut Effects) {
        if !self.usable(index) {
            return;
        }
        if index != self.current {
            self.current = index;
            effects.set("current", index);
            effects.raise("current_changed", vec![index.into()]);
        }
        if self.mode == Mode::Single && self.follow_focus {
            self.select_only(index, effects);
        }
    }

    fn select_only(&mut self, index: i64, effects: &mut Effects) {
        let next: BTreeSet<i64> = [index].into_iter().collect();
        self.set_selected(next, effects);
    }

    fn set_selected(&mut self, next: BTreeSet<i64>, effects: &mut Effects) {
        if next != self.selected {
            self.selected = next;
            effects.set("selected", indices(&self.selected));
            effects.raise("selection_changed", vec![indices(&self.selected)]);
        }
    }

    /// The usable index `step` items from the current along, skipping
    /// disabled ones; wrapping round if asked.
    fn step(&self, step: i64) -> Option<i64> {
        if self.count == 0 {
            return None;
        }
        let mut at = if self.current == 0 {
            if step > 0 { 0 } else { self.count + 1 }
        } else {
            self.current
        };
        for _ in 0..self.count {
            at += step;
            if at < 1 || at > self.count {
                if !self.wrap {
                    return None;
                }
                at = (at - 1).rem_euclid(self.count) + 1;
            }
            if self.usable(at) {
                return Some(at);
            }
        }
        None
    }

    fn first_usable(&self, from_end: bool) -> Option<i64> {
        let mut range: Box<dyn Iterator<Item = i64>> = if from_end {
            Box::new((1..=self.count).rev())
        } else {
            Box::new(1..=self.count)
        };
        range.find(|i| self.usable(*i))
    }

    fn move_by(&mut self, step: i64, extend: bool, effects: &mut Effects) -> bool {
        let Some(next) = self.step(step) else {
            return false;
        };
        self.go(next, effects);
        if extend && self.mode == Mode::Range {
            self.extend_to(next, effects);
        } else if !extend {
            self.anchor = next;
        }
        true
    }

    fn extend_to(&mut self, index: i64, effects: &mut Effects) {
        let anchor = if self.anchor == 0 { index } else { self.anchor };
        let (a, b) = (anchor.min(index), anchor.max(index));
        let next = (a..=b).filter(|i| self.usable(*i)).collect();
        self.set_selected(next, effects);
    }

    fn typeahead(&mut self, typed: &str, now: f64, effects: &mut Effects) -> bool {
        if now - self.typed_at > TYPEAHEAD_MS {
            self.typed.clear();
        }
        self.typed_at = now;
        self.typed.push_str(&typed.to_lowercase());
        // Repeating one letter cycles through the items it begins.
        let single = self
            .typed
            .chars()
            .all(|c| Some(c) == self.typed.chars().next())
            && self.typed.chars().count() > 1;
        let needle = if single {
            self.typed
                .chars()
                .next()
                .map(String::from)
                .unwrap_or_default()
        } else {
            self.typed.clone()
        };
        let start = if single || self.typed.chars().count() == 1 {
            self.current
        } else {
            self.current - 1
        };
        for offset in 1..=self.count {
            let index = (start + offset - 1).rem_euclid(self.count.max(1)) + 1;
            let label = self
                .labels
                .get((index - 1) as usize)
                .map(|l| l.to_lowercase())
                .unwrap_or_default();
            if self.usable(index) && label.starts_with(&needle) {
                self.go(index, effects);
                self.anchor = index;
                return true;
            }
        }
        false
    }
}

impl Selection {
    /// The number of entries.
    pub(crate) fn count(&self) -> i64 {
        self.count
    }

    /// The current entry, from 1 (0: none).
    pub(crate) fn current(&self) -> i64 {
        self.current
    }

    /// Makes `index` current, as an arrow would. Returns whether it could.
    pub(crate) fn go_to(&mut self, index: i64, effects: &mut Effects) -> bool {
        if !self.usable(index) {
            return false;
        }
        self.go(index, effects);
        self.anchor = index;
        true
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(settings: &[(&str, IpcValue)]) -> Selection {
        let mut s = Selection::new();
        for (field, value) in settings {
            s.configure(field, value).unwrap();
        }
        s
    }

    fn key(s: &mut Selection, name: &str, modifiers: &str) -> Effects {
        s.handle(
            "key",
            &[name.into(), modifiers.into(), "".into(), 0.0.into()],
        )
        .unwrap()
    }

    #[test]
    fn arrows_move_the_current_item_and_skip_disabled_ones() {
        let mut tabs = selection(&[
            ("count", 4.0.into()),
            ("current", 1.0.into()),
            (
                "disabled",
                IpcValue::Table(std::sync::Arc::new(IpcTable::List(vec![2i64.into()]))),
            ),
        ]);
        let effects = key(&mut tabs, "Right", "");
        assert!(effects.handled);
        assert_eq!(tabs.current, 3);
        assert_eq!(tabs.selected, [3].into_iter().collect());
        key(&mut tabs, "Right", "");
        assert!(!key(&mut tabs, "Right", "").handled, "no wrap past the end");
        key(&mut tabs, "Home", "");
        assert_eq!(tabs.current, 1);
    }

    #[test]
    fn a_grid_moves_by_rows_and_pages_stop_at_the_edge() {
        let mut grid = selection(&[
            ("count", 12.0.into()),
            ("orientation", "grid".into()),
            ("columns", 4.0.into()),
            ("current", 2.0.into()),
        ]);
        key(&mut grid, "Down", "");
        assert_eq!(grid.current, 6);
        key(&mut grid, "Page_Down", "");
        assert_eq!(grid.current, 12);
    }

    #[test]
    fn multi_mode_toggles_and_selects_all() {
        let mut list = selection(&[("count", 3.0.into()), ("mode", "multi".into())]);
        list.handle("item_pressed", &[1i64.into(), "".into()])
            .unwrap();
        list.handle("item_pressed", &[3i64.into(), "".into()])
            .unwrap();
        assert_eq!(list.selected, [1, 3].into_iter().collect());
        key(&mut list, "a", "ctrl");
        assert_eq!(list.selected.len(), 3);
    }

    #[test]
    fn shift_extends_a_range() {
        let mut list = selection(&[
            ("count", 6.0.into()),
            ("mode", "range".into()),
            ("orientation", "vertical".into()),
        ]);
        list.handle("item_pressed", &[2i64.into(), "".into()])
            .unwrap();
        key(&mut list, "Down", "shift");
        key(&mut list, "Down", "shift");
        assert_eq!(list.selected, [2, 3, 4].into_iter().collect());
    }

    #[test]
    fn typing_finds_an_item_by_its_label() {
        let labels = ["Firefox", "Files", "Terminal", "Thunar"]
            .iter()
            .map(|l| IpcValue::from(*l))
            .collect();
        let mut list = selection(&[
            ("count", 4.0.into()),
            ("orientation", "vertical".into()),
            (
                "labels",
                IpcValue::Table(std::sync::Arc::new(IpcTable::List(labels))),
            ),
        ]);
        list.handle("key", &["t".into(), "".into(), "t".into(), 0.0.into()])
            .unwrap();
        assert_eq!(list.current, 3);
        list.handle("key", &["h".into(), "".into(), "h".into(), 100.0.into()])
            .unwrap();
        assert_eq!(list.current, 4);
        list.handle("key", &["f".into(), "".into(), "f".into(), 5000.0.into()])
            .unwrap();
        assert_eq!(list.current, 1);
        list.handle("key", &["f".into(), "".into(), "f".into(), 5100.0.into()])
            .unwrap();
        assert_eq!(list.current, 2, "a repeated letter cycles");
    }

    #[test]
    fn return_activates_the_current_item() {
        let mut list = selection(&[("count", 2.0.into()), ("current", 2.0.into())]);
        let effects = key(&mut list, "Return", "");
        assert_eq!(effects.signals[0], ("activated".into(), vec![2i64.into()]));
    }
}
