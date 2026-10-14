//! Reading a spec's arguments and building what it is handed back.

use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

pub(crate) fn text(value: &IpcValue) -> Option<String> {
    match value {
        IpcValue::String(text) => Some(text.clone()),
        IpcValue::Integer(number) => Some(number.to_string()),
        IpcValue::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub(crate) fn number(value: Option<&IpcValue>, what: &str) -> Result<f64, String> {
    match value {
        Some(IpcValue::Integer(number)) => Ok(*number as f64),
        Some(IpcValue::Number(number)) if number.is_finite() => Ok(*number),
        other => Err(format!("{what} must be a number, not {other:?}")),
    }
}

pub(crate) fn optional_text(value: Option<&IpcValue>) -> Option<String> {
    value.and_then(text)
}

pub(crate) fn field<'a>(table: Option<&'a IpcValue>, name: &str) -> Option<&'a IpcValue> {
    match table {
        Some(IpcValue::Table(table)) => match &**table {
            IpcTable::Map(fields) => fields.get(name),
            IpcTable::List(_) => None,
        },
        _ => None,
    }
}

pub(crate) fn list(value: Option<&IpcValue>) -> Vec<IpcValue> {
    match value {
        Some(IpcValue::Table(table)) => match &**table {
            IpcTable::List(items) => items.clone(),
            IpcTable::Map(fields) if fields.is_empty() => Vec::new(),
            IpcTable::Map(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub(crate) fn map(fields: Vec<(&str, IpcValue)>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::Map(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )))
}

pub(crate) fn string(text: impl Into<String>) -> IpcValue {
    IpcValue::String(text.into())
}
