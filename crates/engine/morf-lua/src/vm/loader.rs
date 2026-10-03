use luna::{Closure, Context, Executor, Table, UserData, Value as LuaValue};
use morf_io::DbusValue;
use morf_scene::{Value as SceneValue, ViewTransition};
use morf_system::XkbKeymap;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::{reactive_execute::*, scene_bindings::*, state::*, types::*};

pub(crate) fn default_module_roots() -> Vec<PathBuf> {
    std::env::var_os("MORF_RUNTIME_PATH")
        .into_iter()
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .collect()
}

/// The nearest `library/` holding a `lib/`, beside the folder `file` is in
/// or one above it: a project's own library.
pub fn project_library(file: &std::path::Path) -> Option<PathBuf> {
    let folder = file.parent()?;
    let absolute = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    absolute
        .ancestors()
        .map(|dir| dir.join("library"))
        .find(|library| library.join("lib").is_dir())
}

/// Where `require` looks for a configuration at `config`: its own folder,
/// then, when `external`, the nearest `library/` beside a folder it is in
/// (one holding a `lib/`: a project's own library, found the way a
/// project's own modules are elsewhere, so a shell in a repository runs
/// against the repository's library and not an older installed copy), every
/// `MORF_RUNTIME_PATH` entry, the user's `$XDG_DATA_HOME/morf/site` (or
/// `~/.local/share/morf/site`) and the installed library
/// `$XDG_DATA_HOME/morf/library`, without duplicates.
///
/// Public so every host that runs a configuration -- the shell, the frame
/// bench -- resolves modules the same way; a bench that looked only beside
/// the file failed on a configuration the shell loads fine.
pub fn runtimepath_roots(config: &std::path::Path, external: bool) -> Vec<PathBuf> {
    let mut roots = config
        .parent()
        .map(std::path::Path::to_path_buf)
        .into_iter()
        .collect::<Vec<_>>();
    if external {
        roots.extend(project_library(config));
        roots.extend(default_module_roots());
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            });
        if let Some(data) = data {
            // The user's own modules first, then the library `make install`
            // puts beside the binary: `require("lib.material")` from any
            // configuration, wherever it lives.
            roots.push(data.join("morf/site"));
            roots.push(data.join("morf/library"));
        }
        let dirs = std::env::var_os("XDG_DATA_DIRS")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        roots.extend(
            std::env::split_paths(&dirs)
                .filter(|path| path.is_absolute())
                .map(|path| path.join("morf/library")),
        );
    }
    let mut unique = Vec::new();
    for root in roots {
        if !unique.contains(&root) {
            unique.push(root);
        }
    }
    unique
}

pub(crate) fn load_runtime_module(roots: &[PathBuf], name: &str) -> Result<Vec<u8>, String> {
    if name.is_empty()
        || name.split('.').any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        })
    {
        return Err(format!("invalid module name `{name}`"));
    }
    let relative = name.replace('.', "/");
    for root in roots {
        for path in [
            root.join(format!("{relative}.lua")),
            root.join(&relative).join("init.lua"),
            root.join("lua").join(format!("{relative}.lua")),
            root.join("lua").join(&relative).join("init.lua"),
        ] {
            match fs::read(&path) {
                Ok(source) => return Ok(source),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("could not read {}: {error}", path.display())),
            }
        }
    }
    Err(format!("module `{name}` is not available"))
}

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

pub(crate) fn lua_index(index: i64) -> Result<usize, HostError> {
    let index = index
        .checked_sub(1)
        .ok_or_else(|| HostError("list-model indexes start at one".into()))?;
    usize::try_from(index).map_err(|_| HostError("list-model index is out of range".into()))
}

pub(crate) fn lua_insert_index(index: i64, length: usize) -> Result<usize, HostError> {
    if index == length as i64 + 1 {
        Ok(length)
    } else {
        lua_index(index)
    }
}

pub(crate) fn scene_to_lua<'gc>(
    ctx: Context<'gc>,
    value: &SceneValue,
) -> Result<LuaValue<'gc>, String> {
    Ok(match value {
        SceneValue::Nil => LuaValue::Nil,
        SceneValue::Bool(value) => LuaValue::Boolean(*value),
        SceneValue::Number(value) => LuaValue::Number(*value),
        SceneValue::String(value) => LuaValue::String(ctx.intern(value.as_bytes())),
        SceneValue::Color(color) => crate::api_color::scene_color_userdata(ctx, *color),
        SceneValue::List(values) => {
            let table = Table::new(&ctx);
            for (index, value) in values.iter().enumerate() {
                table
                    .set(ctx, index as i64 + 1, scene_to_lua(ctx, value)?)
                    .map_err(|error| error.to_string())?;
            }
            LuaValue::Table(table)
        }
        SceneValue::Map(values) => {
            let table = Table::new(&ctx);
            for (key, value) in values {
                table
                    .set(ctx, ctx.intern(key.as_bytes()), scene_to_lua(ctx, value)?)
                    .map_err(|error| error.to_string())?;
            }
            LuaValue::Table(table)
        }
    })
}

pub(crate) fn xkb_keymap_to_lua<'gc>(ctx: Context<'gc>, keymap: &XkbKeymap) -> Table<'gc> {
    let result = Table::new(&ctx);
    result.set_field(ctx, "source", keymap.source.as_str());
    let keys = Table::new(&ctx);
    for (key_index, key) in keymap.keys.iter().enumerate() {
        let value = Table::new(&ctx);
        value.set_field(ctx, "keycode", i64::from(key.keycode));
        value.set_field(ctx, "evdev_code", i64::from(key.evdev_code));
        value.set_field(ctx, "name", key.name.as_str());
        value.set_field(ctx, "repeats", key.repeats);
        let layouts = Table::new(&ctx);
        for (layout_index, layout) in key.layouts.iter().enumerate() {
            let levels = Table::new(&ctx);
            for (level_index, level) in layout.iter().enumerate() {
                let symbols = Table::new(&ctx);
                for (symbol_index, symbol) in level.iter().enumerate() {
                    let item = Table::new(&ctx);
                    item.set_field(ctx, "keysym", i64::from(symbol.keysym));
                    item.set_field(ctx, "name", symbol.name.as_str());
                    item.set_field(ctx, "text", symbol.text.as_str());
                    symbols
                        .set(ctx, symbol_index as i64 + 1, item)
                        .expect("XKB symbol table accepts integer keys");
                }
                levels
                    .set(ctx, level_index as i64 + 1, symbols)
                    .expect("XKB level table accepts integer keys");
            }
            layouts
                .set(ctx, layout_index as i64 + 1, levels)
                .expect("XKB layout table accepts integer keys");
        }
        value.set_field(ctx, "layouts", layouts);
        keys.set(ctx, key_index as i64 + 1, value)
            .expect("XKB key table accepts integer keys");
    }
    result.set_field(ctx, "keys", keys);
    result
}

pub(crate) fn view_transition_to_lua(ctx: Context<'_>, transition: ViewTransition) -> Table<'_> {
    let table = Table::new(&ctx);
    let (kind, item, from, targets) = match transition {
        ViewTransition::Populate(item) => ("populate", item, None, Vec::new()),
        ViewTransition::Add(item) => ("add", item, None, Vec::new()),
        ViewTransition::Remove(item) => ("remove", item, None, Vec::new()),
        ViewTransition::Move {
            item,
            from,
            target_indexes,
        } => ("move", item, Some(from), target_indexes),
        ViewTransition::Displaced {
            item,
            from,
            target_indexes,
        } => ("displaced", item, Some(from), target_indexes),
    };
    table.set_field(ctx, "kind", kind);
    table.set_field(ctx, "id", item.id.raw() as i64);
    table.set_field(ctx, "index", item.index as i64 + 1);
    table.set_field(ctx, "destination", item.destination);
    table.set_field(
        ctx,
        "from",
        from.map_or(LuaValue::Nil, |index| LuaValue::Integer(index as i64 + 1)),
    );
    let target_indexes = Table::new(&ctx);
    for (index, target) in targets.into_iter().enumerate() {
        target_indexes
            .set(ctx, index as i64 + 1, target as i64 + 1)
            .expect("target-index table accepts integer keys");
    }
    table.set_field(ctx, "target_indexes", target_indexes);
    table
}

pub(crate) fn execute_module<'gc>(
    ctx: Context<'gc>,
    name: &str,
    source: &[u8],
    limits: Limits,
) -> Result<LuaValue<'gc>, String> {
    let closure = Closure::load(ctx, Some(name), source).map_err(|error| error.to_string())?;
    let executor = Executor::start(ctx, closure.into(), ());
    drive_executor(ctx, executor, limits, limits.module_fuel, "module")?;
    match executor.take_result::<LuaValue>(ctx) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
