//! `enter` and `exit`: the values a node arrives from and leaves to, and
//! their timing.

use super::*;

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
pub(super) fn configure_exit<'gc>(
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

pub(super) fn enter_values<'gc>(
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
