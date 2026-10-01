//! Typed annotation input, shared by image workers and live Path nodes.
use crate::scene_bindings::HostError;
use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_image::annotation::{Annotation, Kind, Point};

fn number(value: LuaValue<'_>, field: &str) -> Result<f32, HostError> {
    let n = match value {
        LuaValue::Integer(n) => n as f64,
        LuaValue::Number(n) => n,
        _ => return Err(HostError(format!("annotation {field} must be a number"))),
    };
    if !n.is_finite() || n.abs() > 1_000_000.0 {
        return Err(HostError(format!("annotation {field} is out of range")));
    }
    Ok(n as f32)
}
fn string(value: LuaValue<'_>, default: &str, max: usize) -> Result<String, HostError> {
    match value {
        LuaValue::Nil => Ok(default.into()),
        LuaValue::String(s) if s.as_bytes().len() <= max => Ok(s.display_lossy().to_string()),
        _ => Err(HostError(format!(
            "annotation string exceeds {max} bytes or is not a string"
        ))),
    }
}
pub(crate) fn annotation<'gc>(
    ctx: Context<'gc>,
    value: LuaValue<'gc>,
    dx: f32,
    dy: f32,
) -> Result<Annotation, HostError> {
    let LuaValue::Table(a) = value else {
        return Err(HostError("annotation must be a table".into()));
    };
    let kind = match string(a.get_value(ctx, "type"), "", 16)?.as_str() {
        "rect" => Kind::Rect,
        "ellipse" => Kind::Ellipse,
        "line" => Kind::Line,
        "arrow" => Kind::Arrow,
        "pen" => Kind::Pen,
        "marker" => Kind::Marker,
        "text" => Kind::Text,
        "step" => Kind::Step,
        "zoom" => Kind::Zoom,
        "blur" => Kind::Blur,
        "pixelate" => Kind::Pixelate,
        _ => return Err(HostError("unknown vector annotation tool".into())),
    };
    let color = string(a.get_value(ctx, "color"), "#ef5350", 7)?;
    let rgb = color
        .strip_prefix('#')
        .filter(|s| s.len() == 6)
        .and_then(|s| u32::from_str_radix(s, 16).ok())
        .ok_or_else(|| HostError("annotation colour must be #RRGGBB".into()))?;
    let width = number(a.get_value(ctx, "width"), "width")?;
    if !(1.0..=128.0).contains(&width) {
        return Err(HostError("annotation width must be 1..128".into()));
    }
    let LuaValue::Table(points) = a.get_value(ctx, "points") else {
        return Err(HostError("annotation points must be a list".into()));
    };
    let length = points.length(&ctx);
    if !(2..=4096).contains(&length) {
        return Err(HostError("annotation needs 2..4096 points".into()));
    }
    let mut coords = Vec::with_capacity(length as usize);
    for i in 1..=length {
        let LuaValue::Table(p) = points.get_value(ctx, i) else {
            return Err(HostError("annotation point must be {x,y}".into()));
        };
        coords.push(Point {
            x: number(p.get_value(ctx, "x"), "x")? - dx,
            y: number(p.get_value(ctx, "y"), "y")? - dy,
        });
    }
    let n = a.get_value(ctx, "number");
    let number = if matches!(n, LuaValue::Nil) {
        1
    } else {
        crate::api_image_ops::uint(n, "step number")?
    };
    Ok(Annotation {
        kind,
        points: coords,
        color: [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8],
        width,
        filled: matches!(a.get_value(ctx, "filled"), LuaValue::Boolean(true)),
        text: string(a.get_value(ctx, "text"), "", 4096)?,
        font: string(a.get_value(ctx, "font"), "sans-serif", 256)?,
        number,
    })
}
pub(crate) fn parse<'gc>(
    ctx: Context<'gc>,
    list: LuaValue<'gc>,
    dx: LuaValue<'gc>,
    dy: LuaValue<'gc>,
) -> Result<Vec<Annotation>, HostError> {
    let LuaValue::Table(list) = list else {
        return Err(HostError("annotations must be a list".into()));
    };
    let dx = if matches!(dx, LuaValue::Nil) {
        0.0
    } else {
        number(dx, "offset x")?
    };
    let dy = if matches!(dy, LuaValue::Nil) {
        0.0
    } else {
        number(dy, "offset y")?
    };
    let len = list.length(&ctx);
    if len > 129 {
        return Err(HostError(
            "at most 129 annotations including the draft".into(),
        ));
    }
    (1..=len)
        .map(|i| annotation(ctx, list.get_value(ctx, i), dx, dy))
        .collect()
}
pub(crate) fn install<'gc>(ctx: Context<'gc>, image: Table<'gc>) {
    image.set_field(
        ctx,
        "annotation_path",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let value: LuaValue = stack.consume(ctx)?;
            let a = annotation(ctx, value, 0.0, 0.0)?;
            let (stroke, fill) = a.path_data();
            let result = Table::new(&ctx);
            result.set_field(ctx, "stroke", stroke.as_str());
            result.set_field(ctx, "fill", fill.as_str());
            stack.replace(ctx, result);
            Ok(CallbackReturn::Return)
        }),
    );
}
