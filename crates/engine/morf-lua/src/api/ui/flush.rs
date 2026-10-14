//! The reactive flush from Lua's side: effects run one at a time with the
//! state borrowed only between them, their errors logged once per effect,
//! and the `on_destroyed` hooks a flush leaves behind.

use luna::Context;
use morf_scene::NodeHandle;
use std::rc::Rc;

use crate::{reactive_execute::*, state::*, types::*};

/// How many times one flush re-runs to take in effects registered by the
/// flush before it -- an effect that builds a node with a binding whose
/// evaluation builds another, and so on. Past this the rest wait for the
/// next flush rather than spinning here forever.
const MAX_NESTED_FLUSHES: usize = 32;

pub(crate) fn flush_reactive(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
) -> Result<(), String> {
    let mut result = flush_graph(state, ctx, limits);
    // Anything registered while the graph was away -- a path that still
    // takes it for a moment -- is registered now and gets its first run
    // here, so the caller sees it evaluated.
    for _ in 0..MAX_NESTED_FLUSHES {
        if state.borrow_mut().reactive.register_pending_effects() == 0 {
            break;
        }
        let next = flush_graph(state, ctx, limits);
        if result.is_ok() {
            result = next;
        }
    }
    // A flush is where most removals happen — a Loader let go, a Repeater's
    // row gone — and the end of one is the first moment their hooks can run.
    run_destroyed_hooks(state, ctx, limits);
    result
}

/// Runs the `on_destroyed` hooks of nodes that have been removed.
///
/// Never from inside a flush, and never from inside another hook: a removal
/// happens with the state borrowed, so its hooks wait in a queue for the next
/// moment nothing holds it. Each hook is a handler — its own fuel, its writes
/// flushed once when the last of them returns — and a node its hook removes
/// has its own hook queued and run in the same drain.
pub(crate) fn run_destroyed_hooks(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
) {
    {
        let mut state = state.borrow_mut();
        if state.reactive.flushing || !state.destroy_hooks.begin() {
            return;
        }
        state.tracking.enter_handler();
    }
    loop {
        let hooks = state.borrow_mut().destroy_hooks.take_pending();
        if hooks.is_empty() {
            break;
        }
        for hook in hooks {
            if let Err(error) = execute_handler_args(ctx, &hook, &[], limits) {
                state
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("on_destroyed: {error}"));
            }
        }
    }
    let flush = {
        let mut state = state.borrow_mut();
        state.destroy_hooks.end();
        state.tracking.leave_handler()
    };
    if flush {
        let _ = flush_reactive(state, ctx, limits);
    }
}

fn flush_graph(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
) -> Result<(), String> {
    // A flush asked for from inside one -- a binding that registers another,
    // a module `require`d from a binding writing a signal -- has nothing to
    // do of its own: the flush already under way drains it.
    let Some(mut run) = state.borrow_mut().reactive.begin_flush(limits.frame_fuel)? else {
        return Ok(());
    };
    // Driven one effect at a time, the state borrowed only between the
    // steps: an effect is Lua, and Lua may create signals or effects of its
    // own while it runs.
    let stepped = loop {
        let next = state.borrow_mut().reactive.next_effect(&mut run);
        let pending = match next {
            Ok(Some(pending)) => pending,
            Ok(None) => break Ok(()),
            Err(error) => break Err(error),
        };
        let mut capture = morf_scene::reactive::EffectCapture::default();
        let _span = crate::profile::span(|| effect_label(state, ctx, &pending));
        let outcome = evaluate_effect(
            state,
            ctx,
            limits,
            &mut run.remaining,
            pending.token(),
            &mut capture,
        );
        let completed = state
            .borrow_mut()
            .reactive
            .complete_effect(&mut run, pending, capture, outcome);
        if let Err(error) = completed {
            break Err(error);
        }
    };

    let _span =
        crate::profile::span(|| "engine: after a flush (signal copies, graph gc)".to_owned());
    let mut state = state.borrow_mut();
    let result = state.reactive.finish_flush(run, stepped);
    state.collect_graph_garbage();
    // The effects' own errors are logged here, once per effect and its
    // repeats counted, so the callers of a flush do not log them again.
    match result {
        Ok(errors) => {
            for line in state.reactive.errors.note(&errors) {
                state.log(LogLevel::Warn, line);
            }
            Ok(())
        }
        Err(message) => {
            state.log(LogLevel::Warn, message.clone());
            Err(message)
        }
    }
}

/// A flush's outcome as `signal:set` reports it: an error any effect it ran
/// raised, as well as one of the flush itself.
pub(crate) fn flush_status(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
) -> Result<(), String> {
    state.borrow_mut().reactive.errors.take_last();
    flush_reactive(state, ctx, limits)?;
    match state.borrow_mut().reactive.errors.take_last() {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

/// What an effect is, for the profiler: a binding by the node path and
/// property it drives, anything else by the name it was registered under,
/// and either by where its function was written.
fn effect_label(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'_>,
    pending: &morf_scene::reactive::PendingEffect,
) -> String {
    let state = state.borrow();
    let Some(effect) = state.reactive.effects.get(&pending.token()) else {
        return format!("effect {}", pending.name());
    };
    let origin = match crate::vm::handler_store::stashed(&effect.handler) {
        Ok(closure) => crate::profile::closure_origin(ctx.fetch(&closure)),
        Err(error) => error,
    };
    let node_path = |node: NodeHandle| {
        let id = state
            .scene
            .string_value(node, "id")
            .ok()
            .filter(|id| !id.is_empty())
            .map(|id| format!(" #{id}"))
            .unwrap_or_default();
        format!(
            "{}{id}",
            crate::runtime_config::lint_path(&state.scene, node)
        )
    };
    match &effect.sink {
        Some(EffectSink::Property(sink)) => {
            format!(
                "binding {}.{} ({origin})",
                node_path(sink.node),
                sink.property
            )
        }
        Some(EffectSink::State(node)) => format!("binding {}.state ({origin})", node_path(*node)),
        Some(EffectSink::Loop(node)) => format!("binding {}.loop ({origin})", node_path(*node)),
        None => format!("effect {} ({origin})", pending.name()),
    }
}
