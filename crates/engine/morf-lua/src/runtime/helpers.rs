use luna::{Context, Table, Value as LuaValue};
use std::cell::RefCell;
use std::collections::HashMap;
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

/// Removes `node` and its subtree: from the engine (`Engine::remove_subtree`)
/// and from what this layer keeps per node.
pub(crate) fn remove_scene_subtree(state: &mut ReactiveState, node: NodeHandle) {
    let Some(removed) = state.engine.remove_subtree(node) else {
        return;
    };
    for node in &removed {
        state.terminals.remove(*node);
        state.images.remove(*node);
    }
    state
        .transform_watchers
        .retain(|_, watcher| !removed.contains(&watcher.a) && !removed.contains(&watcher.b));
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
    state.engine.begin_node_exit(node)
}

/// Takes back a node that was on its way out: it rejoins the flow and its
/// properties go back to where they were aimed. `false` if it was not leaving.
pub(crate) fn cancel_node_exit(state: &mut ReactiveState, node: NodeHandle) -> bool {
    state.engine.cancel_node_exit(node)
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
            &state.engine.scene,
            &mut state.engine.retained.retention,
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
    state.engine.register_reloadable(name, initial)
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

pub(crate) use morf_runtime::engine::{scoped_id, validate_scope_part};
