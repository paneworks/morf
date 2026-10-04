//! `behaviors`: how a node's properties move when written, and the option
//! readers a behaviour shares with the rest of the API.

use morf_runtime::animation::behaviors;

use super::*;

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
                    .animation
                    .callbacks
                    .remove(&(node, property.clone()));
            }
            LuaValue::Function(Function::Closure(callback)) => {
                let callback = ctx.stash(callback);
                state.borrow_mut().animation.callbacks.insert(
                    (node, property.clone()),
                    crate::vm::handler_store::register(callback),
                );
            }
            _ => return Err("behavior on_finished must be a function".to_owned()),
        }
        let kind = match behavior.get_value(ctx, "kind") {
            LuaValue::Nil => None,
            LuaValue::String(value) => Some(value.display_lossy().to_string()),
            _ => return Err("behavior kind must be a string".to_owned()),
        };
        if kind.as_deref() == Some("spring") {
            let physics = behaviors::spring(
                table_number(ctx, behavior, "mass", 1.0)?,
                table_number(ctx, behavior, "damping", 18.0)?,
                table_number(ctx, behavior, "stiffness", 180.0)?,
                table_number(ctx, behavior, "epsilon", 0.001)?,
            );
            state
                .borrow_mut()
                .scene
                .set_physics(node, &property, Some(physics))
                .map_err(|error| error.to_string())?;
            continue;
        }
        if kind.as_deref() == Some("smoothed") {
            let physics = behaviors::smoothed(table_number(ctx, behavior, "velocity", 1_000.0)?);
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
        let duration = behaviors::duration(duration)?;
        let easing = parse_easing(ctx, behavior.get_value(ctx, "easing"))?;
        let rotation_direction = parse_rotation_direction(ctx, behavior)?;
        let delay = behaviors::delay(table_number(ctx, behavior, "delay", 0.0)?)?;
        let time_scale = behaviors::time_scale(table_number(ctx, behavior, "time_scale", 1.0)?)?;
        state
            .borrow_mut()
            .scene
            .set_behavior(
                node,
                &property,
                Some(Behavior {
                    duration,
                    easing,
                    rotation_direction,
                    delay,
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
    let loops = match options.get_value(ctx, "loops") {
        LuaValue::Nil => behaviors::Loops::Absent,
        LuaValue::Integer(value) => behaviors::Loops::Count(value as f64),
        LuaValue::Number(value) => behaviors::Loops::Count(value),
        LuaValue::String(value) => behaviors::Loops::Name(value.display_lossy().to_string()),
        _ => return Err("behavior loops must be a pass count or a mode name".to_owned()),
    };
    behaviors::repeat(loops, alternating)
}

/// A field that is a name or absent; anything else is `wrong`.
fn optional_name<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
    field: &str,
    wrong: &str,
) -> Result<Option<String>, String> {
    match options.get_value(ctx, field) {
        LuaValue::Nil => Ok(None),
        LuaValue::String(value) => Ok(Some(value.display_lossy().to_string())),
        _ => Err(wrong.to_owned()),
    }
}

/// `retarget = "blend" | "restart"`, true for blend (the default).
pub(crate) fn parse_retarget<'gc>(ctx: Context<'gc>, options: Table<'gc>) -> Result<bool, String> {
    let name = optional_name(
        ctx,
        options,
        "retarget",
        "behavior retarget must be a string",
    )?;
    behaviors::retarget(name.as_deref())
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
    let name = optional_name(ctx, options, "space", "space must be a string")?;
    behaviors::color_space(name.as_deref())
}

/// `hue = "shorter" | "longer"`: which way round the wheel in `oklch`.
pub(crate) fn parse_hue<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
) -> Result<morf_scene::HueDirection, String> {
    let name = optional_name(ctx, options, "hue", "hue must be a string")?;
    behaviors::hue(name.as_deref())
}

pub(crate) fn parse_rotation_direction<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
) -> Result<RotationDirection, String> {
    let name = optional_name(
        ctx,
        options,
        "rotation_direction",
        "rotation_direction must be a string",
    )?;
    behaviors::rotation_direction(name.as_deref())
}
