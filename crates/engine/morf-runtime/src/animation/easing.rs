//! Timing curves by name, or as a cubic Bezier or a spline given as a scene
//! value: what a behaviour's, a loop's and a theme token's `easing` say.

use morf_scene::Easing;

/// A cubic Bezier timing curve; its x controls must stay in 0..1 so time
/// runs one way.
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Easing, String> {
    if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
        return Err("easing x1 and x2 must be between 0 and 1".into());
    }
    Ok(Easing::CubicBezier { x1, y1, x2, y2 })
}

/// A timing curve by the name a behavior's `easing` takes.
pub fn easing_named(name: &str) -> Result<Easing, String> {
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
pub fn easing_from_scene(value: &morf_scene::Value) -> Result<Easing, String> {
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
