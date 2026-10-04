//! `behaviors`: how a node's properties move when written, and the option
//! readers a behaviour shares with the rest of the API.

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
                    .animation_callbacks
                    .remove(&(node, property.clone()));
            }
            LuaValue::Function(Function::Closure(callback)) => {
                let callback = ctx.stash(callback);
                state.borrow_mut().animation_callbacks.insert(
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
