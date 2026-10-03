use crate::{api_geometry::number, scene_bindings::HostError};
use luna::{Callback, CallbackReturn, Context, Table, Value};
use morf_audio::equalizer::{self, Options};
fn array<'gc>(ctx: Context<'gc>, t: Table<'gc>, key: &str) -> Result<[f64; 8], HostError> {
    match t.get_value(ctx, key) {
        Value::Nil => Ok([0.; 8]),
        Value::Table(v) if v.length(&ctx) == 8 => {
            let mut out = [0.; 8];
            for (i, item) in out.iter_mut().enumerate() {
                *item = number(v.get_value(ctx, i as i64 + 1), key)?;
            }
            Ok(out)
        }
        _ => Err(HostError(format!("{key} must contain eight numbers"))),
    }
}
fn list<'gc>(ctx: Context<'gc>, values: &[f64]) -> Table<'gc> {
    let t = Table::new(&ctx);
    for (i, v) in values.iter().enumerate() {
        t.set(ctx, i as i64 + 1, *v).expect("EQ list");
    }
    t
}
pub(crate) fn install<'gc>(ctx: Context<'gc>, audio: Table<'gc>) {
    audio.set_field(
        ctx,
        "equalizer_curve",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let t: Table = stack.consume(ctx)?;
            let boolean = |key, default| match t.get_value(ctx, key) {
                Value::Nil => Ok(default),
                Value::Boolean(v) => Ok(v),
                _ => Err(HostError(format!("{key} must be boolean"))),
            };
            let numeric = |key, default| match t.get_value(ctx, key) {
                Value::Nil => Ok(default),
                v => number(v, key),
            };
            let o = Options {
                left: array(ctx, t, "left")?,
                right: array(ctx, t, "right")?,
                bands: array(ctx, t, "bands")?,
                strength: numeric("strength", 30.)?,
                trim: numeric("trim", 0.)?,
                per_ear: boolean("per_ear", true)?,
                compensation: boolean("compensation", false)?,
                enabled: boolean("enabled", true)?,
            };
            let c = equalizer::curve(&o).map_err(HostError)?;
            let out = Table::new(&ctx);
            out.set_field(ctx, "left", list(ctx, &c.left));
            out.set_field(ctx, "right", list(ctx, &c.right));
            out.set_field(ctx, "frequencies", list(ctx, &equalizer::FREQUENCIES));
            out.set_field(ctx, "preamp", c.preamp);
            out.set_field(ctx, "response_left", list(ctx, &c.response_left));
            out.set_field(ctx, "response_right", list(ctx, &c.response_right));
            stack.replace(ctx, out);
            Ok(CallbackReturn::Return)
        }),
    );
}
