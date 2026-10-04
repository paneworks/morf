//! A surface's accessible tree on AT-SPI, through AccessKit.
//!
//! One [`Accessibility`] per surface. It builds nothing until a screen
//! reader asks: AccessKit's adapter says so from its own thread, which sets
//! a flag and wakes the surface's loop; the loop then asks
//! [`Accessibility::wants_tree`] once a frame and, while it is wanted,
//! hands [`Accessibility::update`] the scene's tree (`morf_scene`'s
//! accessible nodes). Each update sends only the nodes that changed since
//! the last -- one node per change -- and the focus.
//!
//! A screen reader's requests (focus this, press that, set a value) come
//! back as [`Request`]s for the loop to carry out with the node's own
//! behaviour, so a control acted on by Orca does exactly what a key would.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use accesskit::{
    Action, ActionData, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId,
    Orientation, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate,
};
use morf_value::accessible::{AccessibleNode, AccessibleValue, Checked};

/// What a screen reader asked of a node.
#[derive(Clone, Debug, PartialEq)]
pub enum RequestKind {
    Focus,
    /// Press it: a button's click, a check box's toggle.
    Click,
    Increment,
    Decrement,
    Expand,
    Collapse,
    SetNumber(f64),
    SetText(String),
    ScrollIntoView,
}

/// A request for one node.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The node, as the number its id converted to in [`Accessibility::update`].
    pub node: u64,
    pub kind: RequestKind,
}

struct Activation {
    wanted: Arc<AtomicBool>,
    fresh: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

/// `MORF_A11Y_LOG`: says on stderr when a screen reader comes and goes.
fn trace(what: &str) {
    if std::env::var_os("MORF_A11Y_LOG").is_some() {
        eprintln!("morf: a11y: {what}");
    }
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        trace("a screen reader asked for the tree");
        // Built on the surface's own thread, at its next turn.
        self.wanted.store(true, Ordering::SeqCst);
        self.fresh.store(true, Ordering::SeqCst);
        (self.wake)();
        None
    }
}

struct Deactivation {
    wanted: Arc<AtomicBool>,
}

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        trace("the screen reader went away");
        self.wanted.store(false, Ordering::SeqCst);
    }
}

struct Actions {
    requests: Mutex<Sender<Request>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        let kind = match (request.action, request.data) {
            (Action::Focus, _) => RequestKind::Focus,
            (Action::Click, _) => RequestKind::Click,
            (Action::Increment, _) => RequestKind::Increment,
            (Action::Decrement, _) => RequestKind::Decrement,
            (Action::Expand, _) => RequestKind::Expand,
            (Action::Collapse, _) => RequestKind::Collapse,
            (Action::ScrollIntoView, _) => RequestKind::ScrollIntoView,
            (Action::SetValue, Some(ActionData::NumericValue(n))) => RequestKind::SetNumber(n),
            (Action::SetValue, Some(ActionData::Value(text))) => RequestKind::SetText(text.into()),
            _ => return,
        };
        let request = Request { node: request.target_node.0, kind };
        if let Ok(sender) = self.requests.lock() {
            let _ = sender.send(request);
        }
        (self.wake)();
    }
}

/// A surface's adapter, and what it last told the screen reader.
pub struct Accessibility {
    adapter: accesskit_unix::Adapter,
    wanted: Arc<AtomicBool>,
    fresh: Arc<AtomicBool>,
    requests: Receiver<Request>,
    sent: HashMap<u64, Node>,
    root: Option<u64>,
    focus: Option<u64>,
}

impl Accessibility {
    /// An adapter for one surface; `wake` turns the surface's loop.
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let wanted = Arc::new(AtomicBool::new(false));
        let fresh = Arc::new(AtomicBool::new(false));
        let (sender, requests) = channel();
        let adapter = accesskit_unix::Adapter::new(
            Activation { wanted: Arc::clone(&wanted), fresh: Arc::clone(&fresh), wake: Arc::clone(&wake) },
            Actions { requests: Mutex::new(sender), wake },
            Deactivation { wanted: Arc::clone(&wanted) },
        );
        trace("an adapter for a surface");
        Self { adapter, wanted, fresh, requests, sent: HashMap::new(), root: None, focus: None }
    }

    /// Whether a screen reader wants the tree: false until one asks, so a
    /// desk with none builds nothing.
    pub fn wants_tree(&self) -> bool {
        self.wanted.load(Ordering::SeqCst)
    }

    /// Whether a screen reader has just asked afresh: the whole tree goes
    /// next, not what changed.
    pub fn is_fresh(&self) -> bool {
        self.fresh.load(Ordering::SeqCst)
    }

    /// Whether the surface has the keyboard.
    pub fn set_window_focused(&mut self, focused: bool) {
        self.adapter.update_window_focus_state(focused);
    }

    /// The screen reader's requests since the last call.
    pub fn take_requests(&self) -> Vec<Request> {
        self.requests.try_iter().collect()
    }

    /// Tells the screen reader the tree as it is now: `nodes` root first
    /// (`morf_scene::Scene::accessible_tree`). Sends what changed.
    pub fn update<Id: Copy + Into<u64>>(&mut self, nodes: &[AccessibleNode<Id>]) {
        let Some(first) = nodes.first() else { return };
        if self.fresh.swap(false, Ordering::SeqCst) {
            self.sent.clear();
            self.root = None;
        }
        let root = first.node.into();
        let focus = nodes.iter().find(|n| n.focused).map(|n| n.node.into()).unwrap_or(root);
        let mut changed: Vec<(NodeId, Node)> = Vec::new();
        let mut next: HashMap<u64, Node> = HashMap::with_capacity(nodes.len());
        for item in nodes {
            let id: u64 = item.node.into();
            let node = convert(item);
            if self.sent.get(&id) != Some(&node) {
                changed.push((NodeId(id), node.clone()));
            }
            next.insert(id, node);
        }
        let new_root = self.root != Some(root);
        if changed.is_empty() && !new_root && self.focus == Some(focus) {
            return;
        }
        trace(&format!("{} nodes, {} changed", nodes.len(), changed.len()));
        self.sent = next;
        self.root = Some(root);
        self.focus = Some(focus);
        self.adapter.update_if_active(|| TreeUpdate {
            nodes: changed,
            tree: new_root.then(|| {
                let mut info = TreeInfo::new(NodeId(root));
                info.toolkit_name = Some("morf".into());
                info.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
                info
            }),
            tree_id: TreeId::ROOT,
            focus: NodeId(focus),
        });
    }
}

/// The AccessKit role for an engine role.
pub fn role(name: &str) -> Role {
    match name {
        "window" => Role::Window,
        "application" => Role::Application,
        "dialog" => Role::Dialog,
        "alert_dialog" => Role::AlertDialog,
        "alert" => Role::Alert,
        "status" => Role::Status,
        "tooltip" => Role::Tooltip,
        "menu" => Role::Menu,
        "menu_bar" => Role::MenuBar,
        "menu_item" => Role::MenuItem,
        "menu_item_check" => Role::MenuItemCheckBox,
        "menu_item_radio" => Role::MenuItemRadio,
        "button" => Role::Button,
        "toggle_button" => Role::Button,
        "check_box" => Role::CheckBox,
        "radio_button" => Role::RadioButton,
        "switch" => Role::Switch,
        "link" => Role::Link,
        "slider" => Role::Slider,
        "spin_button" => Role::SpinButton,
        "scroll_bar" => Role::ScrollBar,
        "progress" => Role::ProgressIndicator,
        "meter" => Role::Meter,
        "tab_list" => Role::TabList,
        "tab" => Role::Tab,
        "tab_panel" => Role::TabPanel,
        "list_box" => Role::ListBox,
        "list_box_option" => Role::ListBoxOption,
        "radio_group" => Role::RadioGroup,
        "tree" => Role::Tree,
        "tree_item" => Role::TreeItem,
        "tree_grid" => Role::TreeGrid,
        "grid" => Role::Grid,
        "grid_cell" => Role::GridCell,
        "list" => Role::List,
        "list_item" => Role::ListItem,
        "table" => Role::Table,
        "row" => Role::Row,
        "cell" => Role::Cell,
        "column_header" => Role::ColumnHeader,
        "row_header" => Role::RowHeader,
        "text_field" => Role::TextInput,
        "password_text" => Role::PasswordInput,
        "text_area" => Role::MultilineTextInput,
        "search_field" => Role::SearchInput,
        "scroll_pane" => Role::ScrollView,
        "group" => Role::Group,
        "splitter" => Role::Splitter,
        "grip" => Role::Splitter,
        "label" => Role::Label,
        "heading" => Role::Heading,
        "image" => Role::Image,
        "separator" => Role::Splitter,
        "toolbar" => Role::Toolbar,
        "navigation" => Role::Navigation,
        "main" => Role::Main,
        "complementary" => Role::Complementary,
        "region" => Role::Region,
        "banner" => Role::Banner,
        "content_info" => Role::ContentInfo,
        "search" => Role::Search,
        "form" => Role::Form,
        "figure" => Role::Figure,
        "time" => Role::Time,
        "timer" => Role::Timer,
        "marquee" => Role::Marquee,
        "log" => Role::Log,
        "note" => Role::Note,
        "paragraph" => Role::Paragraph,
        "document" => Role::Document,
        _ => Role::GenericContainer,
    }
}

/// An engine node as AccessKit's.
pub fn convert<Id: Copy + Into<u64>>(item: &AccessibleNode<Id>) -> Node {
    let mut node = Node::new(role(&item.role));
    // A label's text is its value to AccessKit, which names it from that.
    if item.role == "label" && item.value.is_none() {
        node.set_value(item.name.as_str());
    } else if !item.name.is_empty() {
        node.set_label(item.name.as_str());
    }
    if !item.description.is_empty() {
        node.set_description(item.description.as_str());
    }
    match &item.value {
        Some(AccessibleValue::Number(n)) => node.set_numeric_value(*n),
        Some(AccessibleValue::Text(t)) => node.set_value(t.as_str()),
        None => {}
    }
    if let Some(n) = item.minimum {
        node.set_min_numeric_value(n);
    }
    if let Some(n) = item.maximum {
        node.set_max_numeric_value(n);
    }
    if let Some(n) = item.step {
        node.set_numeric_value_step(n);
    }
    match item.checked {
        Some(Checked::True) => node.set_toggled(Toggled::True),
        Some(Checked::False) => node.set_toggled(Toggled::False),
        Some(Checked::Mixed) => node.set_toggled(Toggled::Mixed),
        None if item.role == "toggle_button" => {
            node.set_toggled(if item.pressed { Toggled::True } else { Toggled::False })
        }
        None => {}
    }
    if let Some(e) = item.expanded {
        node.set_expanded(e);
        node.add_action(if e { Action::Collapse } else { Action::Expand });
    }
    if let Some(s) = item.selected {
        node.set_selected(s);
    }
    if item.disabled {
        node.set_disabled();
    }
    if item.read_only {
        node.set_read_only();
    }
    if item.modal {
        node.set_modal();
    }
    match item.orientation.as_str() {
        "horizontal" => node.set_orientation(Orientation::Horizontal),
        "vertical" => node.set_orientation(Orientation::Vertical),
        _ => {}
    }
    if !item.placeholder.is_empty() {
        node.set_placeholder(item.placeholder.as_str());
    }
    if let Some(level) = item.level {
        node.set_level(level);
    }
    if let Some((x, y, w, h)) = item.bounds {
        node.set_bounds(Rect { x0: x, y0: y, x1: x + w, y1: y + h });
    }
    if item.focusable && !item.disabled {
        node.add_action(Action::Focus);
    }
    if !item.disabled {
        match item.role.as_str() {
            "button" | "toggle_button" | "check_box" | "radio_button" | "switch" | "link" | "menu_item"
            | "menu_item_check" | "menu_item_radio" | "tab" | "list_box_option" | "list_item" | "tree_item"
            | "grid_cell" => node.add_action(Action::Click),
            "slider" | "spin_button" | "scroll_bar" => {
                node.add_action(Action::Increment);
                node.add_action(Action::Decrement);
                node.add_action(Action::SetValue);
            }
            "text_field" | "text_area" | "search_field" | "password_text" if !item.read_only => {
                node.add_action(Action::SetValue)
            }
            _ => {}
        }
    }
    node.set_children(item.children.iter().map(|c| NodeId((*c).into())).collect::<Vec<_>>());
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(role: &str, name: &str) -> AccessibleNode<u64> {
        AccessibleNode {
            node: 1 | (1 << 32),
            role: role.into(),
            name: name.into(),
            description: String::new(),
            value: None,
            minimum: None,
            maximum: None,
            step: None,
            checked: None,
            expanded: None,
            selected: None,
            disabled: false,
            pressed: false,
            read_only: false,
            modal: false,
            focusable: true,
            focused: false,
            orientation: String::new(),
            placeholder: String::new(),
            level: None,
            bounds: Some((1.0, 2.0, 3.0, 4.0)),
            children: Vec::new(),
        }
    }

    #[test]
    fn converts_roles_names_and_states() {
        let label = convert(&item("label", "Volume"));
        assert_eq!(label.role(), Role::Label);
        assert_eq!(label.value(), Some("Volume"), "a label speaks its value");
        let mut switch = item("switch", "Wi-Fi");
        switch.checked = Some(Checked::True);
        let node = convert(&switch);
        assert_eq!(node.label(), Some("Wi-Fi"));
        assert_eq!(node.toggled(), Some(Toggled::True));
        assert!(node.supports_action(Action::Click) && node.supports_action(Action::Focus));
        let mut slider = item("slider", "Volume");
        slider.value = Some(AccessibleValue::Number(0.4));
        let node = convert(&slider);
        assert_eq!(node.numeric_value(), Some(0.4));
        assert!(node.supports_action(Action::Increment) && node.supports_action(Action::SetValue));
        assert_eq!(node.bounds(), Some(Rect { x0: 1.0, y0: 2.0, x1: 4.0, y1: 6.0 }));
    }
}
