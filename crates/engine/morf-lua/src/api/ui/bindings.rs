use crate::runtime::states::StateValue;
use luna::{Closure, Context, Value as LuaValue};
use std::rc::Rc;

use morf_scene::{NodeHandle, Value as SceneValue};

use crate::{
    api::ui::scene::*, runtime::host_types::*, runtime::state::*, vm::execute::*, vm::types::*,
};

pub(crate) use super::flush::{flush_reactive, flush_status, run_destroyed_hooks};

pub(crate) fn register_property_binding<'gc>(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
    node: NodeHandle,
    property: String,
    closure: Closure<'gc>,
) {
    let name = format!("{node:?}.{property}");
    {
        let mut state = state.borrow_mut();
        let token = state.tracking_mut().next_effect_token();
        state.reactive.add_effect(
            token,
            LuaEffect {
                handler: crate::vm::handler_store::register(ctx.stash(closure)),
                sink: Some(EffectSink::Property(PropertySink { node, property })),
                owner: None,
            },
        );
        state.register_external_effect(token, name);
    }
    let _span = crate::profile::span(|| "engine: a binding's first run".to_owned());
    let _ = flush_reactive(state, ctx, limits);
}

/// A binding given after construction (`node.width = function() ... end`):
/// it takes the place of any the property had, and runs at once.
pub(crate) fn rebind_property<'gc>(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'gc>,
    node: NodeHandle,
    property: String,
    closure: Closure<'gc>,
) {
    let limits = {
        let mut state = state.borrow_mut();
        let old = state
            .reactive
            .effects()
            .filter(|(_, effect)| {
                matches!(&effect.sink, Some(EffectSink::Property(sink))
                    if sink.node == node && sink.property == property)
            })
            .map(|(token, _)| token)
            .collect::<Vec<_>>();
        for token in old {
            crate::api::ui::retention::dispose_effect(&mut state, token);
        }
        state.limits
    };
    register_property_binding(state, ctx, limits, node, property, closure);
}

pub(crate) fn register_state_binding<'gc>(
    state: &Rc<StateCell<ReactiveState>>,
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
    state: &Rc<StateCell<ReactiveState>>,
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
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
    node: NodeHandle,
    closure: Closure<'gc>,
    sink: EffectSink,
    what: &str,
) {
    {
        let mut state = state.borrow_mut();
        let token = state.tracking_mut().next_effect_token();
        state.reactive.add_effect(
            token,
            LuaEffect {
                handler: crate::vm::handler_store::register(ctx.stash(closure)),
                sink: Some(sink),
                owner: None,
            },
        );
        state.register_external_effect(token, format!("{node:?}.{what}"));
    }
    let _ = flush_reactive(state, ctx, limits);
}

pub(crate) fn apply_state(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    frame_remaining: &mut u64,
    node: NodeHandle,
    name: &str,
) -> Result<(), String> {
    let (definition, old, transition) = {
        let state = state.borrow();
        let set = state
            .states()
            .get(node)
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
        let animated = transition.filter(|_| {
            matches!(value, SceneValue::Number(_) | SceneValue::Color(_))
                && matches!(
                    state.scene().current(node, &property),
                    Ok(SceneValue::Number(_) | SceneValue::Color(_))
                )
        });
        if let Some(transition) = animated {
            let from = state
                .scene()
                .current(node, &property)
                .map_err(|error| error.to_string())?
                .clone();
            animate_scene_property(&mut state, node, &property, from, value, transition)?;
        } else {
            assign_scene_property(&mut state, node, &property, value)?;
        }
    }
    if old != name && (definition.parent.is_some() || definition.anchors.is_some()) {
        let parent = definition.parent.or(state
            .scene()
            .parent(node)
            .map_err(|error| error.to_string())?);
        if let Some(parent) = parent {
            if old.is_empty() && transition.is_none() {
                if let Some(anchors) = definition.anchors {
                    assign_scene_property(&mut state, node, "anchors", SceneValue::Map(anchors))?;
                }
                state
                    .scene_mut()
                    .reparent(node, Some(parent))
                    .map_err(|error| error.to_string())?;
            } else {
                state
                    .windows_mut()
                    .parent_transitions
                    .push(ParentTransitionRequest {
                        node,
                        parent,
                        anchors: definition.anchors,
                        behavior: transition.unwrap_or_default(),
                    });
            }
        }
    }
    // A binding above may have removed the node, and its states with it.
    if !state.states_mut().enter(node, name) {
        return Err(format!("node lost its states while entering `{name}`"));
    }
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
            match userdata.downcast_static::<crate::api::color::color::ColorToken>() {
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
