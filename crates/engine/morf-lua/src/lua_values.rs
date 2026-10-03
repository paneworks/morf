use luna::{Context, Table, Value as LuaValue};
use morf_desktop::DesktopEntry;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use morf_scene::Easing;
use morf_services::{AuthMessageType, GreetdResponse};

use crate::reactive_bindings::lua_to_scene;
use crate::state::*;

pub(crate) fn string_table<'gc>(
    ctx: Context<'gc>,
    values: impl IntoIterator<Item = String>,
) -> Table<'gc> {
    let table = Table::new(&ctx);
    for (index, value) in values.into_iter().enumerate() {
        table
            .set(ctx, index as i64 + 1, value)
            .expect("string table accepts integer keys");
    }
    table
}

pub(crate) fn desktop_entry_table<'gc>(ctx: Context<'gc>, entry: &DesktopEntry) -> Table<'gc> {
    let value = Table::new(&ctx);
    value.set_field(ctx, "id", entry.id.as_str());
    value.set_field(ctx, "name", entry.name.as_str());
    value.set_field(ctx, "generic_name", entry.generic_name.as_str());
    value.set_field(ctx, "startup_class", entry.startup_class.as_str());
    value.set_field(ctx, "no_display", entry.no_display);
    value.set_field(ctx, "try_exec", entry.try_exec.as_str());
    value.set_field(
        ctx,
        "desktop_names",
        string_table(ctx, entry.desktop_names.clone()),
    );
    value.set_field(ctx, "source", entry.source.as_str());
    value.set_field(ctx, "comment", entry.comment.as_str());
    value.set_field(ctx, "icon", entry.icon.as_str());
    value.set_field(ctx, "exec", entry.exec.as_str());
    value.set_field(ctx, "command", string_table(ctx, entry.command.clone()));
    value.set_field(ctx, "working_directory", entry.working_directory.as_str());
    value.set_field(ctx, "run_in_terminal", entry.run_in_terminal);
    value.set_field(
        ctx,
        "categories",
        string_table(ctx, entry.categories.clone()),
    );
    value.set_field(ctx, "keywords", string_table(ctx, entry.keywords.clone()));
    let actions = Table::new(&ctx);
    for (index, action) in entry.actions.iter().enumerate() {
        let item = Table::new(&ctx);
        item.set_field(ctx, "id", action.id.as_str());
        item.set_field(ctx, "name", action.name.as_str());
        item.set_field(ctx, "icon", action.icon.as_str());
        item.set_field(ctx, "exec", action.exec.as_str());
        item.set_field(ctx, "command", string_table(ctx, action.command.clone()));
        actions
            .set(ctx, index as i64 + 1, item)
            .expect("desktop action table accepts integer keys");
    }
    value.set_field(ctx, "actions", actions);
    value
}

pub(crate) fn greetd_response<'gc>(ctx: Context<'gc>, response: GreetdResponse) -> Table<'gc> {
    let value = Table::new(&ctx);
    match response {
        GreetdResponse::Success => {
            value.set_field(ctx, "type", "success");
        }
        GreetdResponse::AuthMessage { kind, message } => {
            value.set_field(ctx, "type", "auth_message");
            value.set_field(
                ctx,
                "auth_message_type",
                match kind {
                    AuthMessageType::Visible => "visible",
                    AuthMessageType::Secret => "secret",
                    AuthMessageType::Info => "info",
                    AuthMessageType::Error => "error",
                },
            );
            value.set_field(ctx, "auth_message", message.as_str());
        }
        GreetdResponse::Error {
            authentication,
            description,
        } => {
            value.set_field(ctx, "type", "error");
            value.set_field(ctx, "authentication", authentication);
            value.set_field(ctx, "description", description.as_str());
        }
    }
    value
}

pub(crate) fn track_clock_dependency(
    state: &Rc<RefCell<ReactiveState>>,
    enabled: bool,
    precision: crate::ClockPrecision,
) {
    if !enabled {
        return;
    }
    let mut state = state.borrow_mut();
    let clock = state.clock_signal(precision);
    if let Some(active) = &mut state.active {
        active.reads.insert(clock);
    }
}

pub(crate) fn local_time_table<'gc>(ctx: Context<'gc>) -> Table<'gc> {
    let now = jiff::Zoned::now();
    let numeric = now.strftime("%Y\t%m\t%d\t%H\t%M\t%S\t%u").to_string();
    let parts = numeric
        .split('\t')
        .map(|value| value.parse::<i64>().unwrap_or(0))
        .collect::<Vec<_>>();
    let value = Table::new(&ctx);
    for (field, index) in [
        ("year", 0),
        ("month", 1),
        ("day", 2),
        ("hours", 3),
        ("minutes", 4),
        ("seconds", 5),
        ("weekday", 6),
    ] {
        value.set_field(ctx, field, parts.get(index).copied().unwrap_or(0));
    }
    value.set_field(ctx, "date", now.strftime("%F").to_string());
    value.set_field(ctx, "time", now.strftime("%T").to_string());
    value.set_field(ctx, "month_name", now.strftime("%B").to_string());
    value.set_field(ctx, "weekday_name", now.strftime("%A").to_string());
    value.set_field(ctx, "timezone", now.strftime("%Z").to_string());
    value
}

pub(crate) fn bounded_timeout(milliseconds: i64) -> Result<Duration, String> {
    u64::try_from(milliseconds)
        .ok()
        .filter(|milliseconds| *milliseconds <= 5_000)
        .map(Duration::from_millis)
        .ok_or_else(|| "timeout must be between 0 and 5000 milliseconds".to_owned())
}

pub(crate) fn parse_easing<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Easing, String> {
    match value {
        LuaValue::Nil => Ok(Easing::Linear),
        LuaValue::String(value) => easing_named(&value.display_lossy().to_string()),
        // A spline is read through the scene value it becomes, so a curve
        // declared here and one a binding or a theme token hands back are
        // checked by the same code.
        LuaValue::Table(table) if !matches!(table.get_value(ctx, "spline"), LuaValue::Nil) => {
            easing_from_scene(&lua_to_scene(ctx, value, 0)?)
        }
        // A theme token written as `{ spline = { ... } }`: a state keeps a
        // nested table as a state of its own and the list in it as a model,
        // so the points are read out of that model.
        LuaValue::UserData(userdata) => {
            let token = userdata
                .downcast_static::<crate::state::StateToken>()
                .map_err(
                    |_| "easing must be a string, a cubic Bezier table or { spline = { ... } }",
                )?;
            let fields = token.fields.borrow();
            let Some((_, model)) = fields.lists.get("spline") else {
                return Err("an easing token must hold a `spline` list".to_owned());
            };
            let model = model.borrow();
            let points = (0..model.len())
                .map(|index| match model.get(index) {
                    Some((_, morf_scene::Value::Number(value))) => Ok(*value),
                    _ => Err("easing spline must be a list of numbers".to_owned()),
                })
                .collect::<Result<Vec<f64>, String>>()?;
            Easing::spline(&points)
        }
        LuaValue::Table(value) => {
            // Named (`{ x1 = .., y1 = .. }`) or in order (`{ 0.05, 0.7, 0.1, 1 }`),
            // as motion specs list them.
            let positional = !matches!(value.get_value(ctx, 1), LuaValue::Nil);
            let read = |field: &str, index: i64| {
                let found = if positional {
                    value.get_value(ctx, index)
                } else {
                    value.get_value(ctx, field)
                };
                match found {
                    LuaValue::Integer(value) => Ok(value as f64),
                    LuaValue::Number(value) if value.is_finite() => Ok(value),
                    _ => Err(format!("easing {field} must be a finite number")),
                }
            };
            cubic_bezier(
                read("x1", 1)?,
                read("y1", 2)?,
                read("x2", 3)?,
                read("y2", 4)?,
            )
        }
        _ => {
            Err("easing must be a string, a cubic Bezier table or { spline = { ... } }".to_owned())
        }
    }
}

/// A cubic Bezier timing curve; its x controls must stay in 0..1 so time
/// runs one way.
fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Easing, String> {
    if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
        return Err("easing x1 and x2 must be between 0 and 1".into());
    }
    Ok(Easing::CubicBezier { x1, y1, x2, y2 })
}

/// A timing curve by the name a behavior's `easing` takes.
pub(crate) fn easing_named(name: &str) -> Result<Easing, String> {
    match name {
        "linear" => Ok(Easing::Linear),
        "in_quad" => Ok(Easing::InQuad),
        "out_quad" => Ok(Easing::OutQuad),
        "in_out_quad" => Ok(Easing::InOutQuad),
        "in_cubic" => Ok(Easing::InCubic),
        "out_cubic" => Ok(Easing::OutCubic),
        "in_out_cubic" => Ok(Easing::InOutCubic),
        "in_quart" => Ok(Easing::InQuart),
        "out_quart" => Ok(Easing::OutQuart),
        "in_out_quart" => Ok(Easing::InOutQuart),
        "in_quint" => Ok(Easing::InQuint),
        "out_quint" => Ok(Easing::OutQuint),
        "in_out_quint" => Ok(Easing::InOutQuint),
        "in_sine" => Ok(Easing::InSine),
        "out_sine" => Ok(Easing::OutSine),
        "in_out_sine" => Ok(Easing::InOutSine),
        "in_expo" => Ok(Easing::InExpo),
        "out_expo" => Ok(Easing::OutExpo),
        "in_out_expo" => Ok(Easing::InOutExpo),
        "in_circ" => Ok(Easing::InCirc),
        "out_circ" => Ok(Easing::OutCirc),
        "in_out_circ" => Ok(Easing::InOutCirc),
        "in_back" => Ok(Easing::InBack),
        "out_back" => Ok(Easing::OutBack),
        "in_out_back" => Ok(Easing::InOutBack),
        "in_bounce" => Ok(Easing::InBounce),
        "out_bounce" => Ok(Easing::OutBounce),
        "in_out_bounce" => Ok(Easing::InOutBounce),
        name => Err(format!("unknown easing `{name}`")),
    }
}

/// A timing curve from a value a binding returned: a name, or a cubic Bezier
/// as `{ x1 = .., y1 = .., x2 = .., y2 = .. }` or four numbers in that order.
pub(crate) fn easing_from_scene(value: &morf_scene::Value) -> Result<Easing, String> {
    use morf_scene::Value;
    match value {
        Value::Nil => Ok(Easing::Linear),
        Value::String(name) => easing_named(name),
        Value::Map(fields) if fields.contains_key("spline") => {
            let Some(Value::List(points)) = fields.get("spline") else {
                return Err("easing spline must be a list of numbers".to_owned());
            };
            let points = points
                .iter()
                .map(|point| match point {
                    Value::Number(value) => Ok(*value),
                    _ => Err("easing spline must be a list of numbers".to_owned()),
                })
                .collect::<Result<Vec<f64>, String>>()?;
            Easing::spline(&points)
        }
        Value::Map(fields) => {
            let read = |field: &str| match fields.get(field) {
                Some(Value::Number(value)) if value.is_finite() => Ok(*value),
                _ => Err(format!("easing {field} must be a finite number")),
            };
            cubic_bezier(read("x1")?, read("y1")?, read("x2")?, read("y2")?)
        }
        Value::List(points) => {
            let read = |index: usize, field: &str| match points.get(index) {
                Some(Value::Number(value)) if value.is_finite() => Ok(*value),
                _ => Err(format!("easing {field} must be a finite number")),
            };
            if points.len() != 4 {
                return Err("a cubic Bezier easing is four numbers: x1, y1, x2, y2".to_owned());
            }
            cubic_bezier(
                read(0, "x1")?,
                read(1, "y1")?,
                read(2, "x2")?,
                read(3, "y2")?,
            )
        }
        _ => {
            Err("easing must be a string, a cubic Bezier table or { spline = { ... } }".to_owned())
        }
    }
}
