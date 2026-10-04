use luna::{Context, Table, Value as LuaValue};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use morf_scene::reactive::SignalId;
use morf_scene::{NodeHandle, Scene};

use crate::{
    events::*, reactive_bindings::run_destroyed_hooks, reactive_execute::*, state::*,
    surface_types::*, types::*,
};

pub(crate) fn geometry_i32(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

pub(crate) fn scene_node_in_subtree(scene: &Scene, root: NodeHandle, node: NodeHandle) -> bool {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate == root {
            return true;
        }
        current = scene.parent(candidate).ok().flatten();
    }
    false
}

/// Whether a node has a handler for key presses or releases.
pub(crate) fn handles_keys(state: &ReactiveState, node: NodeHandle) -> bool {
    state.handlers.contains_key(&(node, UiEvent::KeyPressed))
        || state.handlers.contains_key(&(node, UiEvent::KeyReleased))
}

/// Whether keys can go to a node: it handles them, or it is a text input or
/// a terminal, which take them themselves.
pub(crate) fn takes_keys(state: &ReactiveState, node: NodeHandle) -> bool {
    handles_keys(state, node)
        || matches!(
            state.scene.element(node).ok(),
            Some(morf_scene::Element::TextInput | morf_scene::Element::Terminal)
        )
}

/// Every node under `root` that takes keys and can hold focus -- shown,
/// enabled and staying, through its ancestors -- in tree order.
pub(crate) fn key_targets_in(state: &ReactiveState, root: NodeHandle) -> Vec<NodeHandle> {
    state
        .scene
        .focus_nodes(root, |node| takes_keys(state, node))
}

pub(crate) fn remove_scene_subtree(state: &mut ReactiveState, node: NodeHandle) {
    let mut nodes = vec![node];
    let mut index = 0;
    while index < nodes.len() {
        let children = state.scene.children(nodes[index]).unwrap_or_default();
        nodes.extend_from_slice(children);
        index += 1;
    }
    state.scene_revision = state.scene_revision.wrapping_add(1);
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
        state.retention.unregister(*node);
        state.exit_registered.remove(node);
        state.retain_callbacks.remove(node);
        state.states.remove(node);
        state.views.remove(node);
        state.timer_callbacks.remove(node);
        state.loader_factories.remove(node);
        state.failed_loaders.remove(node);
        state.loaded_loaders.remove(node);
        state.dormant_loaders.remove(node);
        state.preload_pending.remove(node);
        state.node_loops.remove(node);
        state.terminals.remove(*node);
        state.images.remove(*node);
        state.linked_texts.remove(node);
        state.pointer_watch.remove(node);
        state.shortcuts.remove(node);
    }
    // A removed field cannot keep the keyboard. The node that had focus is
    // handed on by `Runtime::check_focus`, which still needs to know it went.
    if state
        .focused_input
        .is_some_and(|node| removed.contains(&node))
    {
        state.focused_input = None;
    }
    state
        .focus
        .memory
        .retain(|scope, node| !removed.contains(scope) && !removed.contains(node));
    if !state.pointer_watch_fresh.is_empty() {
        state
            .pointer_watch_fresh
            .retain(|node| !removed.contains(node));
    }
    state
        .animation_callbacks
        .retain(|(owner, _), _| !removed.contains(owner));
    state
        .handlers
        .retain(|(node, _), _| !removed.contains(node));
    state.text_inputs.retain(|node, _| !removed.contains(node));
    state
        .text_input_order
        .retain(|node, _| !removed.contains(node));
    state
        .input_events
        .retain(|(node, _, _)| !removed.contains(node));
    state.timers.retain_nodes(|node| !removed.contains(&node));
    // Bindings that drive a removed node, and the signals that tracked its
    // properties' reads: the graph forgets both, or every one of them keeps
    // re-running and growing for the life of the shell.
    let dead_tokens = state
        .reactive
        .effects
        .iter()
        .filter(|(_, effect)| match &effect.sink {
            Some(EffectSink::Property(sink)) => removed.contains(&sink.node),
            Some(EffectSink::State(node) | EffectSink::Loop(node)) => removed.contains(node),
            // A `morf.effect` given `owner = node` goes with its node.
            None => effect.owner.is_some_and(|owner| removed.contains(&owner)),
        })
        .map(|(token, _)| *token)
        .collect::<Vec<_>>();
    for token in dead_tokens {
        state.reactive.effects.remove(&token);
        if let Some(id) = state.reactive.effect_ids.remove(&token) {
            state.reactive.dead_effects.push(id);
        }
    }
    let dead_signals = state
        .property_signals
        .iter()
        .filter(|((node, _, _), _)| removed.contains(node))
        .map(|(_, signal)| *signal)
        .collect::<HashSet<_>>();
    if !dead_signals.is_empty() {
        for signal in &dead_signals {
            state.reactive.values.remove(signal);
        }
        state
            .reactive
            .signals
            .retain(|signal| !dead_signals.contains(signal));
        state.reactive.dead_signals.extend(dead_signals);
    }
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
    let surface_count = state.window_surfaces.len();
    state
        .window_surfaces
        .retain(|_, surface| !removed.contains(&surface.root));
    let removed_anchors = state
        .popup_node_anchors
        .iter()
        .filter_map(|(id, anchor)| removed.contains(&anchor.node).then_some(*id))
        .collect::<Vec<_>>();
    for id in removed_anchors {
        state.popup_node_anchors.remove(&id);
        if let Some(surface) = state.window_surfaces.get_mut(&id) {
            surface.visible = false;
            state.window_surfaces_changed = true;
        }
    }
    let surface_ids = state
        .window_surfaces
        .keys()
        .copied()
        .collect::<HashSet<_>>();
    state
        .popup_node_anchors
        .retain(|id, anchor| surface_ids.contains(id) && !removed.contains(&anchor.node));
    state.window_surfaces_changed |= state.window_surfaces.len() != surface_count;
}

pub(crate) fn finish_retained_destroy(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    node: NodeHandle,
) {
    let callback = state
        .borrow()
        .retain_callbacks
        .get(&node)
        .and_then(|callbacks| callbacks.about_to_destroy.clone());
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
    if state.scene.is_exiting(node) {
        return true;
    }
    if !matches!(state.scene.begin_exit(node), Ok(true)) {
        return false;
    }
    if state.retention.state(node).is_none() {
        state.retention.register(node);
        state.exit_registered.insert(node);
    }
    let _ = state.retention.lock(node);
    let _ = state.retention.begin_drop(node);
    state.scene_revision = state.scene_revision.wrapping_add(1);
    true
}

/// Takes back a node that was on its way out: it rejoins the flow and its
/// properties go back to where they were aimed. `false` if it was not leaving.
pub(crate) fn cancel_node_exit(state: &mut ReactiveState, node: NodeHandle) -> bool {
    if !state.scene.cancel_exit(node).unwrap_or(false) {
        return false;
    }
    if state.exit_registered.remove(&node) {
        state.retention.unregister(node);
    } else {
        let _ = state.retention.unlock(node);
        let _ = state.retention.cancel_drop(node);
    }
    state.scene_revision = state.scene_revision.wrapping_add(1);
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
        if !state.scene.contains(node) {
            return;
        }
        let _ = state.retention.unlock(node);
        state.retention.should_destroy(node).unwrap_or(true)
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
        state.retention.state(node).is_some() && !state.exit_registered.contains(&node)
    };
    let exiting = begin_node_exit(&mut state.borrow_mut(), node);
    if !registered {
        if !exiting {
            remove_scene_subtree(&mut state.borrow_mut(), node);
            run_destroyed_hooks(state, ctx, limits);
        }
        return;
    }
    let callback = {
        let mut state = state.borrow_mut();
        let _ = state.retention.begin_drop(node);
        state
            .retain_callbacks
            .get(&node)
            .and_then(|callbacks| callbacks.dropped.clone())
    };
    if let Some(callback) = callback
        && let Err(error) = execute_handler_args(ctx, &callback, &[], limits)
    {
        state
            .borrow_mut()
            .log(LogLevel::Warn, format!("Retainable dropped: {error}"));
    }
    if state
        .borrow()
        .retention
        .should_destroy(node)
        .unwrap_or(true)
    {
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
