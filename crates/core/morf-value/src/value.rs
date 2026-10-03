//! The value that crosses a boundary: Lua and Rust, an archetype's effects,
//! IPC, a signal's state. Bounded: a table is a deep copy, compared by
//! content, so writing an equal table changes nothing.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::color::Color;

/// Primitive value accepted by the bounded IPC surface.
#[derive(Clone, Debug, PartialEq)]
pub enum IpcValue {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    /// A colour value, so a signal or a state field may hold one.
    Color(Color),
    /// A table a signal holds: a deep copy of a JSON-like Lua table.
    /// Compared by content, so writing an equal table changes nothing.
    Table(Arc<IpcTable>),
}

/// The table inside an `IpcValue::Table`.
#[derive(Clone, Debug, PartialEq)]
pub enum IpcTable {
    /// A dense array, `{ a, b, c }`. An empty table is an empty list.
    List(Vec<IpcValue>),
    /// A record with string keys.
    Map(BTreeMap<String, IpcValue>),
}

impl From<bool> for IpcValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<f64> for IpcValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<i64> for IpcValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<&str> for IpcValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for IpcValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl IpcValue {
    /// The value as JSON: what a table looks like where only text goes.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::Value as Json;
        match self {
            Self::Nil => Json::Null,
            Self::Boolean(value) => Json::Bool(*value),
            Self::Integer(value) => Json::from(*value),
            Self::Number(value) => {
                serde_json::Number::from_f64(*value).map_or(Json::Null, Json::Number)
            }
            Self::String(value) => Json::String(value.clone()),
            Self::Color(color) => Json::String(color.to_pastel().to_rgb_hex_string(true)),
            Self::Table(table) => match &**table {
                IpcTable::List(items) => Json::Array(items.iter().map(Self::to_json).collect()),
                IpcTable::Map(fields) => Json::Object(
                    fields
                        .iter()
                        .map(|(key, value)| (key.clone(), value.to_json()))
                        .collect(),
                ),
            },
        }
    }
}
