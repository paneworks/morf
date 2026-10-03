use crate::dbus_decode::array_value;
use crate::dbus_decode::structure_value;
use zbus::zvariant::{Array, Dict, ObjectPath, Signature, Structure, StructureBuilder, Value};

use crate::dbus_types::DbusValue;

/// Encodes one value whose type D-Bus can infer from the value itself.
///
/// `role` names what the value is being used as, so the two things that cannot
/// be inferred — a nil, and a compound with no signature — can say so in the
/// caller's own terms. It is the only thing that differed between the two
/// copies this replaces.
fn dbus_scalar_value<'a>(value: &'a DbusValue, role: &str) -> Result<Value<'a>, String> {
    match value {
        DbusValue::Bool(value) => Ok(Value::Bool(*value)),
        DbusValue::Integer(value) => Ok(Value::I64(*value)),
        DbusValue::Unsigned(value) => Ok(Value::U64(*value)),
        DbusValue::Number(value) => Ok(Value::F64(*value)),
        DbusValue::String(value) => Ok(Value::Str(value.as_str().into())),
        DbusValue::Typed { signature, value } => typed_dbus_value(signature, value),
        DbusValue::Fd(fd) => Ok(Value::Fd(fd.as_fd().into())),
        DbusValue::Bytes(bytes) => byte_array(bytes),
        DbusValue::Nil => Err(format!("nil cannot be a {role}")),
        DbusValue::List(_) | DbusValue::Map(_) => {
            Err(format!("a compound {role} needs an explicit signature"))
        }
    }
}

pub(crate) fn dbus_argument_value(value: &DbusValue) -> Result<Value<'_>, String> {
    match value {
        // A list of strings and a string-keyed map have one shape each on the
        // bus -- `as` and `a{sv}` -- and asking for a signature to say so is
        // asking the obvious. Anything less obvious still has to say.
        DbusValue::List(_) | DbusValue::Map(_) => inferred_dbus_value(value).map_err(|_| {
            "a compound positional D-Bus argument needs an explicit signature".to_owned()
        }),
        other => dbus_scalar_value(other, "positional D-Bus argument"),
    }
}

pub(crate) fn typed_dbus_value<'a>(
    signature: &str,
    value: &'a DbusValue,
) -> Result<Value<'a>, String> {
    let signature = Signature::try_from(signature)
        .map_err(|error| format!("invalid D-Bus signature: {error}"))?;
    dbus_value_for_signature(&signature, value)
}

fn dbus_value_for_signature<'a>(
    signature: &Signature,
    value: &'a DbusValue,
) -> Result<Value<'a>, String> {
    let name = signature.to_string();
    let integer = || match value {
        DbusValue::Integer(value) => Ok(i128::from(*value)),
        DbusValue::Unsigned(value) => Ok(i128::from(*value)),
        _ => Err(format!("D-Bus `{name}` value must be an integer")),
    };
    let range_error = || format!("D-Bus `{name}` integer is out of range");
    Ok(match signature {
        Signature::U8 => Value::U8(u8::try_from(integer()?).map_err(|_| range_error())?),
        Signature::I16 => Value::I16(i16::try_from(integer()?).map_err(|_| range_error())?),
        Signature::U16 => Value::U16(u16::try_from(integer()?).map_err(|_| range_error())?),
        Signature::I32 => Value::I32(i32::try_from(integer()?).map_err(|_| range_error())?),
        Signature::U32 => Value::U32(u32::try_from(integer()?).map_err(|_| range_error())?),
        Signature::I64 => Value::I64(i64::try_from(integer()?).map_err(|_| range_error())?),
        Signature::U64 => Value::U64(u64::try_from(integer()?).map_err(|_| range_error())?),
        Signature::F64 => match value {
            DbusValue::Number(value) => Value::F64(*value),
            DbusValue::Integer(value) => Value::F64(*value as f64),
            DbusValue::Unsigned(value) => Value::F64(*value as f64),
            _ => return Err("D-Bus `d` value must be numeric".to_owned()),
        },
        Signature::Bool => match value {
            DbusValue::Bool(value) => Value::Bool(*value),
            _ => return Err("D-Bus `b` value must be boolean".to_owned()),
        },
        Signature::Str => match value {
            DbusValue::String(value) => Value::Str(value.as_str().into()),
            // A Lua string that is not UTF-8, as it always was: made valid.
            DbusValue::Bytes(bytes) => {
                Value::Str(String::from_utf8_lossy(bytes).into_owned().into())
            }
            _ => return Err("D-Bus `s` value must be a string".to_owned()),
        },
        Signature::ObjectPath => match value {
            DbusValue::String(value) => Value::ObjectPath(
                ObjectPath::try_from(value.as_str()).map_err(|error| error.to_string())?,
            ),
            _ => return Err("D-Bus `o` value must be a string".to_owned()),
        },
        Signature::Signature => match value {
            DbusValue::String(value) => Value::Signature(
                Signature::try_from(value.as_str()).map_err(|error| error.to_string())?,
            ),
            _ => return Err("D-Bus `g` value must be a string".to_owned()),
        },
        Signature::Variant => Value::Value(Box::new(inferred_dbus_value(value)?)),
        // `ay` takes bytes as they come to Lua -- a string -- as well as a
        // list of numbers.
        Signature::Array(child) if matches!(child.signature(), Signature::U8) => match value {
            DbusValue::Bytes(bytes) => byte_array(bytes)?,
            DbusValue::String(text) => byte_array(text.as_bytes())?,
            DbusValue::List(values) => {
                let mut array = Array::new(&Signature::U8);
                for value in values {
                    array
                        .append(dbus_value_for_signature(&Signature::U8, value)?)
                        .map_err(|error| error.to_string())?;
                }
                Value::Array(array)
            }
            DbusValue::Map(values) if values.is_empty() => byte_array(&[])?,
            _ => return Err(format!("D-Bus `{name}` value must be a string or a list")),
        },
        Signature::Array(child) => {
            // An empty Lua table is an empty map as readily as an empty list;
            // with the signature stated, which it was is no longer a question.
            let values = match value {
                DbusValue::List(values) => values.as_slice(),
                DbusValue::Map(values) if values.is_empty() => &[],
                _ => return Err(format!("D-Bus `{name}` value must be a list")),
            };
            let mut array = Array::new(child.signature());
            for value in values {
                array
                    .append(dbus_value_for_signature(child.signature(), value)?)
                    .map_err(|error| error.to_string())?;
            }
            Value::Array(array)
        }
        Signature::Dict {
            key: key_signature,
            value: value_signature,
        } => {
            // `{}` reads as an empty list, because Lua cannot say otherwise —
            // and it was refused here as "not a map", which left no way at all
            // to send an empty `a{sv}`. The signature says it is a map; an
            // empty list is taken at its word.
            static NO_ENTRIES: std::collections::BTreeMap<String, DbusValue> =
                std::collections::BTreeMap::new();
            let values = match value {
                DbusValue::Map(values) => values,
                DbusValue::List(values) if values.is_empty() => &NO_ENTRIES,
                _ => return Err(format!("D-Bus `{name}` value must be a map")),
            };
            let mut dict = Dict::new(key_signature.signature(), value_signature.signature());
            for (key, value) in values {
                dict.append(
                    dbus_map_key(key_signature.signature(), key)?,
                    dbus_value_for_signature(value_signature.signature(), value)?,
                )
                .map_err(|error| error.to_string())?;
            }
            Value::Dict(dict)
        }
        Signature::Structure(fields) => {
            let DbusValue::List(values) = value else {
                return Err(format!("D-Bus `{name}` value must be a list"));
            };
            if values.len() != fields.len() {
                return Err(format!(
                    "D-Bus `{name}` needs {} fields, found {}",
                    fields.len(),
                    values.len()
                ));
            }
            let mut structure = StructureBuilder::new();
            for (field, value) in fields.iter().zip(values) {
                structure = structure.append_field(dbus_value_for_signature(field, value)?);
            }
            Value::Structure(structure.build().map_err(|error| error.to_string())?)
        }
        Signature::Unit => return Err("D-Bus unit values cannot be arguments".to_owned()),
        // Only ever one that came from the bus: nothing in a configuration can
        // make a `DbusFd`, so this hands back what it was given and no more.
        #[cfg(unix)]
        Signature::Fd => match value {
            DbusValue::Fd(fd) => Value::Fd(fd.as_fd().into()),
            _ => return Err("D-Bus `h` value must be a file descriptor handle".to_owned()),
        },
        #[allow(unreachable_patterns)]
        _ => return Err(format!("unsupported explicit D-Bus signature `{name}`")),
    })
}

/// An `ay` holding `bytes`.
fn byte_array(bytes: &[u8]) -> Result<Value<'static>, String> {
    let mut array = Array::new(&Signature::U8);
    for byte in bytes {
        array
            .append(Value::U8(*byte))
            .map_err(|error| error.to_string())?;
    }
    Ok(Value::Array(array))
}

fn dbus_map_key<'a>(signature: &Signature, key: &'a str) -> Result<Value<'a>, String> {
    match signature {
        Signature::Str => Ok(Value::Str(key.into())),
        Signature::ObjectPath => Ok(Value::ObjectPath(
            ObjectPath::try_from(key).map_err(|error| error.to_string())?,
        )),
        Signature::Signature => Ok(Value::Signature(
            Signature::try_from(key).map_err(|error| error.to_string())?,
        )),
        _ => Err(format!(
            "D-Bus map keys from Lua cannot use signature `{signature}`"
        )),
    }
}

/// What goes inside a variant when nobody said.
///
/// A scalar is itself. A value that carries its own signature is that -- which
/// is how a property of type `as` travels inside the `v` a `Get` reply wants.
/// A list of strings is `as` because that is the only thing a list of strings
/// ever is on the bus, and a string-keyed map is `a{sv}` with each value
/// inferred in turn, because that is what `GetAll` and every hints dictionary
/// are. Anything else has to say what it is.
fn inferred_dbus_value(value: &DbusValue) -> Result<Value<'_>, String> {
    match value {
        DbusValue::Typed { signature, value } => typed_dbus_value(signature, value),
        DbusValue::List(values) if values.iter().all(|v| matches!(v, DbusValue::String(_))) => {
            let mut array = Array::new(&Signature::Str);
            for value in values {
                if let DbusValue::String(text) = value {
                    array
                        .append(Value::from(text.as_str()))
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(Value::Array(array))
        }
        DbusValue::Map(entries) => {
            let mut dict = Dict::new(&Signature::Str, &Signature::Variant);
            for (key, value) in entries {
                dict.append(
                    Value::from(key.as_str()),
                    Value::Value(Box::new(inferred_dbus_value(value)?)),
                )
                .map_err(|error| error.to_string())?;
            }
            Ok(Value::Dict(dict))
        }
        other => dbus_scalar_value(other, "D-Bus variant"),
    }
}

pub(crate) fn decode_message_value(message: &zbus::Message) -> Result<DbusValue, String> {
    let body = message.body();
    if body.deserialize::<()>().is_ok() {
        return Ok(DbusValue::Nil);
    }
    // A body carrying a descriptor skips the scalar guesses below: zvariant
    // reads an `h` as an `i32` when asked for one — the descriptor's number in
    // this process — and the first guess to succeed would have been that.
    let signature = body.signature().to_string_no_parens();
    if signature.contains('h') {
        #[cfg(unix)]
        if signature == "h" {
            let fd = body
                .deserialize::<zbus::zvariant::Fd<'_>>()
                .map_err(|error| error.to_string())?;
            return crate::dbus_decode::owned_fd(&fd).map(DbusValue::Fd);
        }
        if let Ok(value) = body.deserialize::<Structure<'_>>() {
            return structure_value(&value);
        }
        if let Ok(value) = body.deserialize::<Array<'_>>() {
            return array_value(&value);
        }
        return Err(format!("D-Bus reply type `{signature}` is not supported"));
    }
    if let Ok(value) = body.deserialize::<bool>() {
        return Ok(DbusValue::Bool(value));
    }
    if let Ok(value) = body.deserialize::<i16>() {
        return Ok(DbusValue::Integer(value as i64));
    }
    if let Ok(value) = body.deserialize::<i32>() {
        return Ok(DbusValue::Integer(value as i64));
    }
    if let Ok(value) = body.deserialize::<i64>() {
        return Ok(DbusValue::Integer(value));
    }
    if let Ok(value) = body.deserialize::<u8>() {
        return Ok(DbusValue::Unsigned(value as u64));
    }
    if let Ok(value) = body.deserialize::<u16>() {
        return Ok(DbusValue::Unsigned(value as u64));
    }
    if let Ok(value) = body.deserialize::<u32>() {
        return Ok(DbusValue::Unsigned(value as u64));
    }
    if let Ok(value) = body.deserialize::<u64>() {
        return Ok(DbusValue::Unsigned(value));
    }
    if let Ok(value) = body.deserialize::<f64>() {
        return Ok(DbusValue::Number(value));
    }
    if let Ok(value) = body.deserialize::<String>() {
        return Ok(DbusValue::String(value));
    }
    if let Ok(value) = body.deserialize::<Structure<'_>>() {
        return structure_value(&value);
    }
    if let Ok(value) = body.deserialize::<Array<'_>>() {
        return array_value(&value);
    }
    Err("D-Bus reply type is not supported".to_owned())
}
