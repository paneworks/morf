//! Native, theme-independent geometry. Lua owns nodes and animation policy.
use crate::scene_bindings::HostError;
use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_outline::{
    geometry::{self, Cubic, Options, SEGMENTS},
    geometry_named, graph, marks, series,
};

pub(crate) fn number(value: LuaValue<'_>, label: &str) -> Result<f64, HostError> {
    let n = match value {
        LuaValue::Integer(n) => n as f64,
        LuaValue::Number(n) => n,
        _ => return Err(HostError(format!("{label} must be a number"))),
    };
    if !n.is_finite() || n.abs() > 1e6 {
        return Err(HostError(format!(
            "{label} must be finite and within ±1000000"
        )));
    }
    Ok(n)
}
/// A reading rather than a coordinate: any finite number. A graph's samples
/// and its `bottom`/`top` are data mapped into the box (a byte rate is
/// millions), so only the box's own size is held to coordinate bounds.
fn reading(value: LuaValue<'_>, label: &str) -> Result<f64, HostError> {
    let n = match value {
        LuaValue::Integer(n) => n as f64,
        LuaValue::Number(n) => n,
        _ => return Err(HostError(format!("{label} must be a number"))),
    };
    if !n.is_finite() {
        return Err(HostError(format!("{label} must be finite")));
    }
    Ok(n)
}
fn reading_field<'gc>(ctx: Context<'gc>, o: Table<'gc>, key: &str, default: f64) -> Result<f64, HostError> {
    match o.get_value(ctx, key) {
        LuaValue::Nil => Ok(default),
        value => reading(value, key),
    }
}
fn field<'gc>(
    ctx: Context<'gc>,
    o: Option<Table<'gc>>,
    key: &str,
    default: f64,
) -> Result<f64, HostError> {
    let value = o.map_or(LuaValue::Nil, |o| o.get_value(ctx, key));
    if matches!(value, LuaValue::Nil) {
        Ok(default)
    } else {
        number(value, key)
    }
}
fn options<'gc>(ctx: Context<'gc>, o: Option<Table<'gc>>) -> Result<Options, HostError> {
    let optional = |name, default| field(ctx, o, name, default);
    Ok(Options {
        rounding: optional("rounding", 0.0)?,
        rotation: optional("rotation", 0.0)?,
        inner_rounding: if o
            .is_some_and(|o| !matches!(o.get_value(ctx, "inner_rounding"), LuaValue::Nil))
        {
            Some(optional("inner_rounding", 0.0)?)
        } else {
            None
        },
        spread: if o.is_some_and(|o| !matches!(o.get_value(ctx, "spread"), LuaValue::Nil)) {
            Some(optional("spread", 0.0)?)
        } else {
            None
        },
    })
}
fn count(value: LuaValue<'_>, max: usize) -> Result<usize, HostError> {
    let n = number(value, "count")?;
    if n.fract() != 0.0 || n < 1.0 || n > max as f64 {
        return Err(HostError(format!("count must be 1..{max}")));
    }
    Ok(n as usize)
}
fn segments(value: LuaValue<'_>) -> Result<Option<usize>, HostError> {
    match value {
        LuaValue::Nil => Ok(Some(SEGMENTS)),
        LuaValue::Boolean(false) => Ok(None),
        n => Ok(Some(count(n, geometry::MAX_CURVES)?)),
    }
}
fn read_curves<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Vec<Cubic>, HostError> {
    let LuaValue::Table(list) = value else {
        return Err(HostError("outline must be a name or cubic list".into()));
    };
    let len = list.length(&ctx);
    if len < 1 || len > geometry::MAX_CURVES as i64 {
        return Err(HostError("invalid number of cubics".into()));
    }
    let mut curves = Vec::with_capacity(len as usize);
    for i in 1..=len {
        let LuaValue::Table(c) = list.get_value(ctx, i) else {
            return Err(HostError("cubic must be eight numbers".into()));
        };
        let mut curve = [0.0; 8];
        for (k, v) in curve.iter_mut().enumerate() {
            *v = number(c.get_value(ctx, k as i64 + 1), "cubic coordinate")?;
        }
        curves.push(curve);
    }
    Ok(curves)
}
fn outline<'gc>(ctx: Context<'gc>, shape: LuaValue<'gc>) -> Result<Vec<Cubic>, HostError> {
    match shape {
        LuaValue::String(name) => {
            geometry_named::outline(&name.display_lossy().to_string()).map_err(HostError)
        }
        _ => read_curves(ctx, shape),
    }
}
fn list<'gc>(ctx: Context<'gc>, curves: &[Cubic]) -> Table<'gc> {
    let out = Table::new(&ctx);
    for (i, c) in curves.iter().enumerate() {
        let row = Table::new(&ctx);
        for (k, v) in c.iter().enumerate() {
            row.set(ctx, k as i64 + 1, *v).expect("numeric table");
        }
        out.set(ctx, i as i64 + 1, row).expect("numeric table");
    }
    out
}
pub(crate) fn install<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
    let api = Table::new(&ctx);
    let names = Table::new(&ctx);
    for (i, name) in geometry_named::names().iter().enumerate() {
        names.set(ctx, i as i64 + 1, *name).expect("name list");
    }
    api.set_field(ctx, "shape_names", names);
    api.set_field(ctx, "shape_segments", SEGMENTS as i64);
    api.set_field(
        ctx,
        "polygon",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (vertices, o): (Table, Option<Table>) = stack.consume(ctx)?;
            let rounding = field(ctx, o, "rounding", 0.0)?;
            let len = vertices.length(&ctx);
            if !(3..=2048).contains(&len) {
                return Err(HostError("polygon needs 3..2048 vertices".into()).into());
            }
            let mut coords = Vec::with_capacity(len as usize);
            for i in 1..=len {
                let LuaValue::Table(v) = vertices.get_value(ctx, i) else {
                    return Err(HostError("vertex must be {x,y,rounding}".into()).into());
                };
                let r = v.get_value(ctx, 3i64);
                coords.push([
                    number(v.get_value(ctx, 1i64), "vertex x")?,
                    number(v.get_value(ctx, 2i64), "vertex y")?,
                    if matches!(r, LuaValue::Nil) {
                        rounding
                    } else {
                        number(r, "rounding")?
                    },
                ]);
            }
            let curves = geometry::polygon(&coords).map_err(HostError)?;
            stack.replace(ctx, list(ctx, &curves));
            Ok(CallbackReturn::Return)
        }),
    );
    for kind in ["star", "regular", "lobes"] {
        api.set_field(
            ctx,
            kind,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let (n, second, third): (LuaValue, LuaValue, Option<Table>) = stack.consume(ctx)?;
                let n = count(n, 2048)?;
                let (inner, o) = if kind == "regular" {
                    (
                        0.0,
                        match second {
                            LuaValue::Nil => None,
                            LuaValue::Table(o) => Some(o),
                            _ => {
                                return Err(
                                    HostError("regular options must be a table".into()).into()
                                );
                            }
                        },
                    )
                } else {
                    (number(second, "inner radius")?, third)
                };
                let o = options(ctx, o)?;
                let curves = match kind {
                    "regular" => geometry::regular(n, o),
                    "star" => geometry::star(n, inner, o),
                    _ => geometry::lobes(n, inner, o),
                }
                .map_err(HostError)?;
                stack.replace(ctx, list(ctx, &curves));
                Ok(CallbackReturn::Return)
            }),
        );
    }
    api.set_field(
        ctx,
        "shape_curves",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (shape, n): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let curves =
                geometry::curves(&outline(ctx, shape)?, segments(n)?).map_err(HostError)?;
            stack.replace(ctx, list(ctx, &curves));
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "shape_path",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (shape, o): (LuaValue, Option<Table>) = stack.consume(ctx)?;
            let size = field(ctx, o, "size", 100.0)?;
            let n = segments(o.map_or(LuaValue::Nil, |o| o.get_value(ctx, "segments")))?;
            let fresh =
                o.is_some_and(|o| matches!(o.get_value(ctx, "fresh"), LuaValue::Boolean(true)));
            let path = if let LuaValue::String(name) = shape
                && !fresh
            {
                geometry_named::path(&name.display_lossy().to_string(), size, n)
                    .map_err(HostError)?
            } else {
                geometry::path(
                    &geometry::curves(&outline(ctx, shape)?, n).map_err(HostError)?,
                    size,
                )
                .map_err(HostError)?
                .into()
            };
            stack.replace(ctx, path.as_ref());
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "graph_series",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (values, o): (Table, Table) = stack.consume(ctx)?;
            let len = values.length(&ctx);
            if len > graph::MAX_SAMPLES as i64 {
                return Err(HostError("graph exceeds 8192 samples".into()).into());
            }
            let samples = count(o.get_value(ctx, "samples"), graph::MAX_SAMPLES)?;
            let read = |key, default| field(ctx, Some(o), key, default);
            let values = (1..=len)
                .map(|i| {
                    let v = values.get_value(ctx, i);
                    if matches!(v, LuaValue::Nil) {
                        Ok(0.0)
                    } else {
                        reading(v, "sample")
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let path = graph::series(
                &values,
                graph::Series {
                    width: read("width", 100.0)?,
                    height: read("height", 100.0)?,
                    bottom: reading_field(ctx, o, "bottom", 0.0)?,
                    top: reading_field(ctx, o, "top", 1.0)?,
                    samples,
                    closed: matches!(o.get_value(ctx, "closed"), LuaValue::Boolean(true)),
                },
            )
            .map_err(HostError)?;
            stack.replace(ctx, path);
            Ok(CallbackReturn::Return)
        }),
    );
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
    // Marks a style draws with (`morf_outline::marks`): path data strings.
    api.set_field(
        ctx,
        "arc",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (cx, cy, r, from, sweep): (LuaValue, LuaValue, LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let d = marks::arc(number(cx, "cx")?, number(cy, "cy")?, number(r, "r")?, number(from, "from")?, number(sweep, "sweep")?);
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "sector",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // (cx, cy, r0, r1, from, sweep): a ring's slice, a pie's when r0 is 0.
            let (cx, cy, r0, r1, from, sweep): (LuaValue, LuaValue, LuaValue, LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let d = marks::sector(number(cx, "cx")?, number(cy, "cy")?, number(r0, "r0")?, number(r1, "r1")?, number(from, "from")?, number(sweep, "sweep")?);
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "hatch",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (w, h, gap): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let gap = if matches!(gap, LuaValue::Nil) { 6.0 } else { number(gap, "gap")? };
            stack.replace(ctx, marks::hatch(number(w, "width")?, number(h, "height")?, gap));
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "hatch_under",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (x0, dx, ys, w, h, gap): (LuaValue, LuaValue, Table, LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let mut levels = Vec::new();
            for i in 1..=graph::MAX_SAMPLES as i64 {
                match ys.get_value(ctx, i) {
                    LuaValue::Nil => break,
                    v => levels.push(reading(v, "level")?),
                }
            }
            let gap = if matches!(gap, LuaValue::Nil) { 6.0 } else { number(gap, "gap")? };
            let d = marks::hatch_under(number(x0, "x0")?, number(dx, "dx")?, &levels, number(w, "width")?, number(h, "height")?, gap);
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "ticks",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // (cx, cy, r0, r1, { from, sweep, count | angles, major, major_r0 })
            let (cx, cy, r0, r1, o): (LuaValue, LuaValue, LuaValue, LuaValue, Option<Table>) = stack.consume(ctx)?;
            let (cx, cy, r0, r1) = (number(cx, "cx")?, number(cy, "cy")?, number(r0, "r0")?, number(r1, "r1")?);
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
                marks::ticks(cx, cy, r0, r1, field(ctx, o, "from", 0.0)?, field(ctx, o, "sweep", 360.0)?, count, major, major_r0)
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
            let vertical = o.is_some_and(|o| matches!(o.get_value(ctx, "vertical"), LuaValue::Boolean(true)));
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
            let (w, h, n, gap, o): (LuaValue, LuaValue, LuaValue, LuaValue, Option<Table>) = stack.consume(ctx)?;
            let gap = if matches!(gap, LuaValue::Nil) { 2.0 } else { number(gap, "gap")? };
            let vertical = o.is_some_and(|o| matches!(o.get_value(ctx, "vertical"), LuaValue::Boolean(true)));
            let d = marks::segments(number(w, "width")?, number(h, "height")?, count(n, 4096)?, gap, vertical);
            stack.replace(ctx, d);
            Ok(CallbackReturn::Return)
        }),
    );
    api.set_field(
        ctx,
        "plot",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // (values, { kind, width, height, samples, bottom, top, ... }): what
            // a Path reading a channel with that `plot` draws.
            let (values, o): (Table, Option<Table>) = stack.consume(ctx)?;
            let mut list = Vec::new();
            for i in 1..=graph::MAX_SAMPLES as i64 {
                match values.get_value(ctx, i) {
                    LuaValue::Nil => break,
                    v => list.push(reading(v, "value")? as f32),
                }
            }
            let get = |key: &str| o.map_or(LuaValue::Nil, |o| o.get_value(ctx, key));
            let number = |key: &str| match get(key) {
                LuaValue::Integer(n) => Some(n as f64),
                LuaValue::Number(n) if n.is_finite() => Some(n),
                _ => None,
            };
            let flag = |key: &str| matches!(get(key), LuaValue::Boolean(true));
            let word = |key: &str| match get(key) {
                LuaValue::String(s) => Some(s.display_lossy().to_string()),
                _ => None,
            };
            let top = match get("top") {
                LuaValue::Nil => None,
                LuaValue::Boolean(false) => Some(None),
                v => Some(Some(reading(v, "top")?)),
            };
            if let Some(kind) = word("kind")
                && series::Kind::parse(&kind).is_none()
            {
                return Err(HostError(format!("unknown plot kind `{kind}`")).into());
            }
            let plot = series::Plot::from_fields(number, flag, word, top, (100.0, 100.0), 0);
            stack.replace(ctx, series::path(&list, &[], &plot));
            Ok(CallbackReturn::Return)
        }),
    );
    morf.set_field(ctx, "geometry", api);
}
