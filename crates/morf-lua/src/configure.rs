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
                .insert(node, ctx.stash(closure));
            continue;
        }
        if matches!(property.as_str(), "stretch" | "track")
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
            state
                .borrow_mut()
                .handlers
                .insert((node, event), ctx.stash(closure));
            continue;
        }
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

/// Puts the `enter` values in place and hands back what each property is
/// meant to settle at.
/// The words in an `enter` or `exit` table that say how, not what.
const TIMING_KEYS: [&str; 3] = ["duration", "easing", "delay"];

/// An `enter` or `exit` table's own timing, when it gives a `duration`:
/// milliseconds, an easing, and a delay.
fn entrance_timing<'gc>(
    ctx: Context<'gc>,
    table: Table<'gc>,
    what: &str,
    default_duration: Option<f64>,
) -> Result<Option<Behavior>, String> {
    let duration = match table.get_value(ctx, "duration") {
        LuaValue::Nil => match default_duration {
            Some(duration) => duration,
            None => return Ok(None),
        },
        LuaValue::Integer(value) => value as f64,
        LuaValue::Number(value) if value.is_finite() => value,
        _ => return Err(format!("{what} duration must be milliseconds")),
    };
    let delay = table_number(ctx, table, "delay", 0.0)?;
    if duration < 0.0 || delay < 0.0 {
        return Err(format!("{what} duration and delay cannot be negative"));
    }
    Ok(Some(Behavior {
        duration: Duration::from_secs_f64(duration / 1_000.0),
        delay: Duration::from_secs_f64(delay / 1_000.0),
        easing: parse_easing(ctx, table.get_value(ctx, "easing"))?,
        ..Behavior::default()
    }))
}

/// `exit = { opacity = 0, scale = 0.9, duration = 200, easing = "in_cubic" }`:
/// how the node leaves when whatever holds it lets it go.
fn configure_exit<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    node: NodeHandle,
    value: LuaValue<'gc>,
) -> Result<(), String> {
    let table = match value {
        LuaValue::Nil | LuaValue::Boolean(false) => {
            return state
                .borrow_mut()
                .scene
                .set_exit(node, None)
                .map_err(|error| error.to_string());
        }
        LuaValue::Table(table) => table,
        _ => return Err("exit must be a property-keyed table".to_owned()),
    };
    let behavior = entrance_timing(ctx, table, "exit", Some(200.0))?.expect("a default duration");
    let mut values = Vec::new();
    for (property, to) in table.iter(ctx) {
        let LuaValue::String(property) = property else {
            return Err("exit keys must be property names".to_owned());
        };
        let property = property.display_lossy().to_string();
        if TIMING_KEYS.contains(&property.as_str()) {
            continue;
        }
        values.push((property, lua_to_scene(ctx, to, 0)?));
    }
    values.sort_by(|a, b| a.0.cmp(&b.0));
    state
        .borrow_mut()
        .scene
        .set_exit(node, Some(morf_scene::ExitSpec { values, behavior }))
        .map_err(|error| format!("exit: {error}"))
}

type Entrance = (Vec<(String, morf_scene::Value)>, Option<Behavior>);

fn enter_values<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    node: NodeHandle,
    value: LuaValue<'gc>,
) -> Result<Entrance, String> {
    let LuaValue::Table(table) = value else {
        return Err("enter must be a property-keyed table".to_owned());
    };
    let timing = entrance_timing(ctx, table, "enter", None)?;
    let mut entering = Vec::new();
    for (property, start) in table.iter(ctx) {
        let LuaValue::String(property) = property else {
            return Err("enter keys must be property names".to_owned());
        };
        let property = property.display_lossy().to_string();
        if TIMING_KEYS.contains(&property.as_str()) {
            if timing.is_none() {
                return Err(format!("enter `{property}` goes with a `duration`"));
            }
            continue;
        }
        let start = lua_to_scene(ctx, start, 0)?;
        let settled = state
            .borrow()
            .scene
            .current(node, &property)
            .map_err(|error| error.to_string())?
            .clone();
        assign_scene_property(&mut state.borrow_mut(), node, &property, start)?;
        entering.push((property, settled));
    }
    Ok((entering, timing))
}

pub(crate) fn configure_behaviors<'gc>(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    node: NodeHandle,
    value: LuaValue<'gc>,
) -> Result<(), String> {
    let LuaValue::Table(behaviors) = value else {
        return Err("behavior must be a property-keyed table".to_owned());
    };
    for (property, behavior) in behaviors.iter(ctx) {
        let LuaValue::String(property) = property else {
            return Err("behavior keys must be property names".to_owned());
        };
        let LuaValue::Table(behavior) = behavior else {
            return Err("each behavior must be a table".to_owned());
        };
        let property = property.display_lossy().to_string();
        // Registered before the kind branches below return early, so spring and
        // smoothed motion report their settling the same way a tween does.
        match behavior.get_value(ctx, "on_finished") {
            LuaValue::Nil => {
                state
                    .borrow_mut()
                    .animation_callbacks
                    .remove(&(node, property.clone()));
            }
            LuaValue::Function(Function::Closure(callback)) => {
                let callback = ctx.stash(callback);
                state
                    .borrow_mut()
                    .animation_callbacks
                    .insert((node, property.clone()), callback);
            }
            _ => return Err("behavior on_finished must be a function".to_owned()),
        }
        let kind = match behavior.get_value(ctx, "kind") {
            LuaValue::Nil => None,
            LuaValue::String(value) => Some(value.display_lossy().to_string()),
            _ => return Err("behavior kind must be a string".to_owned()),
        };
        if kind.as_deref() == Some("spring") {
            let physics = Physics::Spring {
                mass: table_number(ctx, behavior, "mass", 1.0)?,
                damping: table_number(ctx, behavior, "damping", 18.0)?,
                stiffness: table_number(ctx, behavior, "stiffness", 180.0)?,
                epsilon: table_number(ctx, behavior, "epsilon", 0.001)?,
            };
            state
                .borrow_mut()
                .scene
                .set_physics(node, &property, Some(physics))
                .map_err(|error| error.to_string())?;
            continue;
        }
        if kind.as_deref() == Some("smoothed") {
            let physics = Physics::Smoothed {
                velocity: table_number(ctx, behavior, "velocity", 1_000.0)?,
            };
            state
                .borrow_mut()
                .scene
                .set_physics(node, &property, Some(physics))
                .map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(kind) = kind {
            return Err(format!("unknown behavior kind `{kind}`"));
        }
        let duration = match behavior.get_value(ctx, "duration") {
            LuaValue::Integer(value) => value as f64,
            LuaValue::Number(value) if value.is_finite() => value,
            _ => return Err("behavior duration must be milliseconds".to_owned()),
        };
        if duration < 0.0 {
            return Err("behavior duration cannot be negative".to_owned());
        }
        let easing = parse_easing(ctx, behavior.get_value(ctx, "easing"))?;
        let rotation_direction = parse_rotation_direction(ctx, behavior)?;
        let delay = table_number(ctx, behavior, "delay", 0.0)?;
        if delay < 0.0 {
            return Err("behavior delay cannot be negative".to_owned());
        }
        let time_scale = table_number(ctx, behavior, "time_scale", 1.0)?;
        if time_scale <= 0.0 {
            return Err("behavior time_scale must be greater than zero".to_owned());
        }
        state
            .borrow_mut()
            .scene
            .set_behavior(
                node,
                &property,
                Some(Behavior {
                    duration: Duration::from_secs_f64(duration / 1_000.0),
                    easing,
                    rotation_direction,
                    delay: Duration::from_secs_f64(delay / 1_000.0),
                    time_scale,
                    repeat: parse_repeat(ctx, behavior)?,
                    enabled: parse_enabled(ctx, behavior)?,
                    color_space: parse_color_space(ctx, behavior)?,
                    hue: parse_hue(ctx, behavior)?,
                    keep_velocity: parse_retarget(ctx, behavior)?,
                }),
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Reads the `loops` and `ping_pong` pair into a repetition mode.
///
/// `loops` is either a pass count or one of the endless names, and `ping_pong`
/// turns whichever of those was given into an alternating variant. Lua reserves
/// `repeat` as a keyword, so the count field cannot carry that name.
pub(crate) fn parse_repeat<'gc>(ctx: Context<'gc>, options: Table<'gc>) -> Result<Repeat, String> {
    // `alternate` is the word the rest of the vocabulary uses; `ping_pong`
    // stays as its older spelling.
    let mut alternating = false;
    for field in ["ping_pong", "alternate"] {
        match options.get_value(ctx, field) {
            LuaValue::Nil => {}
            LuaValue::Boolean(value) => alternating |= value,
            _ => return Err(format!("behavior {field} must be boolean")),
        }
    }
    let count = |value: f64| -> Result<u32, String> {
        if !value.is_finite() || value < 1.0 {
            return Err("behavior loops must be at least one pass".to_owned());
        }
        Ok(value as u32)
    };
    match options.get_value(ctx, "loops") {
        LuaValue::Nil if alternating => Ok(Repeat::PingPong),
        LuaValue::Nil => Ok(Repeat::Once),
        LuaValue::Integer(value) => Ok(match alternating {
            true => Repeat::PingPongTimes(count(value as f64)?),
            false => Repeat::Times(count(value as f64)?),
        }),
        LuaValue::Number(value) => Ok(match alternating {
            true => Repeat::PingPongTimes(count(value)?),
            false => Repeat::Times(count(value)?),
        }),
        LuaValue::String(value) => match value.display_lossy().to_string().as_str() {
            "once" => Ok(Repeat::Once),
            "forever" => Ok(Repeat::Forever),
            "ping_pong" => Ok(Repeat::PingPong),
            name => Err(format!("unknown behavior loops mode `{name}`")),
        },
        _ => Err("behavior loops must be a pass count or a mode name".to_owned()),
    }
}

/// `retarget = "blend" | "restart"`: whether a write to a moving property
/// carries its speed into the new motion (`"blend"`, the default) or starts
/// the animation over from where the property is (`"restart"`, Qt's
/// Behavior). True for blend.
pub(crate) fn parse_retarget<'gc>(ctx: Context<'gc>, options: Table<'gc>) -> Result<bool, String> {
    match options.get_value(ctx, "retarget") {
        LuaValue::Nil => Ok(true),
        LuaValue::String(value) => match value.display_lossy().to_string().as_str() {
            "blend" => Ok(true),
            "restart" => Ok(false),
            name => Err(format!(
                "behavior retarget must be \"blend\" or \"restart\", not `{name}`"
            )),
        },
        _ => Err("behavior retarget must be a string".to_owned()),
    }
}

/// Reads the optional `enabled` switch, defaulting an absent one to on.
pub(crate) fn parse_enabled<'gc>(ctx: Context<'gc>, options: Table<'gc>) -> Result<bool, String> {
    match options.get_value(ctx, "enabled") {
        LuaValue::Nil => Ok(true),
        LuaValue::Boolean(value) => Ok(value),
        _ => Err("behavior enabled must be boolean".to_owned()),
    }
}

/// `space = "srgb" | "oklab" | "oklch"`: where a colour travels.
pub(crate) fn parse_color_space<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
) -> Result<morf_scene::ColorSpace, String> {
    match options.get_value(ctx, "space") {
        LuaValue::Nil => Ok(morf_scene::ColorSpace::default()),
        LuaValue::String(value) => {
            morf_scene::ColorSpace::parse(&value.display_lossy().to_string())
                .ok_or_else(|| "space must be srgb, oklab or oklch".to_owned())
        }
        _ => Err("space must be a string".to_owned()),
    }
}

/// `hue = "shorter" | "longer"`: which way round the wheel in `oklch`.
pub(crate) fn parse_hue<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
) -> Result<morf_scene::HueDirection, String> {
    match options.get_value(ctx, "hue") {
        LuaValue::Nil => Ok(morf_scene::HueDirection::default()),
        LuaValue::String(value) => {
            morf_scene::HueDirection::parse(&value.display_lossy().to_string())
                .ok_or_else(|| "hue must be shorter or longer".to_owned())
        }
        _ => Err("hue must be a string".to_owned()),
    }
}

pub(crate) fn parse_rotation_direction<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
) -> Result<RotationDirection, String> {
    match options.get_value(ctx, "rotation_direction") {
        LuaValue::Nil => Ok(RotationDirection::Numerical),
        LuaValue::String(value) => match value.display_lossy().to_string().as_str() {
            "numerical" => Ok(RotationDirection::Numerical),
            "shortest" => Ok(RotationDirection::Shortest),
            "clockwise" => Ok(RotationDirection::Clockwise),
            "counterclockwise" => Ok(RotationDirection::CounterClockwise),
            _ => Err(
                "rotation_direction must be numerical, shortest, clockwise, or counterclockwise"
                    .to_owned(),
            ),
        },
        _ => Err("rotation_direction must be a string".to_owned()),
    }
}
