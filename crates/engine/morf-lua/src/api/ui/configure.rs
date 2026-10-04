//! A node configured from its constructor table: its properties, handlers,
//! engine relations, `enter`, `exit` and `behaviors`.

use crate::api_shader::attach_shader;
use crate::configure_states::configure_states;
use luna::{Context, Function, Table, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use morf_scene::{Behavior, NodeHandle, Physics, Repeat, RotationDirection};

use crate::{
    events::*, lua_values::*, reactive_bindings::*, scene_bindings::*, state::*, table_menu::*,
    types::*,
};

mod behaviors;
mod entrance;

pub(crate) use behaviors::{
    configure_behaviors, parse_color_space, parse_hue, parse_repeat, parse_rotation_direction,
};
use entrance::{configure_exit, enter_values};

pub(crate) fn configure_element<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
    node: NodeHandle,
    properties: Table<'gc>,
) -> Result<(), String> {
    let entries: Vec<_> = properties.iter(ctx).collect();
    let mut children = Vec::<(i64, NodeHandle)>::new();
    let mut named = Vec::<(String, LuaValue<'gc>)>::new();
    let mut state_value = None;
    for (key, value) in entries {
        match key {
            LuaValue::Integer(index) => {
                let LuaValue::UserData(child) = value else {
                    return Err(format!("child {index} must be a morf node"));
                };
                let child = child
                    .downcast_static::<NodeToken>()
                    .map_err(|_| format!("child {index} must be a morf node"))?;
                children.push((index, child.handle));
            }
            LuaValue::String(property) => {
                named.push((property.display_lossy().to_string(), value));
            }
            value => {
                return Err(format!(
                    "element table key must be a string or integer, found {}",
                    value.type_name()
                ));
            }
        }
    }
    let named_behavior = named
        .iter()
        .find(|(name, _)| name == "behavior")
        .map(|(_, value)| *value);
    let named_enter = named
        .iter()
        .find(|(name, _)| name == "enter")
        .map(|(_, value)| *value);
    let named_loop = named
        .iter()
        .find(|(name, _)| name == "loop")
        .map(|(_, value)| *value);
    let named_exit = named
        .iter()
        .find(|(name, _)| name == "exit")
        .map(|(_, value)| *value);
    let mut state_selector = None;
    if let Some((_, states)) = named.iter().find(|(name, _)| name == "states") {
        let transitions = named
            .iter()
            .find(|(name, _)| name == "transitions")
            .map_or(LuaValue::Nil, |(_, value)| *value);
        state_selector = configure_states(state, ctx, limits, node, *states, transitions)?;
    }
    // A shader is resolved here rather than kept as a property: the name is
    // looked up once, at configuration time, so painting never consults a
    // registry and a name that does not resolve is reported where it was
    // written.
    if let Some((_, LuaValue::String(name))) = named.iter().find(|(key, _)| key == "shader") {
        let overrides = named
            .iter()
            .find(|(key, _)| key == "shader_params")
            .and_then(|(_, value)| match value {
                LuaValue::Table(table) => Some(*table),
                _ => None,
            });
        attach_shader(
            state,
            ctx,
            node,
            &name.display_lossy().to_string(),
            overrides,
        )?;
    }
    for (property, value) in named {
        if matches!(
            property.as_str(),
            "behavior"
                | "states"
                | "transitions"
                | "shader"
                | "shader_params"
                | "enter"
                | "exit"
                | "loop"
        ) {
            continue;
        }
        if property == "state" {
            state_value = Some(value);
            continue;
        }
        if property == "on_destroyed" {
            let LuaValue::Function(Function::Closure(closure)) = value else {
                return Err("on_destroyed must be a function".to_owned());
            };
            state
                .borrow_mut()
                .destroy_hooks
                .insert(node, crate::vm::handler_store::register(ctx.stash(closure)));
            continue;
        }
        if matches!(property.as_str(), "stretch" | "track" | "shortcuts")
            || (property == "mask" && !matches!(value, LuaValue::Function(_)))
        {
            assign_engine_relation(&mut state.borrow_mut(), ctx, node, &property, value)?;
            continue;
        }
        refuse_runtime_owned(&state.borrow(), node, &property)?;
        if let Some(event) = handler_event(&property) {
            let LuaValue::Function(Function::Closure(closure)) = value else {
                return Err(format!("{property} must be a function"));
            };
            state.borrow_mut().events.set(
                node,
                event,
                Some(crate::vm::handler_store::register(ctx.stash(closure))),
            );
            continue;
        }
        // A table holding bindings among its fields is bound as a whole.
        let value = match value {
            LuaValue::Table(table) if crate::configure_states::binds_inside(&property) => {
                match crate::configure_states::table_binding(ctx, table, limits)? {
                    Some(closure) => LuaValue::Function(Function::Closure(closure)),
                    None => value,
                }
            }
            _ => value,
        };
        if let LuaValue::Function(Function::Closure(closure)) = value {
            if !state
                .borrow()
                .scene
                .has_property(node, &property)
                .map_err(|error| error.to_string())?
            {
                let element = state
                    .borrow()
                    .scene
                    .element(node)
                    .map_err(|error| error.to_string())?;
                return Err(format!("unknown {element:?} property `{property}`"));
            }
            register_property_binding(state, ctx, limits, node, property, closure);
        } else {
            let value = lua_to_scene(ctx, value, 0)?;
            assign_scene_property(&mut state.borrow_mut(), node, &property, value)?;
        }
    }
    // Behaviors are installed only once every declared property has been
    // assigned. A behavior intercepts writes, so installing it first would make
    // an element animate its own construction — every colour easing up from the
    // schema default, every width growing from zero — which is a flash on
    // startup, not a transition. Qt's `Behavior` withholds itself during
    // component construction for the same reason. Anything that changes after
    // this point, including the state applied below, animates normally.
    // `enter` is where the node's first frame starts from. Its values go in
    // before the behaviors, so they land without animating; the declared
    // values go back in after, so the behaviors carry the node from the one
    // to the other. A property with no behavior simply arrives.
    let (entering, entrance) = match named_enter {
        Some(enter) => enter_values(state, ctx, node, enter)?,
        None => (Vec::new(), None),
    };
    if let Some(behavior) = named_behavior {
        configure_behaviors(state, ctx, node, behavior)?;
    }
    for (property, settled) in entering {
        match entrance {
            // Timed by the entrance itself, whatever behaviors say.
            Some(timing) => {
                let mut state = state.borrow_mut();
                let start = state
                    .scene
                    .current(node, &property)
                    .map_err(|error| error.to_string())?
                    .clone();
                state
                    .scene
                    .animate_from(node, &property, start, settled, timing)
                    .map_err(|error| error.to_string())?;
            }
            None => assign_scene_property(&mut state.borrow_mut(), node, &property, settled)?,
        }
    }
    if let Some(exit) = named_exit {
        configure_exit(state, ctx, node, exit)?;
    }
    // After the behaviors and the entrance, so a loop starts from where the
    // node was declared to be rather than from a schema default.
    if let Some(value) = named_loop {
        match value {
            LuaValue::Function(Function::Closure(closure)) => {
                crate::reactive_bindings::register_loop_binding(state, ctx, limits, node, closure);
            }
            value => {
                let value = lua_to_scene(ctx, value, 0)?;
                crate::node_loops::apply_loops(&mut state.borrow_mut(), node, &value)?;
            }
        }
    }
    children.sort_by_key(|(index, _)| *index);
    for (_, child) in children {
        state
            .borrow_mut()
            .scene
            .reparent(child, Some(node))
            .map_err(|error| error.to_string())?;
    }
    if let Some(selector) = state_selector {
        if state_value.is_some() {
            return Err("states with `when` choose themselves; drop `state`".into());
        }
        register_state_binding(state, ctx, limits, node, selector);
    }
    if let Some(value) = state_value {
        match value {
            LuaValue::Function(Function::Closure(closure)) => {
                register_state_binding(state, ctx, limits, node, closure);
            }
            LuaValue::String(name) => {
                let mut remaining = limits.frame_fuel;
                apply_state(
                    state,
                    ctx,
                    limits,
                    &mut remaining,
                    node,
                    &name.display_lossy().to_string(),
                )?;
            }
            _ => return Err("state must be a string or binding function".into()),
        }
    }
    Ok(())
}

/// `stretch` on any node and `track` on a field layer: settings the engine
/// keeps beside the node rather than as properties, because one names another
/// node and the other is a spring the engine runs, and almost no node has
/// either. Written at construction or later through the node, the same way.
pub(crate) fn assign_engine_relation<'gc>(
    state: &mut ReactiveState,
    ctx: Context<'gc>,
    node: NodeHandle,
    property: &str,
    value: LuaValue<'gc>,
) -> Result<(), String> {
    match property {
        "shortcuts" => match crate::shortcut::read_table(ctx, value)? {
            Some(shortcuts) => {
                state.shortcuts.insert(node, shortcuts);
            }
            None => {
                state.shortcuts.remove(&node);
            }
        },
        "stretch" => {
            if matches!(value, LuaValue::Function(_)) {
                return Err("stretch is a setting, not a binding: give it a table or true".into());
            }
            let spec = morf_scene::Stretch::from_value(&lua_to_scene(ctx, value, 0)?)?;
            state
                .scene
                .set_stretch(node, spec)
                .map_err(|error| error.to_string())?;
        }
        // A node is kept as the mask (and moved under this one); a table is
        // a gradient, which replaces it; nothing takes either away.
        "mask" => match value {
            LuaValue::UserData(userdata) => {
                let mask = userdata
                    .downcast_static::<NodeToken>()
                    .map_err(|_| "mask must be a morf node, a table or nil".to_owned())?
                    .handle;
                state
                    .scene
                    .set_mask(node, Some(mask))
                    .map_err(|error| error.to_string())?;
            }
            LuaValue::Nil | LuaValue::Boolean(false) | LuaValue::Table(_) => {
                state
                    .scene
                    .set_mask(node, None)
                    .map_err(|error| error.to_string())?;
                let value = match value {
                    LuaValue::Table(_) => lua_to_scene(ctx, value, 0)?,
                    _ => morf_scene::Value::Nil,
                };
                crate::scene_bindings::assign_scene_property(state, node, "mask", value)?;
            }
            _ => return Err("mask must be a morf node, a table or nil".to_owned()),
        },
        _ => {
            let element = state
                .scene
                .element(node)
                .map_err(|error| error.to_string())?;
            if element != morf_scene::Element::SdfShape {
                return Err(format!("unknown {element:?} property `track`"));
            }
            let target = match value {
                LuaValue::Nil | LuaValue::Boolean(false) => None,
                LuaValue::UserData(userdata) => Some(
                    userdata
                        .downcast_static::<NodeToken>()
                        .map_err(|_| "track must be a morf node or nil".to_owned())?
                        .handle,
                ),
                _ => return Err("track must be a morf node or nil".to_owned()),
            };
            state
                .scene
                .set_track(node, target)
                .map_err(|error| error.to_string())?;
        }
    }
    state.scene_revision = state.scene_revision.wrapping_add(1);
    state.flush_pending = true;
    Ok(())
}

pub(crate) fn handler_event(property: &str) -> Option<UiEvent> {
    EVENT_PROPERTIES
        .iter()
        .find(|(_, name)| *name == property)
        .map(|(event, _)| *event)
}
