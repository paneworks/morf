//! Reading and writing the configuration's values: lists, ids, flat point
//! lists, and the item and port tables.

use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::value::{number, text};

use super::{Item, Port, Shape};

/// Every field a skin reads, by name, for diffing.
pub(super) type Fields = Vec<(String, IpcValue)>;

pub(super) fn list(values: Vec<IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::List(values)))
}

pub(super) fn ids(values: &[String]) -> IpcValue {
    list(values.iter().map(|id| IpcValue::from(id.as_str())).collect())
}

pub(super) fn flat(points: &[[f64; 2]]) -> IpcValue {
    list(points.iter().flat_map(|p| [p[0].into(), p[1].into()]).collect())
}

pub(super) fn entries(value: &IpcValue) -> Vec<IpcValue> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items.clone(),
            IpcTable::Map(map) => map.values().cloned().collect(),
        },
        _ => Vec::new(),
    }
}

fn field<'a>(value: &'a IpcValue, name: &str) -> Option<&'a IpcValue> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::Map(map) => map.get(name),
            IpcTable::List(_) => None,
        },
        _ => None,
    }
}

/// An id as text: a number names an item as well as a string does.
fn id_of(value: Option<&IpcValue>) -> Option<String> {
    match value? {
        IpcValue::String(s) => Some(s.clone()),
        IpcValue::Integer(n) => Some(n.to_string()),
        IpcValue::Number(n) if n.fract() == 0.0 => Some((*n as i64).to_string()),
        IpcValue::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn points_of(value: Option<&IpcValue>) -> Vec<[f64; 2]> {
    let numbers: Vec<f64> = value.map(entries).unwrap_or_default().iter().filter_map(|v| number(Some(v))).collect();
    numbers.chunks_exact(2).map(|p| [p[0], p[1]]).collect()
}

pub(super) fn id_list(value: &IpcValue) -> Vec<String> {
    entries(value).iter().filter_map(|v| id_of(Some(v))).collect()
}

pub(super) fn item_from(value: &IpcValue) -> Option<Item> {
    let id = id_of(field(value, "id"))?;
    let n = |name: &str| number(field(value, name)).unwrap_or(0.0);
    let shape = match text(field(value, "shape")).unwrap_or("rect") {
        "ellipse" | "circle" => Shape::Ellipse([n("x"), n("y"), n("w"), n("h")]),
        "point" => Shape::Point([n("x"), n("y")], number(field(value, "r")).unwrap_or(6.0)),
        "line" => Shape::Line(points_of(field(value, "points")), number(field(value, "width")).unwrap_or(2.0)),
        "polygon" => Shape::Polygon(points_of(field(value, "points"))),
        _ => Shape::Rect([n("x"), n("y"), n("w"), n("h")]),
    };
    let selectable = !matches!(field(value, "selectable"), Some(IpcValue::Boolean(false)));
    Some(Item { id, shape, selectable })
}

pub(super) fn port_from(value: &IpcValue) -> Option<Port> {
    Some(Port {
        id: id_of(field(value, "id"))?,
        item: id_of(field(value, "item")).unwrap_or_default(),
        at: [number(field(value, "x"))?, number(field(value, "y"))?],
        kind: text(field(value, "kind")).unwrap_or("").to_owned(),
    })
}
