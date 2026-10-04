//! `morf.gamma`: an output's colour ramps, for a night light.
//!
//! `set { output, temperature, brightness, gamma }` asks for warmer,
//! dimmer or differently curved colour on the named output (or the one this
//! shell is on), `reset(output)` gives it its own ramps back, and
//! `supported()` says whether the compositor offers
//! `wlr-gamma-control-unstable-v1`. The requests are carried out by the
//! shell's connection; the compositor restores every output when the shell
//! exits, however it exits, and a reload resets them too.

use std::cell::RefCell;
use std::rc::Rc;

use luna::{Callback, CallbackReturn, Table, Value as LuaValue};

use crate::scene_bindings::HostError;
use crate::state::ReactiveState;

pub use morf_runtime::requests::GammaRequest;

/// The temperatures a configuration may ask for, in kelvin.
pub const TEMPERATURE_RANGE: (f64, f64) = (1000.0, 25000.0);

fn number(
    value: LuaValue<'_>,
    what: &str,
    default: f64,
    range: (f64, f64),
) -> Result<f64, HostError> {
    let value = match value {
        LuaValue::Nil => return Ok(default),
        LuaValue::Integer(value) => value as f64,
        LuaValue::Number(value) => value,
        _ => return Err(HostError(format!("gamma {what} must be a number"))),
    };
    if !(value.is_finite() && value >= range.0 && value <= range.1) {
        return Err(HostError(format!(
            "gamma {what} must be {}..{}, not {value}",
            range.0, range.1
        )));
    }
    Ok(value)
}

fn output_name(value: LuaValue<'_>) -> Result<Option<String>, HostError> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::String(name) => Ok(Some(name.display_lossy().to_string())),
        _ => Err(HostError("gamma output must be an output's name".into())),
    }
}

fn push(state: &Rc<RefCell<ReactiveState>>, request: GammaRequest) -> Result<(), HostError> {
    state
        .borrow_mut()
        .requests
        .queue_gamma(request)
        .map_err(HostError)
}

pub(crate) fn install_gamma_api<'gc>(
    ctx: luna::Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let gamma = Table::new(&ctx);
    let set_state = Rc::clone(&state);
    gamma.set_field(
        ctx,
        "set",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let options: Table = stack.consume(ctx)?;
            for (key, _) in options.iter(ctx) {
                let known = matches!(key, LuaValue::String(name) if matches!(
                    name.as_bytes(),
                    b"output" | b"temperature" | b"brightness" | b"gamma"
                ));
                if !known {
                    return Err(HostError(
                        "gamma.set takes output, temperature, brightness and gamma".into(),
                    )
                    .into());
                }
            }
            let request = GammaRequest {
                output: output_name(options.get_value(ctx, "output"))?,
                set: Some((
                    number(
                        options.get_value(ctx, "temperature"),
                        "temperature",
                        6500.0,
                        TEMPERATURE_RANGE,
                    )?,
                    number(
                        options.get_value(ctx, "brightness"),
                        "brightness",
                        1.0,
                        (0.0, 1.0),
                    )?,
                    number(options.get_value(ctx, "gamma"), "gamma", 1.0, (0.1, 10.0))?,
                )),
            };
            push(&set_state, request)?;
            Ok(CallbackReturn::Return)
        }),
    );
    let reset_state = Rc::clone(&state);
    gamma.set_field(
        ctx,
        "reset",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let output: LuaValue = stack.consume(ctx)?;
            push(
                &reset_state,
                GammaRequest {
                    output: output_name(output)?,
                    set: None,
                },
            )?;
            Ok(CallbackReturn::Return)
        }),
    );
    let supported_state = Rc::clone(&state);
    gamma.set_field(
        ctx,
        "supported",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let supported = supported_state
                .borrow()
                .capabilities
                .iter()
                .any(|(key, value)| key == "gamma_control" && value == "true");
            stack.replace(ctx, supported);
            Ok(CallbackReturn::Return)
        }),
    );
    morf.set_field(ctx, "gamma", gamma);
}
