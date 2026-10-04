use morf_system::{GreetdEvent, PamEvent};
use std::time::Duration;

use morf_scene::{NodeHandle, Value as SceneValue};

use crate::runtime::handler::Handler;
use crate::{
    reactive_execute::*, runtime_helpers::*, scene_bindings::*, state::*, surface_types::*,
    types::*, views::*,
};

/// How long a preloading Loader waits for the scene to be still before it
/// builds anyway: an animation that never ends (a spinner, a visualiser)
/// must not hold a preload back for ever.
pub(crate) const PRELOAD_PATIENCE: Duration = Duration::from_millis(1500);

impl Runtime {
    /// Polls native service jobs and runs completed callbacks with bounded fuel.
    pub fn poll_services(&mut self) -> bool {
        // Outside the arena, before this turn's Lua: compile what turned hot.
        self.service_jit();
        let _bookkeeping = crate::profile::span(|| "engine: services bookkeeping".to_owned());
        self.flush_lint();
        // The loop wakes when the caret is due to turn over
        // (`Runtime::next_deadline`), so it blinks without a timer of its own.
        let devices =
            crate::profile::span(|| "engine: appearance, audio, terminals, images".to_owned());
        let blinked = self.blink_text_inputs();
        let appearance_changed = self.poll_appearance();
        let audio_changed = self.poll_audio();
        let terminals_changed = self.poll_terminals();
        let images_changed = self.poll_images();
        let palettes_changed = self.poll_palette_listeners();
        let shared_changed = self.poll_shared();
        let channels_changed = self.poll_channels();
        let focus_changed = self.poll_overlays() | self.check_focus() | self.poll_gestures();
        drop(devices);
        let mut ready = Vec::new();
        let mut timers = Vec::new();
        let mut dbus_signals = Vec::new();
        let mut dbus_replies = Vec::new();
        let mut dbus_calls = Vec::new();
        let mut pam_messages = Vec::new();
        let mut greetd_messages = Vec::new();
        let mut udev_events = Vec::new();
        let mut status_updates = Vec::new();
        let mut http_answers = Vec::new();
        let io_calls;
        let io_more;
        let watch_calls;
        let watch_more;
        let mut loaders = Vec::new();
        let mut preloads = Vec::new();
        let mut loader_drops = Vec::new();
        let mut retained_destroys = Vec::new();
        let mut transform_callbacks = Vec::new();
        let mut service_changed = false;
        {
            let _collect = crate::profile::span(|| "engine: timers, loaders and buses".to_owned());
            let mut state = self.reactive.borrow_mut();
            let mut index = 0;
            while index < state.pam_tasks.len() {
                let result = state.pam_tasks[index].task.wait(Duration::ZERO);
                if let Some(result) = result {
                    let task = state.pam_tasks.swap_remove(index);
                    ready.push((task.callback, task.unlock_on_success, result));
                } else {
                    index += 1;
                }
            }
            // Scene-backed service definitions only need reconciling after
            // a scene write. Native timers still fire and buses still drain
            // below on every poll. Animated Timer intervals invalidate this
            // checkpoint in tick_animations, including their final tick.
            let definitions_changed = state.scene_revision != state.service_definitions_revision;
            state.service_definitions_revision = state.scene_revision;
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
                let current = state
                    .timers
                    .iter()
                    .position(|timer| timer.node == Some(node));
                if !running || duration.is_none() {
                    if let Some(index) = current {
                        state.timers.swap_remove(index);
                        service_changed = true;
                    }
                    continue;
                }
                let duration = duration.expect("validated duration");
                let matches = current.is_some_and(|index| {
                    state.timers[index].interval == duration && state.timers[index].repeat == repeat
                });
                if matches {
                    continue;
                }
                if let Some(index) = current {
                    state.timers.swap_remove(index);
                }
                match state.new_timer(duration) {
                    Ok(timer) => {
                        let id = state.next_timer_id();
                        let origin = state
                            .timer_origins
                            .get(&node)
                            .cloned()
                            .unwrap_or_else(|| format!("ui.Timer {node:?}").into());
                        state.timers.push(PendingTimer {
                            id,
                            timer,
                            callback,
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
                state.timers.retain(|timer| timer.node != Some(node));
                state.timer_origins.remove(&node);
            }
            let loader_definitions = if definitions_changed || !state.preload_pending.is_empty() {
                state
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
                    state.failed_loaders.remove(&node);
                } else if state.failed_loaders.contains(&node) {
                    continue;
                }
                let keep = state.scene.bool_value(node, "keep").unwrap_or(false);
                let preload = state.scene.bool_value(node, "preload").unwrap_or(false);
                // A hidden item stays while the Loader holds it for a reason:
                // kept, or built ahead of being asked for.
                let preloaded_and_waiting = preload && state.dormant_loaders.contains(&node);
                if requested && state.dormant_loaders.remove(&node) {
                    // Built already: shown, not built again.
                    let children = state.scene.children(node).unwrap_or_default().to_vec();
                    for child in children {
                        let _ = assign_scene_property(
                            &mut state,
                            child,
                            "visible",
                            SceneValue::Bool(true),
                        );
                    }
                    let _ =
                        assign_scene_property(&mut state, node, "loading", SceneValue::Bool(false));
                    let _ = assign_scene_property(
                        &mut state,
                        node,
                        "active_async",
                        SceneValue::Bool(false),
                    );
                    if !active {
                        let _ = assign_scene_property(
                            &mut state,
                            node,
                            "active",
                            SceneValue::Bool(true),
                        );
                    }
                    service_changed = true;
                } else if requested && state.loaded_loaders.insert(node) {
                    state.preload_pending.remove(&node);
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
                        && crate::runtime_helpers::cancel_node_exit(&mut state, child)
                    {
                        for (property, value) in [
                            ("loading", false),
                            ("active_async", false),
                            ("active", true),
                        ] {
                            let _ = assign_scene_property(
                                &mut state,
                                node,
                                property,
                                SceneValue::Bool(value),
                            );
                        }
                        service_changed = true;
                    } else {
                        loaders.push((node, factory));
                    }
                } else if !requested
                    && keep
                    && state.loaded_loaders.contains(&node)
                    && !state.dormant_loaders.contains(&node)
                {
                    // Let go, but kept: hidden until it is asked for again.
                    let children = state.scene.children(node).unwrap_or_default().to_vec();
                    for child in children {
                        let _ = assign_scene_property(
                            &mut state,
                            child,
                            "visible",
                            SceneValue::Bool(false),
                        );
                    }
                    state.dormant_loaders.insert(node);
                    service_changed = true;
                } else if !requested
                    && state.loaded_loaders.contains(&node)
                    && !keep
                    && !preloaded_and_waiting
                {
                    state.loaded_loaders.remove(&node);
                    state.dormant_loaders.remove(&node);
                    loader_drops.extend_from_slice(state.scene.children(node).unwrap_or_default());
                    service_changed = true;
                } else if !requested && preload && !state.loaded_loaders.contains(&node) {
                    let since = *state.preload_pending.entry(node).or_insert(now);
                    let waited = now.duration_since(since) >= PRELOAD_PATIENCE;
                    if (still || waited) && preloads.is_empty() {
                        state.preload_pending.remove(&node);
                        state.loaded_loaders.insert(node);
                        preloads.push((node, factory));
                    }
                } else if !preload {
                    state.preload_pending.remove(&node);
                }
            }
            for node in stale_loaders {
                state.loader_factories.remove(&node);
                state.loaded_loaders.remove(&node);
                state.failed_loaders.remove(&node);
                state.dormant_loaders.remove(&node);
                state.preload_pending.remove(&node);
            }
            let mut index = 0;
            let now = state.virtual_now;
            while index < state.timers.len() {
                let interval = state.timers[index].interval;
                if state.timers[index].timer.fire(now, interval) {
                    let timer = &state.timers[index];
                    if wake_log_wanted() {
                        eprintln!(
                            "{} morf: timer {} fired ({:.0} ms{})",
                            crate::profile::stamp(),
                            timer.origin,
                            interval.as_secs_f64() * 1000.0,
                            if timer.repeat { ", repeating" } else { "" }
                        );
                    }
                    timers.push(DueTimer {
                        origin: std::rc::Rc::clone(&timer.origin),
                        id: timer.id,
                        node: timer.node,
                        repeat: timer.repeat,
                        callback: timer.callback.clone(),
                    });
                    if !state.timers[index].repeat {
                        let id = state.timers[index].id;
                        state.due_one_shots.insert(id);
                        if let Some(node) = state.timers[index].node {
                            let _ = assign_scene_property(
                                &mut state,
                                node,
                                "running",
                                SceneValue::Bool(false),
                            );
                        }
                        state.timers.swap_remove(index);
                        continue;
                    }
                }
                index += 1;
            }
            for subscription in &state.dbus_signals {
                while let Some(event) = subscription.signal.next_event(Duration::ZERO) {
                    dbus_signals.push((
                        subscription.id,
                        subscription.callback.clone(),
                        subscription.kind,
                        event,
                    ));
                }
            }
            // An answer leaves the list as it is delivered, and so does one
            // that missed its deadline; the rest wait for a later turn.
            state
                .dbus_replies
                .retain(|entry| match entry.reply.try_take() {
                    Some(reply) => {
                        dbus_replies.push((entry.callback.clone(), reply));
                        false
                    }
                    None => true,
                });
            // A conversation says a few things per turn and then waits on a
            // person, so this never runs long. A finished session leaves the
            // list after its verdict is delivered, which is why the verdict is
            // collected first and the removal follows it.
            state.pam_sessions.retain(|entry| {
                let mut finished = false;
                for _ in 0..8 {
                    let Some(event) = entry.session.borrow_mut().next(Duration::ZERO) else {
                        break;
                    };
                    finished |= matches!(event, PamEvent::Finished(_));
                    pam_messages.push((entry.callback.clone(), event));
                    if finished {
                        break;
                    }
                }
                !finished
            });
            // The same for a greetd login: a few replies per turn, then a
            // wait on greetd, or on a person greetd is waiting on.
            state.greetd_sessions.retain(|entry| {
                let mut conversation = entry.conversation.borrow_mut();
                for _ in 0..8 {
                    let Some(event) = conversation.next(Duration::ZERO) else {
                        break;
                    };
                    let last = matches!(event, GreetdEvent::Failed(_));
                    greetd_messages.push((entry.callback.clone(), event));
                    if last {
                        break;
                    }
                }
                !conversation.ended()
            });
            // Bounded per frame, unlike the signal drain above. A signal that
            // arrives faster than it is read is the sender's problem; a *call*
            // that does is ours, because the caller is blocked until we answer
            // and answering happens after this loop. Taking them all would let
            // one chatty peer hold the frame open.
            // How many calls one service may hand over per frame.
            const MAX_CALLS_PER_FRAME: usize = 32;
            for entry in &state.dbus_services {
                for _ in 0..MAX_CALLS_PER_FRAME {
                    let Some(call) = entry.service.borrow_mut().next_call(Duration::ZERO) else {
                        break;
                    };
                    dbus_calls.push((entry.callback.clone(), call));
                }
            }
            let mut udev_errors = Vec::new();
            for subscription in &mut state.udev_monitors {
                let mut drained = false;
                for _ in 0..32 {
                    match subscription.monitor.next_event(Duration::ZERO) {
                        Ok(Some(event)) => {
                            udev_events.push((subscription.callback.clone(), event));
                        }
                        Ok(None) => {
                            drained = true;
                            break;
                        }
                        Err(error) => {
                            udev_errors.push(error.to_string());
                            drained = true;
                            break;
                        }
                    }
                }
                // This turn's share is taken; the monitor's alarm only rings
                // again once a drain finds the socket empty, so the rest is
                // asked for now.
                if !drained {
                    morf_io::wake_all();
                }
            }
            for error in udev_errors {
                state.log(LogLevel::Warn, format!("udev: {error}"));
            }
            let mut status_errors = Vec::new();
            for subscription in &mut state.status_notifiers {
                match subscription.host.poll_changed() {
                    Ok(Some(items)) => status_updates.push((subscription.callback.clone(), items)),
                    Ok(None) => {}
                    Err(error) => status_errors.push(error.to_string()),
                }
            }
            for error in status_errors {
                state.log(LogLevel::Warn, format!("status notifier: {error}"));
            }
            // A cancelled request leaves here without a word, and dropping
            // its task stops the worker; a finished one leaves with its
            // answer, which is delivered below once the state is released.
            state.http_requests.retain_mut(|entry| {
                if entry.handle.cancelled.get() {
                    return false;
                }
                let Some(outcome) = entry.task.poll() else {
                    return true;
                };
                if let Some(callback) = entry.callback.take() {
                    http_answers.push((
                        callback,
                        outcome,
                        std::mem::take(&mut entry.url),
                        entry.json.clone(),
                        std::rc::Rc::clone(&entry.handle),
                    ));
                } else {
                    entry.handle.done.set(true);
                }
                false
            });
            (io_calls, io_more) = state.io.collect();
            (watch_calls, watch_more) = state.watches.collect();
            retained_destroys.extend(state.retained_destroy_queue.drain());
            for watcher in state.transform_watchers.values_mut() {
                if watcher.pending {
                    watcher.pending = false;
                    if let Some(callback) = &watcher.callback {
                        transform_callbacks.push((callback.clone(), watcher.revision));
                    }
                }
            }
        }
        let letting_go = crate::profile::span(|| "engine: letting go of nodes".to_owned());
        for node in retained_destroys {
            self.lua
                .enter(|ctx| finish_retained_destroy(&self.reactive, ctx, self.limits, node));
            service_changed = true;
        }
        for &node in &loader_drops {
            self.lua
                .enter(|ctx| drop_retainable(&self.reactive, ctx, self.limits, node));
        }
        drop(letting_go);
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
                        state.dormant_loaders.insert(node);
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
                        state.loaded_loaders.remove(&node);
                    }
                }
                Err(error) if preloaded => {
                    // Not tried ahead of time again: built when asked for,
                    // where a failure is reported as any Loader's is.
                    let mut state = self.reactive.borrow_mut();
                    state.loaded_loaders.remove(&node);
                    let _ =
                        assign_scene_property(&mut state, node, "preload", SceneValue::Bool(false));
                    state.log(LogLevel::Warn, format!("Loader preload: {error}"));
                }
                Err(error) => {
                    let mut state = self.reactive.borrow_mut();
                    state.loaded_loaders.remove(&node);
                    state.failed_loaders.insert(node);
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
        service_changed |= {
            let _span = crate::profile::span(|| "engine: views and repeaters".to_owned());
            self.sync_pending_views()
        };
        // Whether a repaint is owed is decided after the callbacks below have
        // run, by asking whether the scene actually changed. A callback merely
        // firing is not a reason to render: a 16ms timer that polls a file and
        // finds it unchanged would otherwise force a full render of every
        // output sixty times a second, forever.
        let (revision_before, hidden_before) = {
            let state = self.reactive.borrow();
            (state.scene_revision, state.hidden_revisions)
        };
        // Timers, loaders and views are in step with the scene as it is now;
        // what the callbacks below change is for the next turn to pick up.
        self.reactive.borrow_mut().polled_revision = revision_before;
        let service_changed = service_changed
            || appearance_changed
            || audio_changed
            || terminals_changed
            || images_changed
            || palettes_changed
            || shared_changed
            || channels_changed
            || focus_changed
            || blinked
            || !transform_callbacks.is_empty();
        for (callback, unlock_on_success, result) in ready {
            if unlock_on_success && result.is_ok() {
                self.reactive.borrow_mut().session_unlock_requested = true;
            }
            let args = match result {
                Ok(()) => vec![IpcValue::Boolean(true), IpcValue::Nil],
                Err(error) => vec![
                    IpcValue::Boolean(false),
                    IpcValue::String(error.to_string()),
                ],
            };
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, &callback, &args, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("PAM callback: {message}"));
            }
        }
        for (callback, outcome, url, json, handle) in http_answers {
            let _span = crate::profile::span(|| "http callback".to_owned());
            // Cancelled by an earlier callback in this same batch.
            if handle.cancelled.get() {
                continue;
            }
            handle.done.set(true);
            if let Err(message) = self.run_handler(|ctx, limits| {
                crate::api_http::execute_http_handler(ctx, &callback, outcome, &url, &json, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("http callback: {message}"));
            }
        }
        for call in &io_calls {
            let _span = crate::profile::span(|| "I/O callback".to_owned());
            if let Err(message) =
                self.run_handler(|ctx, limits| crate::api_io::execute_io_call(ctx, call, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("I/O callback: {message}"));
            }
        }
        for call in &watch_calls {
            let _span = crate::profile::span(|| "fs.watch callback".to_owned());
            if let Err(message) = self
                .run_handler(|ctx, limits| crate::api_watch::execute_watch_call(ctx, call, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("fs.watch callback: {message}"));
            }
        }
        if io_more || watch_more {
            // One turn's share is spent; the rest is for the next turn,
            // which this makes come at once rather than at the next event.
            morf_io::wake_all();
        }
        for DueTimer {
            origin,
            id,
            node,
            repeat,
            callback,
        } in timers
        {
            let _span = crate::profile::span(|| format!("timer {origin}"));
            // Collected before this turn's other callbacks, loader drops and
            // earlier timers ran, any of which may have stopped this one or
            // torn its node down. A timer fires only if it is still wanted.
            if !timer_still_due(&mut self.reactive.borrow_mut(), id, node, repeat) {
                continue;
            }
            if let Err(message) =
                self.run_handler(|ctx, limits| execute_handler_args(ctx, &callback, &[], limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("timer callback: {message}"));
            }
        }
        for (callback, revision) in transform_callbacks {
            let _span = crate::profile::span(|| "transform callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(
                    ctx,
                    &callback,
                    &[IpcValue::Integer(revision as i64)],
                    limits,
                )
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("transform callback: {message}"));
            }
        }
        for (callback, event) in pam_messages {
            let _span = crate::profile::span(|| "PAM session".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_pam_session_handler(ctx, &callback, event, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("PAM session: {message}"));
            }
        }
        for (callback, event) in greetd_messages {
            if let Err(message) = self
                .run_handler(|ctx, limits| execute_greetd_handler(ctx, &callback, event, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("greetd: {message}"));
            }
        }
        for (callback, call) in dbus_calls {
            let _span = crate::profile::span(|| "D-Bus call handler".to_owned());
            if let Err(message) = self
                .run_handler(|ctx, limits| execute_dbus_call_handler(ctx, &callback, call, limits))
            {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("D-Bus call: {message}"));
            }
        }
        for (callback, reply) in dbus_replies {
            let _span = crate::profile::span(|| "D-Bus reply callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_reply_handler(ctx, &callback, reply, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("D-Bus reply callback: {message}"));
            }
        }
        for (id, callback, kind, event) in dbus_signals {
            let _span = crate::profile::span(|| "D-Bus signal callback".to_owned());
            // Closed by an earlier callback in this same batch: what was
            // already read for it is not delivered, because "after close,
            // nothing" is the promise `close` makes.
            if !self
                .reactive
                .borrow()
                .dbus_signals
                .iter()
                .any(|subscription| subscription.id == id)
            {
                continue;
            }
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_signal_handler(ctx, &callback, event, kind, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("D-Bus signal: {message}"));
            }
        }
        for (callback, event) in udev_events {
            let _span = crate::profile::span(|| "udev callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_handler(ctx, &callback, udev_event_value(event), limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("udev callback: {message}"));
            }
        }
        for (callback, items) in status_updates {
            let _span = crate::profile::span(|| "status notifier callback".to_owned());
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_dbus_handler(ctx, &callback, status_notifier_value(items), limits)
            }) {
                self.reactive.borrow_mut().log(
                    LogLevel::Warn,
                    format!("status notifier callback: {message}"),
                );
            }
        }
        {
            let _span = crate::profile::span(|| "engine: image jobs".to_owned());
            self.poll_image_jobs();
        }
        // A source set outright, not animated, is followed at once too.
        crate::state::apply_follows(&mut self.reactive.borrow_mut());
        // Changes to nodes nothing shows are not painted: a hidden panel's
        // chart that follows a counter every second would otherwise draw
        // every output every second for a picture nobody sees.
        let state = self.reactive.borrow();
        let bumps = state.scene_revision.wrapping_sub(revision_before);
        let hidden = state.hidden_revisions.wrapping_sub(hidden_before);
        service_changed || bumps > hidden
    }
}

impl Runtime {
    /// Takes the nodes destroyed since the last frame, and drops what this
    /// crate holds for them on the way past.
    ///
    /// The transform tracker is reachable from here; the caches in the render
    /// backend are not, so the list is handed back for the caller to finish
    /// the job. Nobody else has both the scene and those caches in scope.
    pub fn take_removed_nodes(&self) -> Vec<NodeHandle> {
        let mut state = self.reactive.borrow_mut();
        let removed = state.scene.take_removed_nodes();
        if !removed.is_empty() {
            let ReactiveState {
                scene,
                transform_tracker,
                ..
            } = &mut *state;
            transform_tracker.retain_scene(&*scene);
        }
        removed
    }
}

/// A timer that came due this turn, as collected before any callback ran.
struct DueTimer {
    origin: std::rc::Rc<str>,
    id: u64,
    node: Option<NodeHandle>,
    repeat: bool,
    callback: Handler,
}

/// Whether a timer collected as due should still fire, now that everything
/// before it in this turn has run.
///
/// A `Timer` node must still exist and still be a timer — not removed,
/// alone or with an ancestor such as a Loader letting its item go — and a
/// repeating one must still be running. A timer with no node must not have
/// been cancelled. A one-shot fires at most once either way.
fn timer_still_due(
    state: &mut ReactiveState,
    id: u64,
    node: Option<NodeHandle>,
    repeat: bool,
) -> bool {
    let one_shot_pending = !repeat && state.due_one_shots.remove(&id);
    if let Some(node) = node {
        if !state.timer_callbacks.contains_key(&node) {
            return false;
        }
        let Ok(running) = state.scene.bool_value(node, "running") else {
            return false;
        };
        if repeat && !running {
            return false;
        }
    }
    if repeat {
        state.timers.iter().any(|timer| timer.id == id)
    } else {
        one_shot_pending
    }
}

/// Whether `MORF_WAKE_LOG` asks for wakes -- and so the timers behind them --
/// to be named.
fn wake_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| {
        std::env::var_os("MORF_WAKE_LOG").is_some_and(|value| !value.is_empty() && value != "0")
    })
}
