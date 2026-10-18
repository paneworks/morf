//! Focus: which node of each surface has it, and how it moves -- Tab and
//! Shift+Tab along the chain, entering and leaving scopes whole, and the
//! node a click focuses. A configuration's requests are queued here and
//! settled at the next turn of the loop.

use std::collections::{HashMap, HashSet};

use morf_scene::{NodeHandle, Scene};

/// Why focus moved, which decides whether the ring shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusReason {
    /// A click or a touch: no ring.
    Click,
    /// Tab or Shift+Tab, or a program that asked as a keyboard would: a ring.
    Keyboard,
    /// A program, with no keyboard behind it: no ring.
    Program,
    /// Handed on after the focused node went away: the ring stays as it was.
    Restore,
}

/// What a configuration asked of focus, settled at the next turn.
#[derive(Clone, Copy, Debug)]
pub enum FocusRequest {
    Set(NodeHandle, bool),
    /// A node's `focus` turned true: it takes focus if it can hold it.
    Claim(NodeHandle),
    /// A node's `focus` turned false: it gives focus up if it had it.
    Release(NodeHandle),
    /// Focus into a subtree that opened (an overlay): its first node Tab
    /// would reach, with a ring when the focus it took over had one.
    Into(NodeHandle, bool),
    Clear(NodeHandle),
    Step(bool),
}

/// Who has focus, per surface, and what each scope remembers.
#[derive(Default)]
pub struct FocusState {
    /// The focused node of each scene root.
    owner: HashMap<NodeHandle, NodeHandle>,
    /// The scopes the owner sat in when it took focus, innermost first: where
    /// focus goes when the owner is gone and its ancestry with it.
    scopes: HashMap<NodeHandle, Vec<NodeHandle>>,
    /// The node in each scope that last had focus.
    memory: HashMap<NodeHandle, NodeHandle>,
    /// Scene roots whose surface the keyboard has left: their owner keeps
    /// focus but does not show it.
    inactive: HashSet<NodeHandle>,
    /// The root the last key or click went to, for `morf.focus.next()`.
    last_root: Option<NodeHandle>,
    requests: Vec<FocusRequest>,
}

impl FocusState {
    /// The focused node under `root`.
    pub fn owner(&self, root: NodeHandle) -> Option<NodeHandle> {
        self.owner.get(&root).copied()
    }

    /// Every scene root with its focused node.
    pub fn owners(&self) -> impl Iterator<Item = (NodeHandle, NodeHandle)> + '_ {
        self.owner.iter().map(|(root, owner)| (*root, *owner))
    }

    /// The scopes the owner under `root` sat in, innermost first.
    pub fn scopes(&self, root: NodeHandle) -> &[NodeHandle] {
        self.scopes.get(&root).map_or(&[], Vec::as_slice)
    }

    /// The node in each scope that last had focus.
    pub fn memory(&self) -> &HashMap<NodeHandle, NodeHandle> {
        &self.memory
    }

    /// `node` takes focus under `root`, remembered by each scope around it,
    /// innermost first.
    pub fn give(&mut self, root: NodeHandle, node: NodeHandle, scopes: Vec<NodeHandle>) {
        self.owner.insert(root, node);
        for scope in &scopes {
            self.memory.insert(*scope, node);
        }
        self.scopes.insert(root, scopes);
    }

    /// Nothing has focus under `root`.
    pub fn clear(&mut self, root: NodeHandle) {
        self.owner.remove(&root);
        self.scopes.remove(&root);
    }

    /// Whether the keyboard is on the surface whose tree is `root`.
    pub fn is_active(&self, root: NodeHandle) -> bool {
        !self.inactive.contains(&root)
    }

    /// The keyboard came to `root`'s surface or left it; whether it was
    /// there before.
    pub fn set_active(&mut self, root: NodeHandle, active: bool) -> bool {
        let was = self.is_active(root);
        if active {
            self.inactive.remove(&root);
        } else {
            self.inactive.insert(root);
        }
        was
    }

    /// The root the last key or click went to.
    pub fn last_root(&self) -> Option<NodeHandle> {
        self.last_root
    }

    /// A key or click went to `root`.
    pub fn note_root(&mut self, root: NodeHandle) {
        self.last_root = Some(root);
    }

    /// Queues what a configuration asked of focus.
    pub fn request(&mut self, request: FocusRequest) {
        self.requests.push(request);
    }

    /// Whether anything was asked of focus.
    pub fn has_requests(&self) -> bool {
        !self.requests.is_empty()
    }

    /// What was asked of focus, in order.
    pub fn take_requests(&mut self) -> Vec<FocusRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Forgets what scopes remember of removed nodes.
    pub fn forget(&mut self, removed: &HashSet<NodeHandle>) {
        self.memory
            .retain(|scope, node| !removed.contains(scope) && !removed.contains(node));
    }

    /// Queues what a write to a node's `focus` asks.
    pub fn request_by_property(&mut self, node: NodeHandle, on: bool) {
        self.requests.push(if on {
            FocusRequest::Claim(node)
        } else {
            FocusRequest::Release(node)
        });
    }
}

/// Whether `node` is `root` or under it.
pub fn in_subtree(scene: &Scene, root: NodeHandle, node: NodeHandle) -> bool {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate == root {
            return true;
        }
        current = scene.parent(candidate).ok().flatten();
    }
    false
}

/// The node Tab (or, `backwards`, Shift+Tab) goes to from `current` along
/// `chain` (a surface's focus chain), entering and leaving scopes whole:
/// leaving a scope steps past all of it, and entering one lands on the node
/// it remembers (`memory`).
pub fn next_in(
    scene: &Scene,
    chain: &[NodeHandle],
    memory: &HashMap<NodeHandle, NodeHandle>,
    current: Option<NodeHandle>,
    backwards: bool,
) -> Option<NodeHandle> {
    let within = |scope: NodeHandle, node: NodeHandle| in_subtree(scene, scope, node);
    let count = chain.len();
    if count == 0 {
        return None;
    }
    let step = |index: usize| {
        if backwards {
            (index + count - 1) % count
        } else {
            (index + 1) % count
        }
    };
    let at = current.and_then(|node| chain.iter().position(|n| *n == node));
    let mut index = match at {
        Some(index) => step(index),
        None if backwards => count - 1,
        None => 0,
    };
    // Leaving a scope leaves all of it: its nodes are one run of the chain,
    // so step past the run's far end.
    if let Some(current) = current.filter(|_| at.is_some())
        && let Some(scope) = scene.focus_scope_of(current)
        && !within(scope, chain[index])
    {
        let first = chain.iter().position(|n| within(scope, *n));
        let last = chain.iter().rposition(|n| within(scope, *n));
        if let (Some(first), Some(last)) = (first, last) {
            index = if backwards { step(first) } else { step(last) };
        }
    }
    let candidate = chain[index];
    // Entering a scope lands where it last had focus: the outermost scope
    // entered remembers the deepest node.
    let mut entered = None;
    let mut scope = scene.focus_scope_of(candidate);
    while let Some(s) = scope {
        if current.is_some_and(|c| within(s, c)) {
            break;
        }
        entered = Some(s);
        scope = scene.focus_scope_of(s);
    }
    if let Some(scope) = entered
        && let Some(remembered) = memory.get(&scope).copied()
        && remembered != candidate
        && within(scope, remembered)
        && chain.contains(&remembered)
    {
        return Some(remembered);
    }
    Some(candidate)
}

/// The node a click on `node` focuses: the nearest that takes focus by
/// click, itself or an ancestor. `takes_keys` says which nodes take keys.
pub fn click_target(
    scene: &Scene,
    node: NodeHandle,
    takes_keys: impl Fn(NodeHandle) -> bool,
) -> Option<NodeHandle> {
    let mut current = Some(node);
    while let Some(node) = current {
        if scene.focus_policy(node).by_click(takes_keys(node)) {
            return scene.can_hold_focus(node).then_some(node);
        }
        current = scene.parent(node).ok().flatten();
    }
    None
}
