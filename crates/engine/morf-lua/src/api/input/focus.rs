//! Focus: which node of each surface has it, how it moves, and the two
//! properties a theme reads -- `focused`, and `visual_focus` for the ring.
//!
//! Each scene root (a surface) has at most one focused node. Tab and
//! Shift+Tab walk the chain (`Scene::focus_chain`); a click or a touch
//! focuses the nearest node whose policy takes focus by click, and a click on
//! nothing focusable leaves focus where it was. Only a keyboard sets
//! `visual_focus`: a ring follows Tab, never a click.
//!
//! A `focus_scope` remembers the node in it that last had focus. Tab into
//! the scope lands there; Tab out of it leaves the scope whole rather than
//! visiting the rest of it. When the focused node is removed, hidden or
//! disabled, focus goes to what its nearest surviving scope remembers, or to
//! the scope's first node -- and nowhere when it was in no scope.
//!
//! A configuration moves focus with `morf.focus.set(node)`, asks where it is
//! with `morf.focus.get()`, and steps with `morf.focus.next()` and
//! `morf.focus.previous()`. Those are queued and settled at the next turn of
//! the loop, outside whatever handler asked.

use std::cell::RefCell;

use std::rc::Rc;

use luna::{Callback, CallbackReturn, Context, Table, UserRef};
use morf_scene::{Element, NodeHandle, Value as SceneValue};

use crate::IpcValue;
use crate::Runtime;
use crate::events::UiEvent;
use crate::runtime_helpers::takes_keys;
use crate::scene_bindings::{assign_scene_property, node_userdata};
use crate::state::ReactiveState;
use crate::state_tokens::NodeToken;

pub use morf_runtime::focus::FocusReason;
pub(crate) use morf_runtime::focus::FocusRequest;

/// Queues what a write to a node's `focus` asks.
pub(crate) fn request_by_property(state: &mut ReactiveState, node: NodeHandle, on: bool) {
    state.focus.request_by_property(node, on);
}

fn chain(state: &ReactiveState, root: NodeHandle) -> Vec<NodeHandle> {
    state
        .scene
        .focus_chain(root, |node| takes_keys(state, node))
}

fn within(state: &ReactiveState, scope: NodeHandle, node: NodeHandle) -> bool {
    crate::runtime_helpers::scene_node_in_subtree(&state.scene, scope, node)
}

fn write(state: &mut ReactiveState, node: NodeHandle, property: &str, on: bool) {
    if state.scene.contains(node) && state.scene.bool_value(node, property).ok() != Some(on) {
        let _ = assign_scene_property(state, node, property, SceneValue::Bool(on));
    }
}

impl Runtime {
    /// The focused node of the surface whose tree is `root`.
    pub fn focus_owner(&self, root: NodeHandle) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        state
            .focus
            .owner
            .get(&root)
            .copied()
            .filter(|node| state.scene.contains(*node))
    }

    /// The node Tab (or, `backwards`, Shift+Tab) goes to from `current`
    /// under `root`, entering and leaving scopes whole.
    pub fn next_focus_in(
        &self,
        root: NodeHandle,
        current: Option<NodeHandle>,
        backwards: bool,
    ) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        let chain = chain(&state, root);
        morf_runtime::focus::next_in(
            &state.scene,
            &chain,
            &state.focus.memory,
            current,
            backwards,
        )
    }

    /// The node a click on `node` focuses: the nearest that takes focus by
    /// click, itself or an ancestor.
    pub fn click_focus_target(&self, node: NodeHandle) -> Option<NodeHandle> {
        let state = self.reactive.borrow();
        morf_runtime::focus::click_target(&state.scene, node, |node| takes_keys(&state, node))
    }

    /// Gives focus under `root` to `node`, or to nothing. Writes `focused`
    /// and `visual_focus`, hands the keyboard to a text input or terminal,
    /// remembers the node in every scope around it, and calls
    /// `on_focus_changed` on the nodes that gained and lost it. Returns
    /// whether anything changed.
    pub fn set_focus(
        &mut self,
        root: NodeHandle,
        node: Option<NodeHandle>,
        reason: FocusReason,
    ) -> bool {
        let changed = {
            let mut state = self.reactive.borrow_mut();
            state.focus.last_root = Some(root);
            let old = state.focus.owner.get(&root).copied();
            let old_visual =
                old.is_some_and(|o| state.scene.bool_value(o, "visual_focus").unwrap_or(false));
            let visual = match reason {
                FocusReason::Keyboard => true,
                FocusReason::Click | FocusReason::Program => false,
                FocusReason::Restore => old_visual,
            };
            let active = !state.focus.inactive.contains(&root);
            if let Some(old) = old.filter(|o| Some(*o) != node) {
                write(&mut state, old, "focused", false);
                write(&mut state, old, "visual_focus", false);
                if state.scene.contains(old)
                    && state.scene.element(old).ok() != Some(Element::TextInput)
                {
                    state.editing.events.push((
                        old,
                        UiEvent::FocusChanged,
                        vec![IpcValue::Boolean(false)],
                    ));
                }
            }
            match node {
                Some(node) => {
                    state.focus.owner.insert(root, node);
                    write(&mut state, node, "focused", active);
                    write(&mut state, node, "visual_focus", active && visual);
                    let mut scopes = Vec::new();
                    let mut scope = state.scene.focus_scope_of(node);
                    while let Some(s) = scope {
                        state.focus.memory.insert(s, node);
                        scopes.push(s);
                        scope = state.scene.focus_scope_of(s);
                    }
                    state.focus.scopes.insert(root, scopes);
                    if old != Some(node)
                        && state.scene.element(node).ok() != Some(Element::TextInput)
                    {
                        state.editing.events.push((
                            node,
                            UiEvent::FocusChanged,
                            vec![IpcValue::Boolean(true)],
                        ));
                    }
                }
                None => {
                    state.focus.owner.remove(&root);
                    state.focus.scopes.remove(&root);
                }
            }
            old != node || old_visual != visual
        };
        // The keyboard follows: into a text input or terminal, out of
        // whichever had it otherwise.
        let keyboard = self.set_key_focus(node);
        self.flush_after_event();
        self.drain_input_events();
        changed || keyboard
    }

    /// The keyboard came to the surface whose tree is `root`, or left it:
    /// its focused node shows focus again, or stops showing it.
    pub fn set_focus_active(&mut self, root: NodeHandle, active: bool) -> bool {
        let changed = {
            let mut state = self.reactive.borrow_mut();
            let was = !state.focus.inactive.contains(&root);
            if active {
                state.focus.inactive.remove(&root);
            } else {
                state.focus.inactive.insert(root);
            }
            match state.focus.owner.get(&root).copied() {
                Some(owner) if was != active => {
                    write(&mut state, owner, "focused", active);
                    if !active {
                        write(&mut state, owner, "visual_focus", false);
                    }
                    true
                }
                _ => false,
            }
        };
        self.flush_after_event();
        changed
    }

    /// Settles focus once a turn: hands it on from a node that can no longer
    /// hold it, follows a text input a configuration focused itself, and
    /// runs what `morf.focus` asked. Returns whether anything changed.
    pub fn check_focus(&mut self) -> bool {
        let mut moves: Vec<(NodeHandle, Option<NodeHandle>, FocusReason)> = Vec::new();
        let requests = {
            let state = self.reactive.borrow();
            for (&root, &owner) in &state.focus.owner {
                if state.scene.can_hold_focus(owner) && state.scene.root_of(owner) == Some(root) {
                    continue;
                }
                let scopes = state.focus.scopes.get(&root).cloned().unwrap_or_default();
                let next = scopes
                    .iter()
                    .copied()
                    .filter(|scope| state.scene.can_hold_focus(*scope))
                    .find_map(|scope| {
                        let found = chain(&state, scope);
                        state
                            .focus
                            .memory
                            .get(&scope)
                            .copied()
                            .filter(|m| *m != owner && found.contains(m))
                            .or_else(|| found.first().copied())
                    });
                moves.push((root, next, FocusReason::Restore));
            }
            // A field the configuration focused by writing `focus` has the
            // keyboard: it has focus too.
            if let Some(input) = state.editing.focused
                && let Some(root) = state.scene.root_of(input)
                && state.focus.owner.get(&root) != Some(&input)
                && !moves.iter().any(|(r, _, _)| *r == root)
            {
                moves.push((root, Some(input), FocusReason::Program));
            }
            state.focus.requests.is_empty().then(Vec::new)
        };
        let requests = match requests {
            Some(none) => none,
            None => std::mem::take(&mut self.reactive.borrow_mut().focus.requests),
        };
        let mut changed = false;
        for (root, node, reason) in moves {
            changed |= self.set_focus(root, node, reason);
        }
        for request in requests {
            changed |= self.run_focus_request(request);
        }
        changed
    }

    fn run_focus_request(&mut self, request: FocusRequest) -> bool {
        let (root, node, reason) = {
            let state = self.reactive.borrow();
            match request {
                FocusRequest::Set(node, keyboard) => {
                    let Some(root) = state.scene.root_of(node) else {
                        return false;
                    };
                    if !state.scene.can_hold_focus(node) {
                        return false;
                    }
                    let reason = if keyboard {
                        FocusReason::Keyboard
                    } else {
                        FocusReason::Program
                    };
                    (root, Some(node), reason)
                }
                FocusRequest::Into(node, visual) => {
                    let Some(root) = state.scene.root_of(node) else {
                        return false;
                    };
                    // Already inside it -- what opened it focused a part of
                    // it first -- there is nowhere to move focus into.
                    if state
                        .focus
                        .owner
                        .get(&root)
                        .is_some_and(|owner| within(&state, node, *owner))
                    {
                        return false;
                    }
                    let Some(first) = chain(&state, node).first().copied() else {
                        return false;
                    };
                    let reason = if visual {
                        FocusReason::Keyboard
                    } else {
                        FocusReason::Program
                    };
                    (root, Some(first), reason)
                }
                FocusRequest::Claim(node) => {
                    let Some(root) = state.scene.root_of(node) else {
                        return false;
                    };
                    let still = state.scene.bool_value(node, "focus").unwrap_or(false);
                    if !still || !state.scene.can_hold_focus(node) {
                        return false;
                    }
                    (root, Some(node), FocusReason::Program)
                }
                FocusRequest::Release(node) | FocusRequest::Clear(node) => {
                    let Some(root) = state.scene.root_of(node) else {
                        return false;
                    };
                    if state.focus.owner.get(&root) != Some(&node) {
                        return false;
                    }
                    (root, None, FocusReason::Program)
                }
                FocusRequest::Step(backwards) => {
                    let Some(root) = state.focus.last_root.filter(|r| state.scene.contains(*r))
                    else {
                        return false;
                    };
                    let current = state.focus.owner.get(&root).copied();
                    drop(state);
                    let next = self.next_focus_in(root, current, backwards);
                    (root, next, FocusReason::Keyboard)
                }
            }
        };
        self.set_focus(root, node, reason)
    }
}

/// `morf.focus`: `set(node, keyboard?)`, `clear(node)`, `get()`, `next()`,
/// `previous()`.
pub(crate) fn install_focus_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let focus = Table::new(&ctx);
    let set = Rc::clone(&state);
    focus.set_field(
        ctx,
        "set",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, keyboard): (UserRef<NodeToken>, Option<bool>) = stack.consume(ctx)?;
            let request = FocusRequest::Set(node.handle, keyboard.unwrap_or(false));
            set.borrow_mut().focus.requests.push(request);
            Ok(CallbackReturn::Return)
        }),
    );
    let clear = Rc::clone(&state);
    focus.set_field(
        ctx,
        "clear",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            clear
                .borrow_mut()
                .focus
                .requests
                .push(FocusRequest::Clear(node.handle));
            Ok(CallbackReturn::Return)
        }),
    );
    for (name, backwards) in [("next", false), ("previous", true)] {
        let step = Rc::clone(&state);
        focus.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |_, _, mut stack| {
                step.borrow_mut()
                    .focus
                    .requests
                    .push(FocusRequest::Step(backwards));
                stack.clear();
                Ok(CallbackReturn::Return)
            }),
        );
    }
    let get = Rc::clone(&state);
    focus.set_field(
        ctx,
        "get",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let owner = {
                let state = get.borrow();
                state
                    .focus
                    .last_root
                    .and_then(|root| state.focus.owner.get(&root).copied())
                    .filter(|node| state.scene.contains(*node))
            };
            stack.clear();
            if let Some(node) = owner {
                stack.push_back(node_userdata(ctx, Rc::clone(&get), node).into());
            }
            Ok(CallbackReturn::Return)
        }),
    );
    morf.set_field(ctx, "focus", focus);
}
