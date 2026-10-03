//! Which nodes can have focus, and in what order Tab visits them.
//!
//! A node's `focus_policy` says how it takes focus. The default, "auto",
//! keeps what the engine did before there was a policy: a node that takes
//! keys (a key handler, a text input, a terminal) takes focus by click and
//! by Tab, and nothing else does. A control that handles its keys through
//! something other than its own node -- a button whose skin is a child --
//! says "strong" instead.
//!
//! The chain is the tree in order, depth first, children in their order. A
//! subtree that is hidden, disabled or on its way out is skipped whole: a
//! node inside it cannot be reached and cannot keep focus.

use crate::types::{NodeHandle, Scene};

/// How a node takes focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPolicy {
    /// By click and by Tab when the node takes keys; never otherwise.
    Auto,
    /// Never.
    None,
    /// By click (or touch) only.
    Click,
    /// By Tab only.
    Tab,
    /// By click and by Tab.
    Strong,
}

impl FocusPolicy {
    /// The policy a word names, if it names one.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "auto" => Self::Auto,
            "none" => Self::None,
            "click" => Self::Click,
            "tab" => Self::Tab,
            "strong" => Self::Strong,
            _ => return None,
        })
    }

    /// Whether Tab stops at a node with this policy (`takes_keys`: whether
    /// the node takes keys, for "auto").
    pub fn by_tab(self, takes_keys: bool) -> bool {
        match self {
            Self::Auto => takes_keys,
            Self::Tab | Self::Strong => true,
            Self::None | Self::Click => false,
        }
    }

    /// Whether a click or a touch on a node with this policy focuses it.
    pub fn by_click(self, takes_keys: bool) -> bool {
        match self {
            Self::Auto => takes_keys,
            Self::Click | Self::Strong => true,
            Self::None | Self::Tab => false,
        }
    }
}

impl Scene {
    /// A node's `focus_policy`; "auto" for one that has none or is gone.
    pub fn focus_policy(&self, node: NodeHandle) -> FocusPolicy {
        self.string_value(node, "focus_policy")
            .ok()
            .and_then(FocusPolicy::parse)
            .unwrap_or(FocusPolicy::Auto)
    }

    /// Whether a node itself lets focus in: shown, enabled, not leaving.
    fn admits_focus(&self, node: NodeHandle) -> bool {
        self.bool_value(node, "visible").unwrap_or(false)
            && self.bool_value(node, "enabled").unwrap_or(false)
            && !self.is_exiting(node)
    }

    /// Whether a node can hold focus where it is: it and every ancestor
    /// shown, enabled and staying.
    pub fn can_hold_focus(&self, node: NodeHandle) -> bool {
        let mut current = Some(node);
        while let Some(node) = current {
            if !self.contains(node) || !self.admits_focus(node) {
                return false;
            }
            current = self.parent(node).ok().flatten();
        }
        true
    }

    /// The nodes Tab visits under `root`, in order. `takes_keys` answers
    /// for a node with the "auto" policy.
    pub fn focus_chain(
        &self,
        root: NodeHandle,
        takes_keys: impl Fn(NodeHandle) -> bool,
    ) -> Vec<NodeHandle> {
        self.focus_nodes(root, |node| {
            self.focus_policy(node).by_tab(takes_keys(node))
        })
    }

    /// Every node under `root` that `wanted` accepts and that can hold
    /// focus, in tree order.
    pub fn focus_nodes(
        &self,
        root: NodeHandle,
        wanted: impl Fn(NodeHandle) -> bool,
    ) -> Vec<NodeHandle> {
        let mut found = Vec::new();
        if !self.can_hold_focus(root) {
            return found;
        }
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if node != root && !self.admits_focus(node) {
                continue;
            }
            if wanted(node) {
                found.push(node);
            }
            if let Ok(children) = self.children(node) {
                pending.extend(children.iter().rev().copied());
            }
        }
        found
    }

    /// The nearest ancestor of `node` (not itself) that is a `focus_scope`.
    pub fn focus_scope_of(&self, node: NodeHandle) -> Option<NodeHandle> {
        let mut current = self.parent(node).ok().flatten();
        while let Some(node) = current {
            if self.bool_value(node, "focus_scope").unwrap_or(false) {
                return Some(node);
            }
            current = self.parent(node).ok().flatten();
        }
        None
    }
}
