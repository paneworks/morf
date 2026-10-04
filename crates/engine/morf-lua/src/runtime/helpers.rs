use luna::{Context, Table, Value as LuaValue};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use morf_scene::reactive::SignalId;
use morf_scene::{NodeHandle, Scene};

use crate::{
    reactive_bindings::run_destroyed_hooks, reactive_execute::*, state::*, surface_types::*,
    types::*,
};

pub(crate) fn scene_node_in_subtree(scene: &Scene, root: NodeHandle, node: NodeHandle) -> bool {
    morf_runtime::events::routing::node_in_subtree(scene, root, node)
}

/// Whether keys can go to a node; see [`morf_runtime::events::routing::takes_keys`].
pub(crate) fn takes_keys(state: &ReactiveState, node: NodeHandle) -> bool {
    morf_runtime::events::routing::takes_keys(&state.scene, &state.events, node)
}

pub(crate) fn remove_scene_subtree(state: &mut ReactiveState, node: NodeHandle) {
    let mut nodes = vec![node];
    let mut index = 0;
    while index < nodes.len() {
        let children = state.scene.children(nodes[index]).unwrap_or_default();
        nodes.extend_from_slice(children);
        index += 1;
    }
    state.revisions.scene_revision = state.revisions.scene_revision.wrapping_add(1);
    if state.scene.remove(node).is_err() {
        return;
    }
    // Deepest first, so a child lets go of what it holds before the parent
    // that may have lent it. They run later, once nothing is borrowed and no
    // flush is under way: see `run_destroyed_hooks`.
    for removed in nodes.iter().rev() {
        if let Some(hook) = state.destroy_hooks.remove(removed) {
            state.pending_destroyed.push(hook);
        }
    }
    let removed = nodes.into_iter().collect::<HashSet<_>>();
    for node in &removed {
        state.retained.forget(node);
        state.states.remove(node);
        state.views.remove(node);
        state.timer_callbacks.remove(node);
        state.terminals.remove(*node);
        state.images.remove(*node);
        state.linked_texts.remove(node);
        state.shortcuts.remove(node);
    }
    // A removed field cannot keep the keyboard. The node that had focus is
    // handed on by `Runtime::check_focus`, which still needs to know it went.
    state.editing.forget(&removed);
    state
        .focus
        .memory
        .retain(|scope, node| !removed.contains(scope) && !removed.contains(node));
    state.animation.forget(&removed);
    state.events.forget(&removed);
    state.timers.retain_nodes(|node| !removed.contains(&node));
    // Bindings that drive a removed node, and the signals that tracked its
    // properties' reads: the graph forgets both, or every one of them keeps
    // re-running and growing for the life of the shell.
    state.reactive.forget_effects_of(&removed);
    let dead_signals = state
        .property_signals
        .iter()
        .filter(|((node, _, _), _)| removed.contains(node))
        .map(|(_, signal)| *signal)
        .collect::<HashSet<_>>();
    state.reactive.forget_signals(dead_signals);
    state.collect_graph_garbage();
    state
        .property_signals
        .retain(|(node, _, _), _| !removed.contains(node));
    state
        .current_property_names
        .retain(|_, (node, _)| !removed.contains(node));
    state
        .transform_watchers
        .retain(|_, watcher| !removed.contains(&watcher.a) && !removed.contains(&watcher.b));
    let windows = &mut state.windows;
    windows.window_surfaces_changed |= morf_runtime::layout::forget_windows_of(
        &removed,
        &mut windows.window_surfaces,
        &mut windows.popup_node_anchors,
    );
}

pub(crate) fn finish_retained_destroy(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    node: NodeHandle,
) {
    let callback = state.borrow().retained.about_to_destroy(node);
    if let Some(callback) = callback
        && let Err(error) = execute_handler_args(ctx, &callback, &[], limits)
    {
        state.borrow_mut().log(
            LogLevel::Warn,
            format!("Retainable about_to_destroy: {error}"),
        );
    }
    remove_scene_subtree(&mut state.borrow_mut(), node);
    run_destroyed_hooks(state, ctx, limits);
}

/// Starts a node on its way out, if it declared an `exit`: it stays in the
/// tree, drawn and out of the flow, held in `retention` until the exit ends.
/// `false` when it has no exit to play, and whoever let go of it removes it.
pub(crate) fn begin_node_exit(state: &mut ReactiveState, node: NodeHandle) -> bool {
    let state = &mut *state;
    let start = morf_runtime::animation::exits::begin_exit(
        &mut state.scene,
        &mut state.retained.retention,
        &mut state.animation,
        node,
    );
    if start == morf_runtime::animation::exits::ExitStart::Started {
        state.revisions.scene_revision = state.revisions.scene_revision.wrapping_add(1);
    }
    start.leaving()
}

/// Takes back a node that was on its way out: it rejoins the flow and its
/// properties go back to where they were aimed. `false` if it was not leaving.
pub(crate) fn cancel_node_exit(state: &mut ReactiveState, node: NodeHandle) -> bool {
    let state = &mut *state;
    if !morf_runtime::animation::exits::cancel_exit(
        &mut state.scene,
        &mut state.retained.retention,
        &mut state.animation,
        node,
    ) {
        return false;
    }
    state.revisions.scene_revision = state.revisions.scene_revision.wrapping_add(1);
    true
}

/// A node whose exit has ended: its hold on it goes, and unless something
/// else still holds it, it is removed as it would have been at once.
pub(crate) fn finish_node_exit(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    node: NodeHandle,
) {
    let destroy = {
        let mut state = state.borrow_mut();
        let state = &mut *state;
        match morf_runtime::animation::exits::finish_exit(
            &state.scene,
            &mut state.retained.retention,
            node,
        ) {
            Some(destroy) => destroy,
            None => return,
        }
    };
    if destroy {
        finish_retained_destroy(state, ctx, limits, node);
    }
}

/// Lets go of a node the way a `Loader` or a list does: through its exit,
/// if it has one, else at once.
pub(crate) fn let_go_of_node(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    node: NodeHandle,
) {
    if begin_node_exit(&mut state.borrow_mut(), node) {
        return;
    }
    remove_scene_subtree(&mut state.borrow_mut(), node);
    run_destroyed_hooks(state, ctx, limits);
}

pub(crate) fn drop_retainable(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    node: NodeHandle,
) {
    // Held by the configuration's own `retainable`, or not: either way an
    // exit plays first, and holds it as a lock of its own.
    let registered = {
        let state = state.borrow();
        state.retained.retention.state(node).is_some()
            && !state.animation.exit_registered.contains(&node)
    };
    let exiting = begin_node_exit(&mut state.borrow_mut(), node);
    if !registered {
        if !exiting {
            remove_scene_subtree(&mut state.borrow_mut(), node);
            run_destroyed_hooks(state, ctx, limits);
        }
        return;
    }
    let callback = state.borrow_mut().retained.begin_drop(node);
    if let Some(callback) = callback
        && let Err(error) = execute_handler_args(ctx, &callback, &[], limits)
    {
        state
            .borrow_mut()
            .log(LogLevel::Warn, format!("Retainable dropped: {error}"));
    }
    if state.borrow().retained.should_destroy(node) {
        finish_retained_destroy(state, ctx, limits, node);
    }
}

pub(crate) fn register_reloadable_value(
    state: &mut ReactiveState,
    name: String,
    initial: IpcValue,
) -> Result<(SignalId, bool), String> {
    // Here, not at each door. Four different entry points reach this one map,
    // and they applied three different rules between them — so what counted as
    // a legal name depended on which way you came in, and a name accepted by
    // one door could collide with, or be unreachable from, another.
    validate_scope_part(&name)?;
    if state.reloadable.contains_key(&name) {
        return Err(format!("reloadable id `{name}` is already registered"));
    }
    let mut restored = false;
    let value = match state.reload_seed.remove(&name) {
        Some(value) if std::mem::discriminant(&value) == std::mem::discriminant(&initial) => {
            restored = true;
            value
        }
        Some(_) => {
            state.log(
                LogLevel::Warn,
                format!("reloadable `{name}` changed value type; using its new default"),
            );
            initial
        }
        None => initial,
    };
    let id = state
        .reactive
        .graph
        .as_mut()
        .ok_or_else(|| "reactive graph is already running".to_owned())?
        .signal(format!("reloadable.{name}"), value.clone());
    state.reactive.values.insert(id, value);
    state.reactive.signals.push(id);
    state.reloadable.insert(name, id);
    Ok((id, restored))
}

pub(crate) fn create_persistent_token<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    name: &str,
    defaults: Table<'gc>,
) -> Result<PersistentToken, String> {
    if name.is_empty() || name.len() > 256 {
        return Err("persistent id must be 1..256 bytes".into());
    }
    let mut definitions = Vec::new();
    for (key, value) in defaults.iter(ctx) {
        let LuaValue::String(key) = key else {
            return Err("persistent property names must be strings".into());
        };
        let key = key.display_lossy().to_string();
        if key.is_empty() || key.len() > 256 || matches!(key.as_str(), "loaded" | "reloaded") {
            return Err(format!("invalid persistent property `{key}`"));
        }
        definitions.push((key, IpcValue::from_lua(value)?));
        if definitions.len() > 256 {
            return Err("persistent object exceeds 256 properties".into());
        }
    }
    definitions.sort_by(|left, right| left.0.cmp(&right.0));
    let mut properties = HashMap::new();
    let mut reloaded = false;
    let mut state = state.borrow_mut();
    for (key, initial) in definitions {
        let full_name = format!("{name}.{key}");
        let (id, restored) = register_reloadable_value(&mut state, full_name, initial)?;
        reloaded |= restored;
        properties.insert(key, id);
    }
    Ok(PersistentToken {
        properties,
        reloaded,
    })
}

pub(crate) fn validate_scope_part(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 256 {
        return Err("scope IDs must be 1..256 bytes".into());
    }
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err("scope IDs cannot contain empty segments".into());
    }
    Ok(())
}

pub(crate) fn scoped_id(prefix: &str, name: &str) -> Result<String, String> {
    validate_scope_part(prefix)?;
    validate_scope_part(name)?;
    let value = format!("{prefix}.{name}");
    if value.len() > 256 {
        return Err("scoped reloadable ID exceeds 256 bytes".into());
    }
    Ok(value)
}
