//! D-Bus values to Lua and back, and descriptors from the bus as handles.

use super::*;

pub(crate) fn dbus_value_to_lua(
    ctx: Context<'_>,
    value: DbusValue,
) -> Result<LuaValue<'_>, String> {
    Ok(match value {
        DbusValue::Nil => LuaValue::Nil,
        DbusValue::Bool(value) => LuaValue::Boolean(value),
        DbusValue::Integer(value) => LuaValue::Integer(value),
        DbusValue::Unsigned(value) if value <= i64::MAX as u64 => LuaValue::Integer(value as i64),
        DbusValue::Unsigned(value) => LuaValue::Number(value as f64),
        DbusValue::Number(value) => LuaValue::Number(value),
        DbusValue::String(value) => LuaValue::String(ctx.intern(value.as_bytes())),
        // `ay` is bytes, and Lua's bytes are a string.
        DbusValue::Bytes(bytes) => LuaValue::String(ctx.intern(&bytes)),
        DbusValue::List(values) => {
            let table = Table::new(&ctx);
            for (index, value) in values.into_iter().enumerate() {
                table
                    .set(ctx, index as i64 + 1, dbus_value_to_lua(ctx, value)?)
                    .map_err(|error| error.to_string())?;
            }
            LuaValue::Table(table)
        }
        DbusValue::Map(values) => {
            let table = Table::new(&ctx);
            for (key, value) in values {
                table
                    .set(
                        ctx,
                        ctx.intern(key.as_bytes()),
                        dbus_value_to_lua(ctx, value)?,
                    )
                    .map_err(|error| error.to_string())?;
            }
            LuaValue::Table(table)
        }
        DbusValue::Typed { signature, value } => {
            let table = Table::new(&ctx);
            table.set_field(ctx, "signature", signature.as_str());
            table.set_field(ctx, "value", dbus_value_to_lua(ctx, *value)?);
            LuaValue::Table(table)
        }
        DbusValue::Fd(fd) => LuaValue::UserData(dbus_fd_userdata(ctx, fd)),
    })
}

/// A descriptor from the bus as a handle a configuration can hold, close,
/// and pass back — and nothing else.
///
/// Closed when `:close()` is called, when the handle is collected, and when
/// the runtime holding it goes: the last two are the same drop, so a
/// configuration that forgets a handle releases whatever it held (an
/// inhibitor lock, say) rather than holding it for the life of the shell.
fn dbus_fd_userdata<'gc>(ctx: Context<'gc>, fd: morf_io::DbusFd) -> UserData<'gc> {
    let close = luna::Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let handle: luna::UserRef<DbusFdToken> = stack.consume(ctx)?;
        let was_open = handle.fd.borrow_mut().take().is_some();
        stack.replace(ctx, was_open);
        Ok(luna::CallbackReturn::Return)
    });
    let is_open = luna::Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let handle: luna::UserRef<DbusFdToken> = stack.consume(ctx)?;
        let open = handle.fd.borrow().is_some();
        stack.replace(ctx, open);
        Ok(luna::CallbackReturn::Return)
    });
    let methods = Table::new(&ctx);
    methods.set_field(ctx, "close", close);
    methods.set_field(ctx, "is_open", is_open);
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__index", methods);
    metatable.set_field(ctx, "__name", "morf.dbus.fd");
    let userdata = UserData::new_static(
        &ctx,
        DbusFdToken {
            fd: std::cell::RefCell::new(Some(fd)),
        },
    );
    userdata.set_metatable(ctx, Some(metatable));
    userdata
}

pub(crate) fn lua_to_dbus<'gc>(
    ctx: Context<'gc>,
    value: LuaValue<'gc>,
    depth: usize,
) -> Result<DbusValue, String> {
    if depth > 8 {
        return Err("D-Bus value exceeds maximum depth 8".to_owned());
    }
    match value {
        LuaValue::Nil => Ok(DbusValue::Nil),
        LuaValue::Boolean(value) => Ok(DbusValue::Bool(value)),
        LuaValue::Integer(value) => Ok(DbusValue::Integer(value)),
        LuaValue::Number(value) if value.is_finite() => Ok(DbusValue::Number(value)),
        // Text is a string; anything that is not UTF-8 can only be bytes.
        LuaValue::String(value) => Ok(match std::str::from_utf8(value.as_bytes()) {
            Ok(text) => DbusValue::String(text.to_owned()),
            Err(_) => DbusValue::Bytes(value.as_bytes().to_vec()),
        }),
        LuaValue::Table(table) => {
            if let LuaValue::String(signature) = table.get_value(ctx, "signature") {
                let value = table.get_value(ctx, "value");
                return Ok(DbusValue::Typed {
                    signature: signature.display_lossy().to_string(),
                    value: Box::new(lua_to_dbus(ctx, value, depth + 1)?),
                });
            }
            let entries = table.iter(ctx).collect::<Vec<_>>();
            if entries.len() > 256 {
                return Err("D-Bus table exceeds 256 entries".to_owned());
            }
            if entries.is_empty()
                || entries
                    .iter()
                    .all(|(key, _)| matches!(key, LuaValue::Integer(_)))
            {
                let mut values = entries
                    .into_iter()
                    .map(|(key, value)| {
                        let LuaValue::Integer(index) = key else {
                            unreachable!()
                        };
                        Ok((index, lua_to_dbus(ctx, value, depth + 1)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                values.sort_by_key(|(index, _)| *index);
                for (offset, (index, _)) in values.iter().enumerate() {
                    if *index != offset as i64 + 1 {
                        return Err("D-Bus list must be a dense sequence".to_owned());
                    }
                }
                Ok(DbusValue::List(
                    values.into_iter().map(|(_, value)| value).collect(),
                ))
            } else if entries
                .iter()
                .all(|(key, _)| matches!(key, LuaValue::String(_)))
            {
                let mut values = BTreeMap::new();
                for (key, value) in entries {
                    let LuaValue::String(key) = key else {
                        unreachable!()
                    };
                    values.insert(
                        key.display_lossy().to_string(),
                        lua_to_dbus(ctx, value, depth + 1)?,
                    );
                }
                Ok(DbusValue::Map(values))
            } else {
                Err("D-Bus table keys must be all integers or all strings".to_owned())
            }
        }
        // The only userdata that means anything on the bus is a descriptor
        // that came from it.
        LuaValue::UserData(userdata) => {
            let Ok(handle) = userdata.downcast_static::<DbusFdToken>() else {
                return Err("unsupported D-Bus value".to_owned());
            };
            handle
                .fd
                .borrow()
                .clone()
                .map(DbusValue::Fd)
                .ok_or_else(|| "the file descriptor handle was closed".to_owned())
        }
        _ => Err("unsupported D-Bus value".to_owned()),
    }
}
