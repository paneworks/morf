//! Timer and Loader nodes kept in step with the scene: native timers for
//! the ones that run, items built, shown, hidden, preloaded and let go.

use super::*;

/// Brings the native timers in line with the `Timer` nodes, when the scene
/// changed. Returns whether anything did.
pub(super) fn reconcile_timers(state: &mut ReactiveState, definitions_changed: bool) -> bool {
    let mut service_changed = false;
    let timer_definitions = if definitions_changed {
        state
            .timer_callbacks
            .iter()
            .map(|(node, callback)| (*node, callback.clone()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut stale_timers = Vec::new();
    for (node, callback) in timer_definitions {
        let Ok(running) = state.scene.bool_value(node, "running") else {
            stale_timers.push(node);
            continue;
        };
        let interval = state.scene.number(node, "interval").unwrap_or(0.0);
        let repeat = state.scene.bool_value(node, "repeat").unwrap_or(false);
        let duration = (interval.is_finite() && interval > 0.0)
            .then(|| Duration::from_secs_f64(interval / 1_000.0));
        if !running || duration.is_none() {
            service_changed |= state.timers.remove_node(node);
            continue;
        }
        let duration = duration.expect("validated duration");
        let matches = state
            .timers
            .for_node(node)
            .is_some_and(|timer| timer.interval == duration && timer.repeat == repeat);
        if matches {
            continue;
        }
        state.timers.remove_node(node);
        match state.timers.source(duration) {
            Ok(source) => {
                let id = state.timers.next_id();
                let origin = state
                    .timer_origins
                    .get(&node)
                    .cloned()
                    .unwrap_or_else(|| format!("ui.Timer {node:?}").into());
                state.timers.add(Timer {
                    id,
                    source,
                    handler: callback,
                    repeat,
                    interval: duration,
                    node: Some(node),
                    origin,
                });
            }
            Err(error) => state.log(LogLevel::Warn, format!("Timer: {error}")),
        }
        service_changed = true;
    }
    for node in stale_timers {
        state.timer_callbacks.remove(&node);
        state.timers.remove_node(node);
        state.timer_origins.remove(&node);
    }
    service_changed
}

/// Brings each `Loader` node's item in line with what the node asks for:
/// shown, hidden, let go, or owed a build (`loaders`, and at most one of
/// `preloads`). Returns whether anything changed.
pub(super) fn reconcile_loaders(
    state: &mut ReactiveState,
    definitions_changed: bool,
    loaders: &mut Vec<(NodeHandle, Handler)>,
    preloads: &mut Vec<(NodeHandle, Handler)>,
    loader_drops: &mut Vec<NodeHandle>,
) -> bool {
    let mut service_changed = false;
    let loader_definitions = if definitions_changed || !state.retained.preload_pending.is_empty() {
        state
            .retained
            .loader_factories
            .iter()
            .map(|(node, factory)| (*node, factory.clone()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut stale_loaders = Vec::new();
    // Preloading is for when nothing is moving: one item built per
    // turn, and none while anything animates -- the frame a build
    // costs is exactly the frame a motion cannot spare. Something
    // that never stops (a spinner) holds a preload back only so long.
    let still = !loader_definitions.is_empty() && !state.scene.has_motion();
    let now = std::time::Instant::now();
    for (node, factory) in loader_definitions {
        let Ok(active) = state.scene.bool_value(node, "active") else {
            stale_loaders.push(node);
            continue;
        };
        let loading = state.scene.bool_value(node, "loading").unwrap_or(false);
        let active_async = state
            .scene
            .bool_value(node, "active_async")
            .unwrap_or(false);
        let requested = active || loading || active_async;
        // A source that failed stays failed until the Loader is let go
        // and asked again; trying it every frame filled the log with
        // one error sixty times a second and spent a frame on it each.
        if !requested {
            state.retained.failed_loaders.remove(&node);
        } else if state.retained.failed_loaders.contains(&node) {
            continue;
        }
        let keep = state.scene.bool_value(node, "keep").unwrap_or(false);
        let preload = state.scene.bool_value(node, "preload").unwrap_or(false);
        // A hidden item stays while the Loader holds it for a reason:
        // kept, or built ahead of being asked for.
        let preloaded_and_waiting = preload && state.retained.dormant_loaders.contains(&node);
        if requested && state.retained.dormant_loaders.remove(&node) {
            // Built already: shown, not built again.
            let children = state.scene.children(node).unwrap_or_default().to_vec();
            for child in children {
                let _ = assign_scene_property(state, child, "visible", SceneValue::Bool(true));
            }
            let _ = assign_scene_property(state, node, "loading", SceneValue::Bool(false));
            let _ = assign_scene_property(state, node, "active_async", SceneValue::Bool(false));
            if !active {
                let _ = assign_scene_property(state, node, "active", SceneValue::Bool(true));
            }
            service_changed = true;
        } else if requested && state.retained.loaded_loaders.insert(node) {
            state.retained.preload_pending.remove(&node);
            // Let go and asked for again before its item had finished
            // leaving: that item is taken back rather than built anew.
            let leaving = state
                .scene
                .children(node)
                .unwrap_or_default()
                .iter()
                .copied()
                .find(|child| state.scene.is_exiting(*child));
            if let Some(child) = leaving
                && crate::runtime_helpers::cancel_node_exit(state, child)
            {
                for (property, value) in [
                    ("loading", false),
                    ("active_async", false),
                    ("active", true),
                ] {
                    let _ = assign_scene_property(state, node, property, SceneValue::Bool(value));
                }
                service_changed = true;
            } else {
                loaders.push((node, factory));
            }
        } else if !requested
            && keep
            && state.retained.loaded_loaders.contains(&node)
            && !state.retained.dormant_loaders.contains(&node)
        {
            // Let go, but kept: hidden until it is asked for again.
            let children = state.scene.children(node).unwrap_or_default().to_vec();
            for child in children {
                let _ = assign_scene_property(state, child, "visible", SceneValue::Bool(false));
            }
            state.retained.dormant_loaders.insert(node);
            service_changed = true;
        } else if !requested
            && state.retained.loaded_loaders.contains(&node)
            && !keep
            && !preloaded_and_waiting
        {
            state.retained.loaded_loaders.remove(&node);
            state.retained.dormant_loaders.remove(&node);
            loader_drops.extend_from_slice(state.scene.children(node).unwrap_or_default());
            service_changed = true;
        } else if !requested && preload && !state.retained.loaded_loaders.contains(&node) {
            let since = *state.retained.preload_pending.entry(node).or_insert(now);
            let waited = now.duration_since(since) >= PRELOAD_PATIENCE;
            if (still || waited) && preloads.is_empty() {
                state.retained.preload_pending.remove(&node);
                state.retained.loaded_loaders.insert(node);
                preloads.push((node, factory));
            }
        } else if !preload {
            state.retained.preload_pending.remove(&node);
        }
    }
    for node in stale_loaders {
        state.retained.loader_factories.remove(&node);
        state.retained.loaded_loaders.remove(&node);
        state.retained.failed_loaders.remove(&node);
        state.retained.dormant_loaders.remove(&node);
        state.retained.preload_pending.remove(&node);
    }
    service_changed
}

impl Runtime {
    /// Builds the items owed: the ones asked for, then the ones preloaded.
    /// Returns whether anything changed.
    pub(super) fn build_loaders(
        &mut self,
        loaders: Vec<(NodeHandle, Handler)>,
        preloads: Vec<(NodeHandle, Handler)>,
    ) -> bool {
        let mut service_changed = false;
        let asked_for = loaders.len();
        for (index, (node, factory)) in loaders.into_iter().chain(preloads).enumerate() {
            // Built ahead of being asked for: kept hidden, and `active`
            // left as it is.
            let preloaded = index >= asked_for;
            let _span = crate::profile::span(|| {
                let state = self.reactive.borrow();
                let origin = self.lua.enter(|ctx| {
                    crate::profile::closure_origin(
                        ctx.fetch(&crate::vm::handler_store::stashed(&factory)),
                    )
                });
                format!(
                    "loader build {} ({origin})",
                    crate::runtime_config::lint_path(&state.scene, node)
                )
            });
            let result = self
                .lua
                .enter(|ctx| execute_node_factory(ctx, &factory, self.limits));
            match result {
                Ok(child) => {
                    let mut state = self.reactive.borrow_mut();
                    if preloaded && state.scene.reparent(child, Some(node)).is_ok() {
                        let _ = assign_scene_property(
                            &mut state,
                            child,
                            "visible",
                            SceneValue::Bool(false),
                        );
                        state.retained.dormant_loaders.insert(node);
                        service_changed = true;
                    } else if !preloaded && state.scene.reparent(child, Some(node)).is_ok() {
                        let _ = assign_scene_property(
                            &mut state,
                            node,
                            "active",
                            SceneValue::Bool(true),
                        );
                        let _ = assign_scene_property(
                            &mut state,
                            node,
                            "loading",
                            SceneValue::Bool(false),
                        );
                        let _ = assign_scene_property(
                            &mut state,
                            node,
                            "active_async",
                            SceneValue::Bool(false),
                        );
                        service_changed = true;
                    } else {
                        remove_scene_subtree(&mut state, child);
                        state.retained.loaded_loaders.remove(&node);
                    }
                }
                Err(error) if preloaded => {
                    // Not tried ahead of time again: built when asked for,
                    // where a failure is reported as any Loader's is.
                    let mut state = self.reactive.borrow_mut();
                    state.retained.loaded_loaders.remove(&node);
                    let _ =
                        assign_scene_property(&mut state, node, "preload", SceneValue::Bool(false));
                    state.log(LogLevel::Warn, format!("Loader preload: {error}"));
                }
                Err(error) => {
                    let mut state = self.reactive.borrow_mut();
                    state.retained.loaded_loaders.remove(&node);
                    state.retained.failed_loaders.insert(node);
                    let _ =
                        assign_scene_property(&mut state, node, "loading", SceneValue::Bool(false));
                    let _ = assign_scene_property(
                        &mut state,
                        node,
                        "active_async",
                        SceneValue::Bool(false),
                    );
                    state.log(LogLevel::Warn, format!("Loader: {error}"));
                }
            }
        }
        service_changed
    }
}
