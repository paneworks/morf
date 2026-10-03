//! The words a `Path` is drawn with: how its stroke ends and turns, which
//! side of a crossing is inside, and which part of path space fills the node.

use crate::types::Value;

/// How an open stroke ends.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum StrokeCap {
    #[default]
    Butt,
    Round,
    Square,
}

impl StrokeCap {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "butt" => Some(Self::Butt),
            "round" => Some(Self::Round),
            "square" => Some(Self::Square),
            _ => None,
        }
    }
}

/// How a stroke turns a corner.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

impl StrokeJoin {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "miter" => Some(Self::Miter),
            "round" => Some(Self::Round),
            "bevel" => Some(Self::Bevel),
            _ => None,
        }
    }
}

/// Which regions of a self-crossing outline are inside it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl FillRule {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "nonzero" | "non_zero" => Some(Self::NonZero),
            "evenodd" | "even_odd" => Some(Self::EvenOdd),
            _ => None,
        }
    }
}

/// The rectangle of path space that is stretched over the node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathViewBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PathViewBox {
    /// Reads `{ x, y, w, h }`, `{ x, y, width, height }` or four numbers.
    /// Nothing, or an empty table, is no view box: path units are pixels.
    pub fn parse(value: &Value) -> Result<Option<Self>, String> {
        let number = |value: Option<&Value>, name: &str| match value {
            None => Ok(0.0),
            Some(Value::Number(number)) if number.is_finite() => Ok(*number),
            Some(_) => Err(format!("view_box `{name}` must be a finite number")),
        };
        let (x, y, width, height) = match value {
            Value::Nil => return Ok(None),
            Value::Map(fields) if fields.is_empty() => return Ok(None),
            Value::List(items) if items.is_empty() => return Ok(None),
            Value::List(items) if items.len() == 4 => (
                number(items.first(), "x")?,
                number(items.get(1), "y")?,
                number(items.get(2), "w")?,
                number(items.get(3), "h")?,
            ),
            Value::Map(fields) => {
                if let Some(key) = fields
                    .keys()
                    .find(|key| !matches!(key.as_str(), "x" | "y" | "w" | "h" | "width" | "height"))
                {
                    return Err(format!("view_box has no field `{key}`"));
                }
                (
                    number(fields.get("x"), "x")?,
                    number(fields.get("y"), "y")?,
                    number(fields.get("w").or(fields.get("width")), "w")?,
                    number(fields.get("h").or(fields.get("height")), "h")?,
                )
            }
            _ => return Err("view_box is { x, y, w, h } or four numbers".to_owned()),
        };
        if width <= 0.0 || height <= 0.0 {
            return Err("view_box needs a positive width and height".to_owned());
        }
        Ok(Some(Self {
            x,
            y,
            width,
            height,
        }))
    }
}

/// Reads a dash list: lengths, dash then gap, none negative and not all zero.
pub fn path_dash(value: &Value) -> Result<Vec<f64>, String> {
    let items = match value {
        Value::Nil => return Ok(Vec::new()),
        Value::Map(fields) if fields.is_empty() => return Ok(Vec::new()),
        Value::List(items) => items,
        _ => return Err("dash is a list of lengths".to_owned()),
    };
    let mut lengths = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::Number(length) if length.is_finite() && *length >= 0.0 => lengths.push(*length),
            _ => return Err("dash lengths are non-negative numbers".to_owned()),
        }
    }
    Ok(lengths)
}
