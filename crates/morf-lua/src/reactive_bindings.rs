use crate::states::StateValue;
use luna::{Closure, Context, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use morf_scene::{NodeHandle, Value as SceneValue};

use crate::{reactive_execute::*, scene_bindings::*, state::*, surface_types::*, types::*};

pub(crate) fn register_property_binding<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
    node: NodeHandle,
    property: String,
    closure: Closure<'gc>,
) {
    let name = format!("{node:?}.{property}");
    {
        let mut state = state.borrow_mut();
        let token = state.next_effect;
        state.next_effect = state.next_effect.wrapping_add(1);
        state.effects.insert(
            token,
            LuaEffect {
                closure: ctx.stash(closure),
                sink: Some(EffectSink::Property(PropertySink { node, property })),
            },
        );
        state.register_external_effect(token, name);
    }
    let _ = flush_reactive(state, ctx, limits);
}

pub(crate) fn register_state_binding<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
    node: NodeHandle,
    closure: Closure<'gc>,
) {
    register_node_binding(
        state,
        ctx,
        limits,
        node,
        closure,
        EffectSink::State(node),
        "state",
    );
}

/// A `loop` given as a function: each value it returns is the node's loops.
pub(crate) fn register_loop_binding<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
    node: NodeHandle,
    closure: Closure<'gc>,
) {
    register_node_binding(
        state,
        ctx,
        limits,
        node,
        closure,
        EffectSink::Loop(node),
        "loop",
    );
}

fn register_node_binding<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
    node: NodeHandle,
    closure: Closure<'gc>,
    sink: EffectSink,
    what: &str,
) {
    {
        let mut state = state.borrow_mut();
        let token = state.next_effect;
        state.next_effect = state.next_effect.wrapping_add(1);
        state.effects.insert(
            token,
            LuaEffect {
                closure: ctx.stash(closure),
                sink: Some(sink),
            },
        );
        state.register_external_effect(token, format!("{node:?}.{what}"));
    }
    let _ = flush_reactive(state, ctx, limits);
}

pub(crate) fn apply_state(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    frame_remaining: &mut u64,
    node: NodeHandle,
    name: &str,
) -> Result<(), String> {
    let (definition, old, transition) = {
        let state = state.borrow();
        let set = state
            .states
            .get(&node)
            .ok_or_else(|| format!("node has no states for `{name}`"))?;
        let definition = set
            .definitions
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown state `{name}`"))?;
        let old = set.current.clone().unwrap_or_default();
        let transition = set.transitions.iter().find_map(|transition| {
            let forward = (transition.from == "*" || transition.from == old)
                && (transition.to == "*" || transition.to == name);
            let reverse = transition.reversible
                && (transition.from == "*" || transition.from == name)
                && (transition.to == "*" || transition.to == old);
            (forward || reverse).then_some(transition.behavior)
        });
        (definition, old, transition)
    };
    let transition = (old != name).then_some(transition).flatten();
    let mut properties = Vec::new();
    for (property, value) in definition.properties {
        let value = match value {
            StateValue::Value(value) => value,
            StateValue::Binding(closure) => {
                execute_effect(ctx, &closure, limits, frame_remaining, true)?
                    .ok_or_else(|| format!("state property `{property}` returned no value"))?
            }
        };
        properties.push((property, value));
    }
    let mut state = state.borrow_mut();
    for (property, value) in properties {
        let animated = transition.is_some()
            && matches!(value, SceneValue::Number(_) | SceneValue::Color(_))
            && matches!(
                state.scene.current(node, &property),
                Ok(SceneValue::Number(_) | SceneValue::Color(_))
            );
        if animated {
            let from = state
                .scene
                .current(node, &property)
                .map_err(|error| error.to_string())?
                .clone();
            animate_scene_property(
                &mut state,
                node,
                &property,
                from,
                value,
                transition.unwrap(),
            )?;
        } else {
            assign_scene_property(&mut state, node, &property, value)?;
        }
    }
    if old != name && (definition.parent.is_some() || definition.anchors.is_some()) {
        let parent = definition.parent.or(state
            .scene
            .parent(node)
            .map_err(|error| error.to_string())?);
        if let Some(parent) = parent {
            if old.is_empty() && transition.is_none() {
                if let Some(anchors) = definition.anchors {
                    assign_scene_property(&mut state, node, "anchors", SceneValue::Map(anchors))?;
                }
                state
                    .scene
                    .reparent(node, Some(parent))
                    .map_err(|error| error.to_string())?;
            } else {
                state.parent_transitions.push(ParentTransitionRequest {
                    node,
                    parent,
                    anchors: definition.anchors,
                    behavior: transition.unwrap_or_default(),
                });
            }
        }
    }
    state.states.get_mut(&node).unwrap().current = Some(name.to_owned());
    Ok(())
}

pub(crate) fn lua_to_scene<'gc>(
    ctx: Context<'gc>,
    value: LuaValue<'gc>,
    depth: usize,
) -> Result<SceneValue, String> {
    if depth >= 16 {
        return Err("declarative value nesting exceeds 16 levels".to_owned());
    }
    match value {
        LuaValue::Nil => Ok(SceneValue::Nil),
        LuaValue::Boolean(value) => Ok(SceneValue::Bool(value)),
        LuaValue::Integer(value) => Ok(SceneValue::Number(value as f64)),
        LuaValue::Number(value) if value.is_finite() => Ok(SceneValue::Number(value)),
        LuaValue::String(value) => Ok(SceneValue::String(value.display_lossy().to_string())),
        LuaValue::UserData(userdata) => {
            match userdata.downcast_static::<crate::api_color::ColorToken>() {
                Ok(token) => Ok(SceneValue::Color(morf_scene::Color::from_pastel(
                    &token.color,
                ))),
                Err(_) => Err("a property cannot hold this value".to_owned()),
            }
        }
        LuaValue::Table(table) => {
            let entries: Vec<_> = table.iter(ctx).collect();
            let is_list = entries
                .iter()
                .all(|(key, _)| matches!(key, LuaValue::Integer(index) if *index > 0));
            if is_list {
                let mut items = entries
                    .into_iter()
                    .map(|(key, value)| {
                        let LuaValue::Integer(index) = key else {
                            unreachable!()
                        };
                        Ok((index, lua_to_scene(ctx, value, depth + 1)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                items.sort_by_key(|(index, _)| *index);
                Ok(SceneValue::List(
                    items.into_iter().map(|(_, value)| value).collect(),
                ))
            } else {
                let mut map = std::collections::BTreeMap::new();
                for (key, value) in entries {
                    let LuaValue::String(key) = key else {
                        return Err("declarative maps require string keys".to_owned());
                    };
                    map.insert(
                        key.display_lossy().to_string(),
                        lua_to_scene(ctx, value, depth + 1)?,
                    );
                }
                Ok(SceneValue::Map(map))
            }
        }
        value => Err(format!(
            "scene properties do not support {} values",
            value.type_name()
        )),
    }
}

pub(crate) fn replace_status<'gc>(
    ctx: Context<'gc>,
    stack: &mut luna::Stack<'gc, '_>,
    result: Result<(), String>,
) {
    match result {
        Ok(()) => stack.replace(ctx, (true, LuaValue::Nil)),
        Err(message) => stack.replace(ctx, (false, message)),
    }
}

/// How many times one flush re-runs to take in effects registered by the
/// flush before it -- an effect that builds a node with a binding whose
/// evaluation builds another, and so on. Past this the rest wait for the
/// next flush rather than spinning here forever.
const MAX_NESTED_FLUSHES: usize = 32;

pub(crate) fn flush_reactive(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
) -> Result<(), String> {
    let mut result = flush_graph(state, ctx, limits);
    // Anything registered while the graph was away -- a path that still
    // takes it for a moment -- is registered now and gets its first run
    // here, so the caller sees it evaluated.
    for _ in 0..MAX_NESTED_FLUSHES {
        if state.borrow_mut().register_pending_effects() == 0 {
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
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
) {
    {
        let mut state = state.borrow_mut();
        if state.flushing || state.running_destroyed || state.pending_destroyed.is_empty() {
            return;
        }
        state.running_destroyed = true;
        state.handler_depth += 1;
    }
    loop {
        let hooks = std::mem::take(&mut state.borrow_mut().pending_destroyed);
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
        state.running_destroyed = false;
        state.handler_depth = state.handler_depth.saturating_sub(1);
        state.handler_depth == 0 && std::mem::take(&mut state.flush_pending)
    };
    if flush {
        let _ = flush_reactive(state, ctx, limits);
    }
}

fn flush_graph(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
) -> Result<(), String> {
    {
        let mut state = state.borrow_mut();
        // A flush asked for from inside one — a binding that registers
        // another, a module `require`d from a binding writing a signal — has
        // nothing to do of its own: whatever it would have run is dirty in
        // the graph, and the flush already under way drains it.
        if state.flushing {
            return Ok(());
        }
        if state.graph.is_none() {
            return Err("reactive graph unavailable".to_owned());
        }
        state.flushing = true;
    }
    // Driven one effect at a time, with the graph left in the state between
    // the steps rather than taken out for the whole flush. An effect is Lua,
    // and Lua may create signals or effects of its own while it runs — the
    // first `require` of a module that holds state does — so the graph has to
    // be where Lua can reach it.
    let mut flush = morf_reactive::Flush::default();
    let mut remaining = limits.frame_fuel;
    let result = loop {
        let next = state
            .borrow_mut()
            .graph
            .as_mut()
            .expect("the graph stays in place during a flush")
            .next_effect(&mut flush);
        let pending = match next {
            Ok(Some(pending)) => pending,
            Ok(None) => break Ok(()),
            Err(error) => break Err(error),
        };
        let mut capture = morf_reactive::EffectCapture::default();
        let outcome = evaluate_effect(
            state,
            ctx,
            limits,
            &mut remaining,
            pending.token(),
            &mut capture,
        );
        state
            .borrow_mut()
            .graph
            .as_mut()
            .expect("the graph stays in place during a flush")
            .complete_effect(&mut flush, pending, capture, outcome);
    };
    let result = result.map(|()| flush.finish());

    let mut state = state.borrow_mut();
    state.flushing = false;
    let graph = state
        .graph
        .take()
        .expect("the graph stays in place during a flush");
    for signal in state.signals.clone() {
        if let Ok(value) = graph.read(signal) {
            state.values.insert(signal, value.clone());
        }
    }
    state.graph = Some(graph);
    state.collect_graph_garbage();

    match result {
        Ok(report) if report.errors.is_empty() => Ok(()),
        Ok(report) => {
            let message = report
                .errors
                .into_iter()
                .map(|error| format!("{}: {}", error.effect, error.message))
                .collect::<Vec<_>>()
                .join("; ");
            state.log(LogLevel::Warn, message.clone());
            Err(message)
        }
        Err(error) => {
            let message = error.to_string();
            state.log(LogLevel::Warn, message.clone());
            Err(message)
        }
    }
}
