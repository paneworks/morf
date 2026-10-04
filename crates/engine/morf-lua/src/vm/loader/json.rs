//! JSON to Lua and back.

use super::*;

pub(crate) fn json_to_lua<'gc>(
    ctx: Context<'gc>,
    value: &serde_json::Value,
    array_metatable: Table<'gc>,
    object_metatable: Table<'gc>,
    null: UserData<'gc>,
    depth: usize,
    entries: &mut usize,
) -> Result<LuaValue<'gc>, String> {
    if depth > 64 {
        return Err("JSON value exceeds maximum depth 64".to_owned());
    }
    *entries += 1;
    if *entries > 65_536 {
        return Err("JSON value exceeds 65536 entries".to_owned());
    }
    Ok(match value {
        serde_json::Value::Null => LuaValue::UserData(null),
        serde_json::Value::Bool(value) => LuaValue::Boolean(*value),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                LuaValue::Integer(value)
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                LuaValue::Integer(value)
            } else {
                LuaValue::Number(
                    value
                        .as_f64()
                        .ok_or_else(|| "JSON number is not representable".to_owned())?,
                )
            }
        }
        serde_json::Value::String(value) => LuaValue::String(ctx.intern(value.as_bytes())),
        serde_json::Value::Array(values) => {
            let table = Table::new(&ctx);
            table.set_metatable(ctx, Some(array_metatable));
            for (index, value) in values.iter().enumerate() {
                table
                    .set(
                        ctx,
                        index as i64 + 1,
                        json_to_lua(
                            ctx,
                            value,
                            array_metatable,
                            object_metatable,
                            null,
                            depth + 1,
                            entries,
                        )?,
                    )
                    .map_err(|error| error.to_string())?;
            }
            LuaValue::Table(table)
        }
        serde_json::Value::Object(values) => {
            let table = Table::new(&ctx);
            table.set_metatable(ctx, Some(object_metatable));
            for (key, value) in values {
                table
                    .set(
                        ctx,
                        ctx.intern(key.as_bytes()),
                        json_to_lua(
                            ctx,
                            value,
                            array_metatable,
                            object_metatable,
                            null,
                            depth + 1,
                            entries,
                        )?,
                    )
                    .map_err(|error| error.to_string())?;
            }
            LuaValue::Table(table)
        }
    })
}

pub(crate) fn lua_to_json<'gc>(
    ctx: Context<'gc>,
    value: LuaValue<'gc>,
    depth: usize,
    entries: &mut usize,
) -> Result<serde_json::Value, String> {
    if depth > 64 {
        return Err("JSON value exceeds maximum depth 64".to_owned());
    }
    *entries += 1;
    if *entries > 65_536 {
        return Err("JSON value exceeds 65536 entries".to_owned());
    }
    match value {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(value) => Ok(serde_json::Value::Bool(value)),
        LuaValue::Integer(value) => Ok(serde_json::Value::Number(value.into())),
        LuaValue::Number(value) if value.is_finite() => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| "JSON number is not representable".to_owned()),
        LuaValue::String(value) => Ok(serde_json::Value::String(value.display_lossy().to_string())),
        LuaValue::UserData(value) if value.is_static::<JsonNullToken>() => {
            Ok(serde_json::Value::Null)
        }
        LuaValue::Table(table) => {
            let kind = table.metatable().and_then(|metatable| {
                let LuaValue::String(kind) = metatable.get_value(ctx, "__json_kind") else {
                    return None;
                };
                Some(kind.display_lossy().to_string())
            });
            let values = table.iter(ctx).collect::<Vec<_>>();
            let is_array = match kind.as_deref() {
                Some("array") => true,
                Some("object") => false,
                Some(_) => return Err("unknown JSON table kind".to_owned()),
                None => {
                    !values.is_empty()
                        && values
                            .iter()
                            .all(|(key, _)| matches!(key, LuaValue::Integer(_)))
                }
            };
            if is_array {
                let mut values = values
                    .into_iter()
                    .map(|(key, value)| {
                        let LuaValue::Integer(index) = key else {
                            return Err("JSON array keys must be integers".to_owned());
                        };
                        Ok((index, lua_to_json(ctx, value, depth + 1, entries)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                values.sort_by_key(|(index, _)| *index);
                for (offset, (index, _)) in values.iter().enumerate() {
                    if *index != offset as i64 + 1 {
                        return Err("JSON arrays must be dense sequences".to_owned());
                    }
                }
                Ok(serde_json::Value::Array(
                    values.into_iter().map(|(_, value)| value).collect(),
                ))
            } else {
                let mut object = serde_json::Map::new();
                for (key, value) in values {
                    let LuaValue::String(key) = key else {
                        return Err("JSON object keys must be strings".to_owned());
                    };
                    object.insert(
                        key.display_lossy().to_string(),
                        lua_to_json(ctx, value, depth + 1, entries)?,
                    );
                }
                Ok(serde_json::Value::Object(object))
            }
        }
        value => Err(format!(
            "JSON does not support Lua {} values",
            value.type_name()
        )),
    }
}
