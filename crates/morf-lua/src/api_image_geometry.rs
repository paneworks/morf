use crate::{api_geometry::number, api_image_annotation::annotation, scene_bindings::HostError};
use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
fn tolerance(value: LuaValue<'_>) -> Result<f32, HostError> {
    if matches!(value, LuaValue::Nil) {
        Ok(6.0)
    } else {
        Ok(number(value, "hit tolerance")? as f32)
    }
}
pub(crate) fn install<'gc>(ctx: Context<'gc>, image: Table<'gc>) {
    image.set_field(
        ctx,
        "annotation_bounds",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let a: LuaValue = stack.consume(ctx)?;
            let b = annotation(ctx, a, 0.0, 0.0)?.bounds();
            let out = Table::new(&ctx);
            for (name, value) in [("x", b.x), ("y", b.y), ("w", b.w), ("h", b.h)] {
                out.set_field(ctx, name, f64::from(value));
            }
            stack.replace(ctx, out);
            Ok(CallbackReturn::Return)
        }),
    );
    image.set_field(
        ctx,
        "annotation_hit",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (a, x, y, t): (LuaValue, LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let a = annotation(ctx, a, 0.0, 0.0)?;
            let hit = a.hit(
                number(x, "hit x")? as f32,
                number(y, "hit y")? as f32,
                tolerance(t)?,
            );
            stack.replace(ctx, hit);
            Ok(CallbackReturn::Return)
        }),
    );
    image.set_field(
        ctx,
        "annotation_pick",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (items, x, y, t): (Table, LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let n = items.length(&ctx);
            if n > 128 {
                return Err(HostError("at most 128 annotations can be picked".into()).into());
            }
            let (x, y, t) = (
                number(x, "hit x")? as f32,
                number(y, "hit y")? as f32,
                tolerance(t)?,
            );
            let mut picked = None;
            for i in (1..=n).rev() {
                if annotation(ctx, items.get_value(ctx, i), 0.0, 0.0)?.hit(x, y, t) {
                    picked = Some(i);
                    break;
                }
            }
            stack.replace(ctx, picked);
            Ok(CallbackReturn::Return)
        }),
    );
}
