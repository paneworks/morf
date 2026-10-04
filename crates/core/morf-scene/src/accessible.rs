//! What a screen reader is told about a scene: a tree of accessible nodes
//! built from the shown nodes that mean something -- a role given, text,
//! an editable field, a control that takes focus -- with the plain boxes
//! between them left out.
//!
//! Nodes carry `accessible_role`, `accessible_name`, `accessible_description`
//! and `accessible`, a table of the rest: `value` (and `minimum`, `maximum`,
//! `step`) and states (`checked`, `expanded`, `selected`, `disabled`,
//! `pressed`, `read_only`, `modal`, `orientation`, `placeholder`, `level`);
//! `accessible_hidden` takes a subtree out. Nothing here knows a platform:
//! a backend (AccessKit, in `morf-app`) turns [`AccessibleNode`]s into
//! its own, and only while a screen reader asks for them.

use crate::types::{Element, NodeHandle, Scene, Value};

/// Every role a node may name, the engine's vocabulary.
pub const ROLES: &[&str] = &[
    "", "window", "application", "dialog", "alert_dialog", "alert", "status", "tooltip", "menu", "menu_bar",
    "menu_item", "menu_item_check", "menu_item_radio", "button", "toggle_button", "check_box", "radio_button",
    "switch", "link", "slider", "spin_button", "scroll_bar", "progress", "meter", "tab_list", "tab", "tab_panel",
    "list_box", "list_box_option", "radio_group", "tree", "tree_item", "tree_grid", "grid", "grid_cell", "list",
    "list_item", "table", "row", "cell", "column_header", "row_header", "text_field", "password_text",
    "text_area", "search_field", "scroll_pane", "group", "splitter", "grip", "label", "heading", "image",
    "separator", "toolbar", "navigation", "main", "complementary", "region", "banner", "content_info", "search",
    "form", "figure", "generic", "time", "timer", "marquee", "log", "note", "paragraph", "document",
];

/// Roles whose children are presentational: the node is read as one thing,
/// named from the text under it.
fn is_leaf(role: &str) -> bool {
    matches!(
        role,
        "button" | "toggle_button" | "check_box" | "radio_button" | "switch" | "link" | "menu_item"
            | "menu_item_check" | "menu_item_radio" | "slider" | "spin_button" | "scroll_bar" | "progress"
            | "meter" | "tab" | "text_field" | "password_text" | "text_area" | "search_field" | "label"
            | "heading" | "image" | "splitter" | "grip" | "separator" | "time" | "timer"
    )
}

/// Roles named from their content when not named outright, children kept.
fn named_by_content(role: &str) -> bool {
    matches!(
        role,
        "list_item" | "list_box_option" | "grid_cell" | "tree_item" | "row" | "cell" | "column_header"
            | "row_header" | "status" | "alert" | "tooltip" | "note" | "paragraph"
    )
}

/// The checked state of a check box, switch or toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    False,
    True,
    Mixed,
}

/// A value read out: a number on a range, text in a field.
#[derive(Clone, Debug, PartialEq)]
pub enum AccessibleValue {
    Number(f64),
    Text(String),
}

/// One node of the accessible tree.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibleNode {
    pub node: NodeHandle,
    pub role: String,
    pub name: String,
    pub description: String,
    pub value: Option<AccessibleValue>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub step: Option<f64>,
    pub checked: Option<Checked>,
    pub expanded: Option<bool>,
    pub selected: Option<bool>,
    pub disabled: bool,
    pub pressed: bool,
    pub read_only: bool,
    pub modal: bool,
    pub focusable: bool,
    pub focused: bool,
    pub orientation: String,
    pub placeholder: String,
    pub level: Option<usize>,
    /// The box on the surface: x, y, width, height.
    pub bounds: Option<(f64, f64, f64, f64)>,
    pub children: Vec<NodeHandle>,
}

impl NodeHandle {
    /// The handle as a number that stays the node's while it lives (a
    /// screen reader's id for it).
    pub fn to_bits(self) -> u64 {
        slotmap::Key::data(&self.0).as_ffi()
    }

    /// The handle a number from [`NodeHandle::to_bits`] stands for.
    pub fn from_bits(bits: u64) -> Self {
        Self(slotmap::KeyData::from_ffi(bits).into())
    }
}

impl Scene {
    fn shown_here(&self, node: NodeHandle) -> bool {
        self.bool_value(node, "visible").unwrap_or(false)
            && self.number(node, "opacity").unwrap_or(1.0) > 0.0
            && !self.bool_value(node, "accessible_hidden").unwrap_or(false)
            && !self.is_exiting(node)
    }

    fn own_text(&self, node: NodeHandle) -> Option<String> {
        match self.element(node).ok()? {
            Element::Text => {
                let text = self.string_value(node, "text").ok()?.trim();
                (!text.is_empty()).then(|| text.to_owned())
            }
            _ => None,
        }
    }

    /// The text shown under `node`, joined: a control's name when it is
    /// given none.
    pub fn accessible_text_under(&self, node: NodeHandle) -> String {
        let mut words: Vec<String> = Vec::new();
        let mut pending = vec![node];
        while let Some(n) = pending.pop() {
            if !self.shown_here(n) {
                continue;
            }
            if let Some(text) = self.own_text(n) {
                words.push(text);
            }
            if let Ok(children) = self.children(n) {
                pending.extend(children.iter().rev().copied());
            }
        }
        words.join(" ")
    }

    /// The role a node plays, or "" when it is only a box: its own
    /// `accessible_role`, else what its element is.
    pub fn accessible_role(&self, node: NodeHandle) -> String {
        let own = self.string_value(node, "accessible_role").unwrap_or("");
        if !own.is_empty() {
            return own.to_owned();
        }
        match self.element(node) {
            Ok(Element::Text) if self.own_text(node).is_some() => "label".into(),
            Ok(Element::TextInput) => {
                if self.bool_value(node, "multiline").unwrap_or(false) {
                    "text_area".into()
                } else {
                    "text_field".into()
                }
            }
            Ok(Element::Image | Element::Icon)
                if !self.string_value(node, "accessible_name").unwrap_or("").is_empty() =>
            {
                "image".into()
            }
            // A pressable thing Tab reaches, that nobody named the role of.
            Ok(Element::MouseArea)
                if matches!(self.string_value(node, "focus_policy").unwrap_or(""), "tab" | "strong") =>
            {
                "button".into()
            }
            _ => String::new(),
        }
    }

    fn accessible_node(
        &self,
        node: NodeHandle,
        role: String,
        bounds: &dyn Fn(NodeHandle) -> Option<(f64, f64, f64, f64)>,
    ) -> AccessibleNode {
        let string = |p: &str| self.string_value(node, p).unwrap_or("").to_owned();
        let table = match self.current(node, "accessible") {
            Ok(Value::Map(map)) => Some(map),
            _ => None,
        };
        let entry = |k: &str| table.and_then(|m| m.get(k));
        let number = |k: &str| match entry(k) {
            Some(Value::Number(n)) => Some(*n),
            _ => None,
        };
        let flag = |k: &str| match entry(k) {
            Some(Value::Bool(b)) => Some(*b),
            _ => None,
        };
        let text = |k: &str| match entry(k) {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        let mut name = string("accessible_name");
        // (A range's text is its reading, not its name.)
        let ranged = matches!(role.as_str(), "slider" | "spin_button" | "scroll_bar" | "progress" | "meter");
        if name.is_empty() && !ranged && (is_leaf(&role) || named_by_content(&role)) {
            name = self.accessible_text_under(node);
        }
        if name.is_empty() && matches!(role.as_str(), "text_field" | "text_area" | "search_field" | "password_text") {
            name = self.string_value(node, "placeholder").unwrap_or("").to_owned();
        }
        let value = match entry("value") {
            Some(Value::Number(n)) => Some(AccessibleValue::Number(*n)),
            Some(Value::String(s)) => Some(AccessibleValue::Text(s.clone())),
            _ if self.element(node) == Ok(Element::TextInput) && role != "password_text" => self
                .string_value(node, "text")
                .ok()
                .map(|t| AccessibleValue::Text(t.to_owned())),
            _ => None,
        };
        let checked = match entry("checked") {
            Some(Value::Bool(true)) => Some(Checked::True),
            Some(Value::Bool(false)) => Some(Checked::False),
            Some(Value::String(s)) if s == "mixed" => Some(Checked::Mixed),
            _ => None,
        };
        let policy = self.string_value(node, "focus_policy").unwrap_or("auto");
        AccessibleNode {
            node,
            name,
            description: string("accessible_description"),
            value,
            minimum: number("minimum"),
            maximum: number("maximum"),
            step: number("step"),
            checked,
            expanded: flag("expanded"),
            selected: flag("selected"),
            disabled: flag("disabled").unwrap_or(false) || !self.bool_value(node, "enabled").unwrap_or(true),
            pressed: flag("pressed").unwrap_or(false),
            read_only: flag("read_only").unwrap_or(false),
            modal: flag("modal").unwrap_or(false),
            focusable: policy != "none"
                && (matches!(policy, "tab" | "strong" | "click") || self.element(node) == Ok(Element::TextInput)),
            focused: self.bool_value(node, "focused").unwrap_or(false),
            orientation: text("orientation"),
            placeholder: text("placeholder"),
            level: number("level").filter(|l| *l >= 1.0).map(|l| l as usize),
            bounds: bounds(node),
            children: Vec::new(),
            role,
        }
    }

    /// The accessible tree under `root`, root first, each node listing the
    /// accessible nodes under it. `root` is always in it, as `root_role`
    /// named `root_name` unless it says otherwise. `bounds` gives a node's
    /// box on the surface.
    pub fn accessible_tree(
        &self,
        root: NodeHandle,
        root_role: &str,
        root_name: &str,
        bounds: &dyn Fn(NodeHandle) -> Option<(f64, f64, f64, f64)>,
    ) -> Vec<AccessibleNode> {
        let mut out: Vec<AccessibleNode> = Vec::new();
        if !self.contains(root) {
            return out;
        }
        let role = match self.accessible_role(root) {
            r if r.is_empty() => root_role.to_owned(),
            r => r,
        };
        let mut top = self.accessible_node(root, role, bounds);
        if top.name.is_empty() {
            top.name = root_name.to_owned();
        }
        out.push(top);
        // (node, index in `out` of the accessible node it goes under)
        let mut pending: Vec<(NodeHandle, usize)> = Vec::new();
        if let Ok(children) = self.children(root) {
            pending.extend(children.iter().rev().map(|c| (*c, 0)));
        }
        while let Some((node, parent)) = pending.pop() {
            if !self.shown_here(node) {
                continue;
            }
            let role = self.accessible_role(node);
            let under = if role.is_empty() {
                parent
            } else {
                let leaf = is_leaf(&role);
                let item = self.accessible_node(node, role, bounds);
                out.push(item);
                let index = out.len() - 1;
                out[parent].children.push(node);
                if leaf {
                    continue;
                }
                index
            };
            if let Ok(children) = self.children(node) {
                pending.extend(children.iter().rev().map(|c| (*c, under)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{Element, Scene, Value};

    #[test]
    fn builds_controls_and_skips_boxes() {
        let mut scene = Scene::new();
        let root = scene.create(Element::Item);
        let panel = scene.create(Element::Item);
        let button = scene.create(Element::MouseArea);
        let label = scene.create(Element::Text);
        let hidden = scene.create(Element::Text);
        scene.reparent(panel, Some(root)).unwrap();
        scene.reparent(button, Some(panel)).unwrap();
        scene.reparent(label, Some(button)).unwrap();
        scene.reparent(hidden, Some(panel)).unwrap();
        scene.assign(button, "accessible_role", Value::String("button".into())).unwrap();
        scene.assign(label, "text", Value::String("Apply".into())).unwrap();
        scene.assign(hidden, "text", Value::String("secret".into())).unwrap();
        scene.assign(hidden, "visible", Value::Bool(false)).unwrap();
        let tree = scene.accessible_tree(root, "window", "Shell", &|_| None);
        assert_eq!(tree.len(), 2, "{tree:?}");
        assert_eq!(tree[0].role, "window");
        assert_eq!(tree[0].children, vec![button]);
        assert_eq!(tree[1].role, "button");
        assert_eq!(tree[1].name, "Apply");
        assert_eq!(super::NodeHandle::from_bits(button.to_bits()), button);
    }
}
