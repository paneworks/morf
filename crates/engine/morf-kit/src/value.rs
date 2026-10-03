//! Reading the values Lua passes in.

use morf_lua::IpcValue;

pub(crate) fn number(value: Option<&IpcValue>) -> Option<f64> {
    match value? {
        IpcValue::Integer(n) => Some(*n as f64),
        IpcValue::Number(n) => Some(*n),
        _ => None,
    }
}

pub(crate) fn boolean(value: Option<&IpcValue>) -> Option<bool> {
    match value? {
        IpcValue::Boolean(on) => Some(*on),
        _ => None,
    }
}

pub(crate) fn text(value: Option<&IpcValue>) -> Option<&str> {
    match value? {
        IpcValue::String(text) => Some(text),
        _ => None,
    }
}

pub(crate) fn expect_number(value: Option<&IpcValue>, what: &str) -> Result<f64, String> {
    number(value).ok_or_else(|| format!("{what} must be a number"))
}

pub(crate) fn expect_boolean(value: Option<&IpcValue>, what: &str) -> Result<bool, String> {
    boolean(value).ok_or_else(|| format!("{what} must be true or false"))
}
