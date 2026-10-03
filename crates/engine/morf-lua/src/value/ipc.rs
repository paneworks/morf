//! Tables a signal can hold.
//!
//! A signal held one scalar, so a record -- a window's title and class, a
//! player's track -- had to be a `morf.state`, whose fields are fixed at
//! creation. A signal may now hold a JSON-like table: string keys or a
//! dense array, of scalars, colours and more such tables. It is copied in
//! on `set` and copied out on `get`, so neither side can change the other's
//! table behind the graph's back, and it is compared by content, so
//! writing an equal table re-runs nothing.
//!
//! It is a value like any other in the graph (`IpcValue::Table`), not a
//! Lua-side registry: a registry would need its own equality and its own
//! lifetime. Across the IPC socket, which carries scalars, a table goes as
//! its JSON text.

use luna::{Context, Table, Value as LuaValue};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) use morf_value::{IpcTable, IpcValue};

/// How deep a signal's table may nest, and how many entries it may hold in
/// all: a signal is state, not storage.
const MAX_DEPTH: usize = 16;
const MAX_ENTRIES: usize = 65_536;

/// A value or a table as Lua sees it: a fresh copy each time, so what a
/// reader does to it stays with the reader.
pub(crate) trait IpcToLua {
    fn to_lua<'gc>(&self, ctx: Context<'gc>) -> LuaValue<'gc>;
}

/// A Lua value read into one that may cross the boundary.
pub(crate) trait IpcFromLua: Sized {
    /// A scalar or a colour: nil, boolean, number, string, colour.
    fn from_lua(value: LuaValue<'_>) -> Result<Self, String>;
    /// Anything `from_lua` takes, and tables of those, copied deeply.
    fn from_lua_deep<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Self, String>;
}

impl IpcToLua for IpcValue {
    fn to_lua<'gc>(&self, ctx: Context<'gc>) -> LuaValue<'gc> {
        match self {
            Self::Nil => LuaValue::Nil,
            Self::Boolean(value) => LuaValue::Boolean(*value),
            Self::Integer(value) => LuaValue::Integer(*value),
            Self::Number(value) => LuaValue::Number(*value),
            Self::String(value) => LuaValue::String(ctx.intern(value.as_bytes())),
            Self::Color(color) => crate::api_color::scene_color_userdata(ctx, *color),
            // A fresh copy each time: what a reader does to it stays with
            // the reader.
            Self::Table(table) => table.to_lua(ctx),
        }
    }
}

impl IpcFromLua for IpcValue {
    fn from_lua(value: LuaValue<'_>) -> Result<Self, String> {
        match value {
            LuaValue::Nil => Ok(Self::Nil),
            LuaValue::Boolean(value) => Ok(Self::Boolean(value)),
            LuaValue::Integer(value) => Ok(Self::Integer(value)),
            LuaValue::Number(value) if value.is_finite() => Ok(Self::Number(value)),
            LuaValue::String(value) => Ok(Self::String(value.display_lossy().to_string())),
            LuaValue::UserData(userdata)
                if userdata
                    .downcast_static::<crate::api_color::ColorToken>()
                    .is_ok() =>
            {
                let token = userdata
                    .downcast_static::<crate::api_color::ColorToken>()
                    .expect("checked above");
                Ok(Self::Color(morf_scene::Color::from_pastel(&token.color)))
            }
            value => Err(format!(
                "values crossing the Lua boundary must be nil, boolean, number, string or colour, found {}",
                value.type_name()
            )),
        }
    }

    fn from_lua_deep<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Self, String> {
        let mut entries = 0;
        from_lua_deep(ctx, value, 0, &mut entries)
    }
}
fn from_lua_deep<'gc>(
    ctx: Context<'gc>,
    value: LuaValue<'gc>,
    depth: usize,
    entries: &mut usize,
) -> Result<IpcValue, String> {
    let LuaValue::Table(table) = value else {
        return IpcValue::from_lua(value);
    };
    if depth >= MAX_DEPTH {
        return Err(format!(
            "a table a signal holds nests deeper than {MAX_DEPTH} levels (is it its own member?)"
        ));
    }
    let mut integers = Vec::new();
    let mut strings = BTreeMap::new();
    for (key, value) in table.iter(ctx) {
        *entries += 1;
        if *entries > MAX_ENTRIES {
            return Err(format!(
                "a table a signal holds has more than {MAX_ENTRIES} entries"
            ));
        }
        let value = from_lua_deep(ctx, value, depth + 1, entries)?;
        match key {
            LuaValue::Integer(index) if index >= 1 => integers.push((index, value)),
            LuaValue::String(key) => {
                strings.insert(key.display_lossy().to_string(), value);
            }
            key => {
                return Err(format!(
                    "a table a signal holds needs string keys or an array, found a {} key",
                    key.type_name()
                ));
            }
        }
    }
    let table = match (integers.is_empty(), strings.is_empty()) {
        (_, true) => {
            integers.sort_by_key(|(index, _)| *index);
            if integers
                .iter()
                .enumerate()
                .any(|(position, (index, _))| *index != position as i64 + 1)
            {
                return Err("a table a signal holds must be a dense array, without holes".into());
            }
            IpcTable::List(integers.into_iter().map(|(_, value)| value).collect())
        }
        (true, false) => IpcTable::Map(strings),
        (false, false) => {
            return Err("a table a signal holds is an array or a record, not both".into());
        }
    };
    Ok(IpcValue::Table(Arc::new(table)))
}

impl IpcToLua for IpcTable {
    fn to_lua<'gc>(&self, ctx: Context<'gc>) -> LuaValue<'gc> {
        let table = Table::new(&ctx);
        match self {
            Self::List(items) => {
                for (index, value) in items.iter().enumerate() {
                    table
                        .set(ctx, index as i64 + 1, value.to_lua(ctx))
                        .expect("a table accepts integer keys");
                }
            }
            Self::Map(fields) => {
                for (key, value) in fields {
                    table
                        .set(ctx, ctx.intern(key.as_bytes()), value.to_lua(ctx))
                        .expect("a table accepts string keys");
                }
            }
        }
        LuaValue::Table(table)
    }
}
