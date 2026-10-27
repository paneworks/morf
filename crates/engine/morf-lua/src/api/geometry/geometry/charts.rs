//! `morf.geometry`'s charts and marks: graph grids, arcs and sectors,
//! hatching, ticks and rulers, segments.

use super::*;

/// Installs the chart and mark calls on `api`.
pub(super) fn install_charts<'gc>(ctx: Context<'gc>, api: Table<'gc>) {
    api.set_field(
        ctx,
        "graph_grid",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (w, h, c, r): (LuaValue, LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let path = graph::grid(
                number(w, "width")?,
                number(h, "height")?,
                count(c, 256)?,
                count(r, 256)?,
            )
            .map_err(HostError)?;
            stack.replace(ctx, path);
            Ok(CallbackReturn::Return)
        }),
    );
    // Marks a style draws with (`morf_vector::marks`): path data strings.
    api.set_field(
        ctx,
        "arc",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (cx, cy, r, from, sweep): (LuaValue, LuaValue, LuaValue, LuaValue, LuaValue) =
                stack.consume(ctx)?;
            let d = marks::arc(
                number(cx, "cx")?,
                number(cy, "cy")?,
                number(r, "r")?,
                number(from, "from")?,
                number(sweep, "sweep")?,
            );
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "sector",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // (cx, cy, r0, r1, from, sweep): a ring's slice, a pie's when r0 is 0.
            let (cx, cy, r0, r1, from, sweep): (
                LuaValue,
                LuaValue,
                LuaValue,
                LuaValue,
                LuaValue,
                LuaValue,
            ) = stack.consume(ctx)?;
            let d = marks::sector(
                number(cx, "cx")?,
                number(cy, "cy")?,
                number(r0, "r0")?,
                number(r1, "r1")?,
                number(from, "from")?,
                number(sweep, "sweep")?,
            );
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "hatch",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (w, h, gap): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let gap = if matches!(gap, LuaValue::Nil) {
                6.0
            } else {
                number(gap, "gap")?
            };
            stack.replace(
                ctx,
                marks::hatch(number(w, "width")?, number(h, "height")?, gap),
            );
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "hatch_under",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (x0, dx, ys, w, h, gap): (LuaValue, LuaValue, Table, LuaValue, LuaValue, LuaValue) =
                stack.consume(ctx)?;
            let mut levels = Vec::new();
            for i in 1..=graph::MAX_SAMPLES as i64 {
                match ys.get_value(ctx, i) {
                    LuaValue::Nil => break,
                    v => levels.push(reading(v, "level")?),
                }
            }
            let gap = if matches!(gap, LuaValue::Nil) {
                6.0
            } else {
                number(gap, "gap")?
            };
            let d = marks::hatch_under(
                number(x0, "x0")?,
                number(dx, "dx")?,
                &levels,
                number(w, "width")?,
                number(h, "height")?,
                gap,
            );
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "ticks",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // (cx, cy, r0, r1, { from, sweep, count | angles, major, major_r0 })
            let (cx, cy, r0, r1, o): (LuaValue, LuaValue, LuaValue, LuaValue, Option<Table>) =
                stack.consume(ctx)?;
            let (cx, cy, r0, r1) = (
                number(cx, "cx")?,
                number(cy, "cy")?,
                number(r0, "r0")?,
                number(r1, "r1")?,
            );
            let major = field(ctx, o, "major", 0.0)?.max(0.0) as usize;
            let major_r0 = field(ctx, o, "major_r0", r0)?;
            let angles = o.map_or(LuaValue::Nil, |o| o.get_value(ctx, "angles"));
            let d = if let LuaValue::Table(list) = angles {
                let mut at = Vec::new();
                for i in 1..=4096i64 {
                    match list.get_value(ctx, i) {
                        LuaValue::Nil => break,
                        v => at.push(number(v, "angle")?),
                    }
                }
                marks::radials(cx, cy, r0, r1, &at, major, major_r0)
            } else {
                let count = field(ctx, o, "count", 12.0)?.clamp(1.0, 4096.0) as usize;
                marks::ticks(
                    cx,
                    cy,
                    r0,
                    r1,
                    field(ctx, o, "from", 0.0)?,
                    field(ctx, o, "sweep", 360.0)?,
                    count,
                    major,
                    major_r0,
                )
            };
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "ruler",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // (length, size, { pitch = 8, major = 5, minor = size / 2, min_count = 4, vertical })
            let (length, size, o): (LuaValue, LuaValue, Option<Table>) = stack.consume(ctx)?;
            let (length, size) = (number(length, "length")?, number(size, "size")?);
            let vertical =
                o.is_some_and(|o| matches!(o.get_value(ctx, "vertical"), LuaValue::Boolean(true)));
            let d = marks::ruler(
                length,
                size,
                field(ctx, o, "pitch", 8.0)?,
                field(ctx, o, "major", 5.0)?.max(0.0) as usize,
                field(ctx, o, "minor", size / 2.0)?,
                field(ctx, o, "min_count", 4.0)?.clamp(1.0, 4096.0) as usize,
                vertical,
            );
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "segments",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // (width, height, count, gap = 2, { vertical })
            let (w, h, n, gap, o): (LuaValue, LuaValue, LuaValue, LuaValue, Option<Table>) =
                stack.consume(ctx)?;
            let gap = if matches!(gap, LuaValue::Nil) {
                2.0
            } else {
                number(gap, "gap")?
            };
            let vertical =
                o.is_some_and(|o| matches!(o.get_value(ctx, "vertical"), LuaValue::Boolean(true)));
            let d = marks::segments(
                number(w, "width")?,
                number(h, "height")?,
                count(n, 4096)?,
                gap,
                vertical,
            );
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
}
