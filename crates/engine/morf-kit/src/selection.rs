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
//! Shift extends), `"item_activated"` (index: a double press), `"point"`
//! (dx, dy, dead radius: a radial menu's pointer from its centre -- the
//! item whose sector it is in becomes current, none inside the dead
//! radius), `"point_release"` (the same, let go: that item is activated --
//! a marking menu's flick), `"key"`
//! (name, modifiers, text, now in ms -- for typeahead). Signals:
//! `current_changed` (index), `selection_changed` (indices), `activated`
//! (index).

mod archetype;

use std::collections::BTreeSet;

use morf_value::{IpcTable, IpcValue};

use crate::Effects;
use crate::control::ControlState;

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
    /// Alt with the arrows asks to move the current entry (`reorder`).
    reorderable: bool,
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
            reorderable: false,
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
        let single =
            self.typed.chars().all(|c| self.typed.starts_with(c)) && self.typed.chars().count() > 1;
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

#[cfg(test)]
mod tests;
