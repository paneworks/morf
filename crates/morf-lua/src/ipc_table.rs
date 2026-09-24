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

use crate::surface_types::IpcValue;

/// How deep a signal's table may nest, and how many entries it may hold in
/// all: a signal is state, not storage.
const MAX_DEPTH: usize = 16;
const MAX_ENTRIES: usize = 65_536;

/// The table inside an `IpcValue::Table`.
#[derive(Clone, Debug, PartialEq)]
pub enum IpcTable {
    /// A dense array, `{ a, b, c }`. An empty table is an empty list.
    List(Vec<IpcValue>),
    /// A record with string keys.
    Map(BTreeMap<String, IpcValue>),
}

impl IpcValue {
    /// Reads a Lua value a signal may hold: anything `from_lua` takes, and
    /// tables of those, copied deeply.
    pub(crate) fn from_lua_deep<'gc>(
        ctx: Context<'gc>,
        value: LuaValue<'gc>,
    ) -> Result<Self, String> {
        let mut entries = 0;
        from_lua_deep(ctx, value, 0, &mut entries)
    }

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

impl IpcTable {
    pub(crate) fn to_lua<'gc>(&self, ctx: Context<'gc>) -> LuaValue<'gc> {
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
