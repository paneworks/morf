//! Numeric filtering is native; audio subscription and display stay in Lua.
use crate::{api_geometry::number, scene_bindings::HostError};
use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_audio::spectrum::{self, Filter, Options};
use std::cell::RefCell;
use std::rc::Rc;
fn bands<'gc>(ctx: Context<'gc>, value: Table<'gc>) -> Result<Vec<f64>, HostError> {
    let len = value.length(&ctx);
    if len > 4096 {
        return Err(HostError("too many spectrum bands".into()));
    }
    (1..=len)
        .map(|i| number(value.get_value(ctx, i), "audio band"))
        .collect()
}
fn list<'gc>(ctx: Context<'gc>, values: &[f64]) -> Table<'gc> {
    let out = Table::new(&ctx);
    for (i, v) in values.iter().enumerate() {
        out.set(ctx, i as i64 + 1, *v).expect("numeric list");
    }
    out
}
pub(crate) fn options<'gc>(
    ctx: Context<'gc>,
    table: Option<Table<'gc>>,
) -> Result<Options, HostError> {
    let mut o = Options::default();
    let read = |key, default| match table.map_or(LuaValue::Nil, |t| t.get_value(ctx, key)) {
        LuaValue::Nil => Ok(default),
        v => number(v, key),
    };
    let bars = read("bars", o.bars as f64)?;
    if bars.fract() != 0.0 || !(1.0..=512.0).contains(&bars) {
        return Err(HostError("bars must be 1..512".into()));
    }
    o.bars = bars as usize;
    o.rate_hz = read("rate_hz", o.rate_hz)?;
    o.noise = read("noise", o.noise)?;
    o.smoothing = read("smoothing", o.smoothing)?;
    o.gravity = read("gravity", o.gravity)?;
    o.spread = read("spread", o.spread)?;
    o.attack = read("attack", o.attack)?;
    o.release = read("release", o.release)?;
    o.sensitivity = read("sensitivity", o.sensitivity)?;
    o.auto = match table.map_or(LuaValue::Nil, |t| t.get_value(ctx, "auto")) {
        LuaValue::Nil => o.auto,
        LuaValue::Boolean(b) => b,
        _ => return Err(HostError("auto must be boolean".into())),
    };
    o.validate().map_err(HostError)
}
pub(crate) fn install<'gc>(ctx: Context<'gc>, audio: Table<'gc>) {
    audio.set_field(
        ctx,
        "spectrum_resample",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (values, n): (Table, LuaValue) = stack.consume(ctx)?;
            let n = number(n, "bar count")?;
            if n.fract() != 0.0 || !(1.0..=512.0).contains(&n) {
                return Err(HostError("bars must be 1..512".into()).into());
            }
            let out = spectrum::resample(&bands(ctx, values)?, n as usize).map_err(HostError)?;
            stack.replace(ctx, list(ctx, &out));
            Ok(CallbackReturn::Return)
        }),
    );
    audio.set_field(
        ctx,
        "spectrum_filter",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let o: Option<Table> = stack.consume(ctx)?;
            let filter = Rc::new(RefCell::new(
                Filter::new(options(ctx, o)?).map_err(HostError)?,
            ));
            let result = Table::new(&ctx);
            let step = Rc::clone(&filter);
            result.set_field(
                ctx,
                "step",
                Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    let (values, dt): (Table, LuaValue) = stack.consume(ctx)?;
                    let bands = bands(ctx, values)?;
                    let dt = if matches!(dt, LuaValue::Nil) {
                        None
                    } else {
                        Some(number(dt, "dt")?)
                    };
                    let mut filter = step.borrow_mut();
                    let values = filter.step(&bands, dt).map_err(HostError)?;
                    stack.replace(ctx, list(ctx, values));
                    Ok(CallbackReturn::Return)
                }),
            );
            result.set_field(
                ctx,
                "gain",
                Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    stack.replace(ctx, filter.borrow().gain());
                    Ok(CallbackReturn::Return)
                }),
            );
            stack.replace(ctx, result);
            Ok(CallbackReturn::Return)
        }),
    );
}
