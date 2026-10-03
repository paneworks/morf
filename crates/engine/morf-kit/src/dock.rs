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

use std::collections::BTreeMap;
use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl Zone {
    fn name(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "center" => Self::Center,
            "left" => Self::Left,
            "right" => Self::Right,
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Split { id: String, vertical: bool, ratios: Vec<f64>, children: Vec<Node> },
    Stack { id: String, panels: Vec<String>, current: usize },
}

impl Node {
    fn id(&self) -> &str {
        match self {
            Node::Split { id, .. } | Node::Stack { id, .. } => id,
        }
    }

    fn stacks<'a>(&'a self, out: &mut Vec<&'a Node>) {
        match self {
            Node::Stack { .. } => out.push(self),
            Node::Split { children, .. } => children.iter().for_each(|c| c.stacks(out)),
        }
    }

    fn find_mut(&mut self, wanted: &str) -> Option<&mut Node> {
        if self.id() == wanted {
            return Some(self);
        }
        match self {
            Node::Split { children, .. } => children.iter_mut().find_map(|c| c.find_mut(wanted)),
            Node::Stack { .. } => None,
        }
    }

    /// The stack holding `panel`.
    fn stack_of(&self, panel: &str) -> Option<&str> {
        match self {
            Node::Stack { id, panels, .. } => panels.iter().any(|p| p == panel).then_some(id.as_str()),
            Node::Split { children, .. } => children.iter().find_map(|c| c.stack_of(panel)),
        }
    }

    fn remove_panel(&mut self, panel: &str) -> bool {
        match self {
            Node::Stack { panels, current, .. } => match panels.iter().position(|p| p == panel) {
                Some(i) => {
                    panels.remove(i);
                    if *current >= panels.len() || (i < *current) {
                        *current = current.saturating_sub(1);
                    }
                    true
                }
                None => false,
            },
            Node::Split { children, .. } => children.iter_mut().any(|c| c.remove_panel(panel)),
        }
    }

    /// Drops empty stacks and splits of one; `None` when nothing is left.
    fn tidy(self) -> Option<Node> {
        match self {
            Node::Stack { ref panels, .. } if panels.is_empty() => None,
            Node::Stack { .. } => Some(self),
            Node::Split { id, vertical, ratios, children } => {
                let mut kept = Vec::new();
                let mut kept_ratios = Vec::new();
                for (child, ratio) in children.into_iter().zip(ratios.into_iter().chain(std::iter::repeat(0.0))) {
                    if let Some(child) = child.tidy() {
                        kept.push(child);
                        kept_ratios.push(ratio);
                    }
                }
                match kept.len() {
                    0 => None,
                    1 => kept.pop(),
                    n => {
                        let sum: f64 = kept_ratios.iter().sum();
                        let ratios = if sum > 0.0 {
                            kept_ratios.iter().map(|r| r / sum).collect()
                        } else {
                            vec![1.0 / n as f64; n]
                        };
                        Some(Node::Split { id, vertical, ratios, children: kept })
                    }
                }
            }
        }
    }

    fn to_ipc(&self) -> IpcValue {
        let mut map = BTreeMap::new();
        match self {
            Node::Split { id, vertical, ratios, children } => {
                map.insert("id".into(), id.as_str().into());
                map.insert("kind".into(), "split".into());
                map.insert("orientation".into(), if *vertical { "vertical" } else { "horizontal" }.into());
                map.insert("ratios".into(), list(ratios.iter().map(|r| (*r).into()).collect()));
                map.insert("children".into(), list(children.iter().map(Node::to_ipc).collect()));
            }
            Node::Stack { id, panels, current } => {
                map.insert("id".into(), id.as_str().into());
                map.insert("kind".into(), "stack".into());
                map.insert("panels".into(), list(panels.iter().map(|p| p.as_str().into()).collect()));
                map.insert("current".into(), panels.get(*current).map(String::as_str).unwrap_or("").into());
            }
        }
        IpcValue::Table(Arc::new(IpcTable::Map(map)))
    }
}

/// Puts `new` beside the stack `target` on `zone`'s side: into the split
/// around it when that runs the same way, else a split of the two.
fn insert_beside(node: &mut Node, target: &str, new: Node, zone: Zone, fresh_id: &mut dyn FnMut() -> String) -> Result<(), Node> {
    let vertical = matches!(zone, Zone::Top | Zone::Bottom);
    let before = matches!(zone, Zone::Left | Zone::Top);
    if let Node::Split { vertical: v, ratios, children, .. } = node {
        if let Some(i) = children.iter().position(|c| c.id() == target) {
            if *v == vertical {
                let half = ratios[i] / 2.0;
                ratios[i] = half;
                let at = if before { i } else { i + 1 };
                children.insert(at, new);
                ratios.insert(at, half);
                return Ok(());
            }
            let old = std::mem::replace(&mut children[i], Node::Stack { id: String::new(), panels: Vec::new(), current: 0 });
            let pair = if before { vec![new, old] } else { vec![old, new] };
            children[i] = Node::Split { id: fresh_id(), vertical, ratios: vec![0.5, 0.5], children: pair };
            return Ok(());
        }
        let mut new = new;
        for child in children.iter_mut() {
            match insert_beside(child, target, new, zone, fresh_id) {
                Ok(()) => return Ok(()),
                Err(back) => new = back,
            }
        }
        return Err(new);
    }
    if node.id() == target {
        let old = std::mem::replace(node, Node::Stack { id: String::new(), panels: Vec::new(), current: 0 });
        let pair = if before { vec![new, old] } else { vec![old, new] };
        *node = Node::Split { id: fresh_id(), vertical, ratios: vec![0.5, 0.5], children: pair };
        return Ok(());
    }
    Err(new)
}

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

impl Archetype for Dock {
    fn name(&self) -> &'static str {
        "Dock"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let word = |i: usize| text(arguments.get(i)).unwrap_or("").to_owned();
        let n = |i: usize| number(arguments.get(i)).unwrap_or(0.0);
        Ok(match event {
            "activate" => {
                let panel = word(0);
                self.changing(|s, e| s.activate(&panel, e))
            }
            "close" => {
                let panel = word(0);
                self.changing(|s, e| s.close(&panel, e))
            }
            "drag_start" => {
                let panel = word(0);
                self.changing(|s, _| {
                    s.dragging = panel;
                    s.drop = None;
                })
            }
            "drag_over" => {
                let stack = word(0);
                let (x, y, w, h) = (n(1), n(2), n(3), n(4));
                self.changing(|s, _| {
                    if !s.dragging.is_empty() {
                        let zone = s.mirror(s.zone_at(x, y, w, h));
                        s.drop = Some((stack, zone));
                    }
                })
            }
            "drag_outside" => self.changing(|s, _| {
                if !s.dragging.is_empty() {
                    s.drop = Some(("float".into(), Zone::Center));
                }
            }),
            "drop" => {
                let rect = [n(0), n(1), n(2).max(120.0), n(3).max(80.0)];
                self.changing(|s, e| {
                    let panel = std::mem::take(&mut s.dragging);
                    match s.drop.take() {
                        Some((target, _)) if target == "float" => {
                            s.take(&panel);
                            s.floating.push(Floating { panel: panel.clone(), rect });
                        }
                        Some((target, zone)) if !panel.is_empty() => s.dock(&panel, &target, zone, e),
                        _ => {}
                    }
                })
            }
            "drag_cancel" => self.changing(|s, _| {
                s.dragging.clear();
                s.drop = None;
            }),
            "resize" => {
                let (split, index, position) = (word(0), n(1).max(0.0) as usize, n(2).clamp(0.0, 1.0));
                let min = self.min_ratio;
                self.changing(|s, _| {
                    if let Some(Node::Split { ratios, .. }) = s.root.as_mut().and_then(|r| r.find_mut(&split))
                        && index + 1 < ratios.len()
                    {
                        let start: f64 = ratios[..index].iter().sum();
                        let pair = ratios[index] + ratios[index + 1];
                        let first = (position - start).clamp(min.min(pair / 2.0), (pair - min).max(pair / 2.0));
                        ratios[index] = first;
                        ratios[index + 1] = pair - first;
                    }
                })
            }
            "maximize" => {
                let panel = word(0);
                self.changing(|s, e| {
                    if s.maximized == panel {
                        s.maximized.clear();
                    } else {
                        s.activate(&panel, e);
                        s.maximized = panel;
                    }
                })
            }
            "float" => {
                let panel = word(0);
                let rect = [n(1), n(2), n(3).max(120.0), n(4).max(80.0)];
                self.changing(|s, _| {
                    if s.take(&panel) {
                        s.floating.push(Floating { panel, rect });
                    }
                })
            }
            "move_floating" => {
                let panel = word(0);
                let rect = [n(1), n(2), n(3).max(120.0), n(4).max(80.0)];
                self.changing(|s, _| {
                    if let Some(f) = s.floating.iter_mut().find(|f| f.panel == panel) {
                        f.rect = rect;
                    }
                })
            }
            "dock" => {
                let (panel, stack) = (word(0), word(1));
                let zone = Zone::parse(&word(2)).unwrap_or(Zone::Center);
                self.changing(|s, e| s.dock(&panel, &stack, zone, e))
            }
            "focus_stack" => {
                let stack = word(0);
                self.changing(|s, _| s.focused = stack)
            }
            "key" if self.base.enabled => {
                let (name, modifiers) = (word(0), word(1));
                let ctrl = modifiers.contains("ctrl");
                let shift = modifiers.contains("shift");
                let mut used = true;
                let mut effects = self.changing(|s, e| {
                    used = match name.as_str() {
                        "Page_Down" if ctrl => s.walk_tabs(false),
                        "Page_Up" if ctrl => s.walk_tabs(true),
                        "w" | "W" if ctrl => {
                            let panel = s.current_of(&s.focused);
                            s.close(&panel, e);
                            !panel.is_empty()
                        }
                        "m" | "M" if ctrl && shift => {
                            let panel = s.current_of(&s.focused);
                            if s.maximized == panel {
                                s.maximized.clear();
                            } else {
                                s.maximized = panel;
                            }
                            true
                        }
                        "F6" => s.cycle_stacks(shift),
                        "Escape" if !s.dragging.is_empty() => {
                            s.dragging.clear();
                            s.drop = None;
                            true
                        }
                        "Escape" if !s.maximized.is_empty() => {
                            s.maximized.clear();
                            true
                        }
                        _ => false,
                    };
                });
                effects.handled = used;
                effects
            }
            "clicked" | "key" => Effects::default(),
            _ => self.base.handle(event, arguments).unwrap_or_default(),
        })
    }

    fn configure(&mut self, field_name: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field_name, value) {
            return result;
        }
        Ok(match field_name {
            "layout" => {
                let root = self.parse(value);
                self.changing(|s, _| s.root = root.and_then(Node::tidy))
            }
            "floating" => {
                let floating: Vec<Floating> = entries(Some(value))
                    .iter()
                    .filter_map(|f| {
                        let g = |k: &str| number(field(f, k)).unwrap_or(0.0);
                        Some(Floating {
                            panel: text(field(f, "panel"))?.to_owned(),
                            rect: [g("x"), g("y"), g("w").max(120.0), g("h").max(80.0)],
                        })
                    })
                    .collect();
                self.changing(|s, _| s.floating = floating)
            }
            "fixed" => {
                self.fixed = words(Some(value));
                Effects::default()
            }
            "edge" => {
                self.edge = expect_number(Some(value), field_name)?.clamp(0.05, 0.45);
                Effects::default()
            }
            "min_ratio" => {
                self.min_ratio = expect_number(Some(value), field_name)?.clamp(0.0, 0.45);
                Effects::default()
            }
            _ => return Err(format!("Dock has no setting `{field_name}`")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, IpcValue)]) -> IpcValue {
        IpcValue::Table(Arc::new(IpcTable::Map(entries.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect())))
    }

    fn strs(items: &[&str]) -> IpcValue {
        list(items.iter().map(|s| (*s).into()).collect())
    }

    fn dock() -> Dock {
        let mut d = Dock::new();
        let layout = map(&[
            ("orientation", "horizontal".into()),
            ("ratios", list(vec![0.25.into(), 0.75.into()])),
            (
                "children",
                list(vec![
                    map(&[("id", "left".into()), ("panels", strs(&["files", "search"]))]),
                    map(&[("id", "main".into()), ("panels", strs(&["editor"]))]),
                ]),
            ),
        ]);
        d.configure("layout", &layout).unwrap();
        d
    }

    fn signal<'a>(effects: &'a Effects, name: &str) -> Option<&'a Vec<IpcValue>> {
        effects.signals.iter().find(|(n, _)| n == name).map(|(_, a)| a)
    }

    #[test]
    fn a_tab_dropped_on_an_edge_splits_and_in_the_middle_joins() {
        let mut d = dock();
        d.handle("drag_start", &["search".into()]).unwrap();
        d.handle("drag_over", &["main".into(), 10.0.into(), 300.0.into(), 800.0.into(), 600.0.into()]).unwrap();
        assert_eq!(d.drop, Some(("main".into(), Zone::Left)));
        let e = d.handle("drop", &[]).unwrap();
        assert!(signal(&e, "layout_changed").is_some());
        // The root split runs the same way, so the new stack joins it.
        let Some(Node::Split { children, ratios, .. }) = &d.root else { panic!() };
        assert_eq!(children.len(), 3);
        assert!((ratios.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        // Into the middle of another stack, it becomes a tab there; the
        // stack it left, empty, goes.
        d.handle("drag_start", &["search".into()]).unwrap();
        d.handle("drag_over", &["main".into(), 400.0.into(), 300.0.into(), 800.0.into(), 600.0.into()]).unwrap();
        d.handle("drop", &[]).unwrap();
        let Some(Node::Split { children, .. }) = &d.root else { panic!() };
        assert_eq!(children.len(), 2);
        assert_eq!(d.root.as_ref().unwrap().stack_of("search"), Some("main"));
    }

    #[test]
    fn closing_the_last_panel_of_a_stack_folds_the_split() {
        let mut d = dock();
        d.configure("fixed", &strs(&["editor"])).unwrap();
        d.handle("close", &["editor".into()]).unwrap();
        assert!(d.root.as_ref().unwrap().stack_of("editor").is_some());
        d.handle("close", &["files".into()]).unwrap();
        let e = d.handle("close", &["search".into()]).unwrap();
        assert!(signal(&e, "closed").is_some());
        assert!(matches!(d.root, Some(Node::Stack { .. })));
        assert_eq!(d.focused, "main");
    }

    #[test]
    fn keys_walk_tabs_and_stacks_and_float_round_trips() {
        let mut d = dock();
        d.handle("focus_stack", &["left".into()]).unwrap();
        let e = d.handle("key", &["Page_Down".into(), "ctrl".into()]).unwrap();
        assert!(e.handled);
        assert_eq!(d.current_of("left"), "search");
        d.handle("key", &["F6".into(), "".into()]).unwrap();
        assert_eq!(d.focused, "main");
        d.handle("float", &["files".into(), 10.0.into(), 10.0.into(), 300.0.into(), 200.0.into()]).unwrap();
        assert_eq!(d.floating.len(), 1);
        d.handle("dock", &["files".into(), "main".into(), "bottom".into()]).unwrap();
        assert!(d.floating.is_empty());
        assert_eq!(d.panel_count(), 3);
        d.handle("resize", &["split1".into(), 0.into(), 0.4.into()]).unwrap();
    }
}
