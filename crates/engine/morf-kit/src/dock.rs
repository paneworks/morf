//! `Dock`: panels arranged in a tree of splits and tab stacks -- an
//! editor's docked shelves, tool windows and tabbed containers -- that the
//! user rearranges by dragging tabs onto drop zones, floats, maximises
//! and closes.
//!
//! The tree is the archetype's; the panels' contents are the
//! configuration's, named by id. A split lays its children side by side
//! (`"horizontal"`) or one above the other (`"vertical"`) at `ratios`
//! summing to one; a stack shows one of its `panels` at a time, the
//! `current`.
//!
//! Settings: `layout` (the tree: `{ orientation, ratios, children }` for a
//! split, `{ panels, current }` for a stack; each may carry an `id`),
//! `floating` (`{ panel, x, y, w, h }`), `fixed` (panel ids that cannot be
//! closed), `edge` (the fraction of a stack along each side that drops
//! beside it rather than into it; 0.25), `min_ratio` (0.08).
//!
//! State: `focused` (the stack the keys act on) and `focused_panel` (its
//! current), `maximized` (a panel, or ""), `dragging` (the panel a tab drag
//! carries, or ""), `drop_target` (a stack id, `"float"`, or "") and
//! `drop_zone` (`"center"`, `"left"`, `"right"`, `"top"`, `"bottom"`),
//! `panel_count`.
//!
//! Events: the base's; `"activate"` (panel); `"close"` (panel);
//! `"drag_start"` (panel); `"drag_over"` (stack, x, y, width, height: the
//! pointer in that stack); `"drag_outside"`; `"drop"` (x, y, w, h: where a
//! floated panel goes); `"drag_cancel"`; `"resize"` (split, divider index,
//! position 0..1 across the split); `"maximize"` (panel); `"float"` (panel,
//! x, y, w, h); `"move_floating"` (panel, x, y, w, h); `"dock"` (panel,
//! stack, zone); `"focus_stack"` (stack); `"key"` (name, modifiers):
//! Ctrl+Page_Down and Ctrl+Page_Up walk the focused stack's tabs, Ctrl+W
//! closes its panel, Ctrl+Shift+M maximises it, F6 and Shift+F6 move
//! between stacks, Escape cancels a drag or restores a maximised panel.
//!
//! Signals: `layout_changed` (tree, floating), `activated` (panel),
//! `closed` (panel), `maximized` (panel or ""), `focus_changed` (stack).

mod tree;
mod archetype;

use std::collections::BTreeMap;
use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{number, text};
use crate::Effects;

use tree::{Node, Zone, insert_beside};

fn list(values: Vec<IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::List(values)))
}

fn field<'a>(value: &'a IpcValue, name: &str) -> Option<&'a IpcValue> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::Map(map) => map.get(name),
            IpcTable::List(_) => None,
        },
        _ => None,
    }
}

fn entries(value: Option<&IpcValue>) -> Vec<IpcValue> {
    match value {
        Some(IpcValue::Table(t)) => match t.as_ref() {
            IpcTable::List(items) => items.clone(),
            IpcTable::Map(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn words(value: Option<&IpcValue>) -> Vec<String> {
    entries(value).iter().filter_map(|v| text(Some(v)).map(str::to_owned)).collect()
}

#[derive(Clone, Debug, PartialEq)]
struct Floating {
    panel: String,
    rect: [f64; 4],
}

pub(crate) struct Dock {
    pub(crate) base: ControlState,
    root: Option<Node>,
    floating: Vec<Floating>,
    fixed: Vec<String>,
    edge: f64,
    min_ratio: f64,
    next_id: u64,
    focused: String,
    maximized: String,
    dragging: String,
    drop: Option<(String, Zone)>,
}

impl Dock {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            root: None,
            floating: Vec::new(),
            fixed: Vec::new(),
            edge: 0.25,
            min_ratio: 0.08,
            next_id: 0,
            focused: String::new(),
            maximized: String::new(),
            dragging: String::new(),
            drop: None,
        }
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    fn parse(&mut self, value: &IpcValue) -> Option<Node> {
        let id = text(field(value, "id")).map(str::to_owned);
        if let Some(children) = field(value, "children") {
            let children: Vec<Node> = entries(Some(children)).iter().filter_map(|c| self.parse(c)).collect();
            if children.is_empty() {
                return None;
            }
            let n = children.len();
            let mut ratios: Vec<f64> = entries(field(value, "ratios")).iter().filter_map(|v| number(Some(v))).collect();
            if ratios.len() != n || ratios.iter().sum::<f64>() <= 0.0 {
                ratios = vec![1.0 / n as f64; n];
            }
            let sum: f64 = ratios.iter().sum();
            let ratios = ratios.iter().map(|r| r / sum).collect();
            let vertical = text(field(value, "orientation")) == Some("vertical");
            let id = id.unwrap_or_else(|| self.fresh("split"));
            return Some(Node::Split { id, vertical, ratios, children });
        }
        let panels = words(field(value, "panels"));
        let current = text(field(value, "current")).and_then(|c| panels.iter().position(|p| p == c)).unwrap_or(0);
        let id = id.unwrap_or_else(|| self.fresh("stack"));
        Some(Node::Stack { id, panels, current })
    }

    fn stacks(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.stacks(&mut out);
        }
        out
    }

    fn current_of(&self, stack: &str) -> String {
        self.stacks()
            .into_iter()
            .find_map(|s| match s {
                Node::Stack { id, panels, current } if id == stack => panels.get(*current).cloned(),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn panel_count(&self) -> usize {
        let docked: usize = self
            .stacks()
            .iter()
            .map(|s| match s {
                Node::Stack { panels, .. } => panels.len(),
                Node::Split { .. } => 0,
            })
            .sum();
        docked + self.floating.len()
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        let (target, zone) = match &self.drop {
            Some((t, z)) => (t.clone(), z.name()),
            None => (String::new(), "center"),
        };
        vec![
            ("focused".into(), self.focused.as_str().into()),
            ("focused_panel".into(), self.current_of(&self.focused).into()),
            ("maximized".into(), self.maximized.as_str().into()),
            ("dragging".into(), self.dragging.as_str().into()),
            ("drop_target".into(), target.into()),
            ("drop_zone".into(), zone.into()),
            ("panel_count".into(), (self.panel_count() as i64).into()),
        ]
    }

    fn layout_signal(&self) -> Vec<IpcValue> {
        let tree = self.root.as_ref().map(Node::to_ipc).unwrap_or(IpcValue::Nil);
        let floating = list(
            self.floating
                .iter()
                .map(|f| {
                    let mut map = BTreeMap::new();
                    map.insert("panel".into(), f.panel.as_str().into());
                    for (k, v) in ["x", "y", "w", "h"].iter().zip(f.rect) {
                        map.insert((*k).into(), v.into());
                    }
                    IpcValue::Table(Arc::new(IpcTable::Map(map)))
                })
                .collect(),
        );
        vec![tree, floating]
    }

    /// Runs `change`, then says what moved: the fields that differ, the
    /// layout when the tree or the floating panels changed, the focus.
    fn changing(&mut self, change: impl FnOnce(&mut Self, &mut Effects)) -> Effects {
        let before = self.fields();
        let (tree, floating, focused, maximized) =
            (self.root.clone(), self.floating.clone(), self.focused.clone(), self.maximized.clone());
        let mut inner = Effects::default();
        change(self, &mut inner);
        // A focused stack that has gone hands the focus to the first.
        if !self.stacks().iter().any(|s| s.id() == self.focused) {
            self.focused = self.stacks().first().map(|s| s.id().to_owned()).unwrap_or_default();
        }
        if !self.maximized.is_empty() && self.root.as_ref().and_then(|r| r.stack_of(&self.maximized)).is_none() {
            self.maximized.clear();
        }
        let mut out = Effects::default();
        for (name, value) in self.fields() {
            if before.iter().find(|(n, _)| *n == name).map(|(_, v)| v) != Some(&value) {
                out.set(&name, value);
            }
        }
        if self.root != tree || self.floating != floating {
            out.raise("layout_changed", self.layout_signal());
        }
        if self.focused != focused {
            out.raise("focus_changed", vec![self.focused.as_str().into()]);
        }
        if self.maximized != maximized {
            out.raise("maximized", vec![self.maximized.as_str().into()]);
        }
        out.signals.extend(inner.signals);
        out.handled = inner.handled;
        out
    }

    fn take(&mut self, panel: &str) -> bool {
        if let Some(i) = self.floating.iter().position(|f| f.panel == panel) {
            self.floating.remove(i);
            return true;
        }
        let Some(mut root) = self.root.take() else { return false };
        let removed = root.remove_panel(panel);
        self.root = root.tidy();
        removed
    }

    fn activate(&mut self, panel: &str, effects: &mut Effects) {
        let Some(stack) = self.root.as_ref().and_then(|r| r.stack_of(panel)).map(str::to_owned) else {
            return;
        };
        if let Some(Node::Stack { panels, current, .. }) = self.root.as_mut().and_then(|r| r.find_mut(&stack)) {
            if let Some(i) = panels.iter().position(|p| p == panel) {
                *current = i;
            }
        }
        self.focused = stack;
        effects.raise("activated", vec![panel.into()]);
    }

    fn dock(&mut self, panel: &str, target: &str, zone: Zone, effects: &mut Effects) {
        // Onto its own stack's middle, a panel stays where it is.
        let home = self.root.as_ref().and_then(|r| r.stack_of(panel)).map(str::to_owned);
        if home.as_deref() == Some(target) {
            let alone = self.stacks().iter().any(|s| matches!(s, Node::Stack { id, panels, .. } if id == target && panels.len() == 1));
            if zone == Zone::Center || alone {
                return;
            }
        }
        self.take(panel);
        let stack_id = self.fresh("stack");
        let new = Node::Stack { id: stack_id.clone(), panels: vec![panel.to_owned()], current: 0 };
        match self.root.take() {
            None => self.root = Some(new),
            Some(mut root) => {
                let target_exists = root.find_mut(target).is_some();
                if zone == Zone::Center && target_exists {
                    if let Some(Node::Stack { panels, current, .. }) = root.find_mut(target) {
                        panels.push(panel.to_owned());
                        *current = panels.len() - 1;
                    }
                    self.root = Some(root);
                } else {
                    let target = if target_exists { target.to_owned() } else { root.id().to_owned() };
                    let zone = if zone == Zone::Center { Zone::Right } else { zone };
                    let mut counter = self.next_id;
                    let mut fresh = || {
                        counter += 1;
                        format!("split{counter}")
                    };
                    let result = insert_beside(&mut root, &target, new, zone, &mut fresh);
                    self.next_id = counter;
                    if let Err(new) = result {
                        // (No such place: beside everything.)
                        self.next_id += 1;
                        root = Node::Split {
                            id: format!("split{}", self.next_id),
                            vertical: false,
                            ratios: vec![0.5, 0.5],
                            children: vec![root, new],
                        };
                    }
                    self.root = Some(root);
                }
            }
        }
        self.activate(panel, effects);
    }

    fn close(&mut self, panel: &str, effects: &mut Effects) {
        if self.fixed.iter().any(|f| f == panel) {
            return;
        }
        if self.take(panel) {
            effects.raise("closed", vec![panel.into()]);
        }
    }

    fn walk_tabs(&mut self, back: bool) -> bool {
        let focused = self.focused.clone();
        let Some(Node::Stack { panels, current, .. }) = self.root.as_mut().and_then(|r| r.find_mut(&focused)) else {
            return false;
        };
        if panels.len() < 2 {
            return false;
        }
        let n = panels.len();
        *current = if back { (*current + n - 1) % n } else { (*current + 1) % n };
        true
    }

    fn cycle_stacks(&mut self, back: bool) -> bool {
        let ids: Vec<String> = self.stacks().iter().map(|s| s.id().to_owned()).collect();
        if ids.len() < 2 {
            return false;
        }
        let n = ids.len();
        let at = ids.iter().position(|i| *i == self.focused).unwrap_or(0);
        self.focused = ids[if back { (at + n - 1) % n } else { (at + 1) % n }].clone();
        true
    }

    fn zone_at(&self, x: f64, y: f64, w: f64, h: f64) -> Zone {
        let (fx, fy) = ((x / w.max(1.0)).clamp(0.0, 1.0), (y / h.max(1.0)).clamp(0.0, 1.0));
        let e = self.edge;
        if fx > e && fx < 1.0 - e && fy > e && fy < 1.0 - e {
            return Zone::Center;
        }
        let near = [(fx, Zone::Left), (1.0 - fx, Zone::Right), (fy, Zone::Top), (1.0 - fy, Zone::Bottom)];
        near.iter().min_by(|a, b| a.0.total_cmp(&b.0)).map(|n| n.1).unwrap_or(Zone::Center)
    }

    fn mirror(&self, zone: Zone) -> Zone {
        match (self.base.mirrored, zone) {
            (true, Zone::Left) => Zone::Right,
            (true, Zone::Right) => Zone::Left,
            (_, z) => z,
        }
    }
}

#[cfg(test)]
mod tests;
