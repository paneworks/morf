//! Manifests: where a module's declaration lives, and reading one as data.
//!
//! A directory module (`name/init.lua`) declares in `name/permissions.lua`;
//! a single-file module or plugin (`name.lua`) in `name.permissions.lua`
//! beside it. The file is a Lua chunk returning a table of groups, run in
//! an empty environment -- no `morf`, no globals, a small fuel budget -- so
//! reading it can only ever produce a value.

use luna::{Closure, Context, Executor, Table, Value as LuaValue};
use std::path::{Path, PathBuf};

use super::{Declaration, Grant};
use crate::vm::execute::drive_executor;
use crate::vm::types::Limits;

/// Instructions a manifest may spend before it is called broken.
const MANIFEST_FUEL: u64 = 100_000;

/// The names one group may list.
const MAX_NAMES: usize = 64;

/// Where the manifest for the module entered at `entry` lives.
pub fn manifest_path(entry: &Path) -> PathBuf {
    let directory = entry.parent().unwrap_or_else(|| Path::new(""));
    match entry.file_name().and_then(|name| name.to_str()) {
        Some("init.lua") => directory.join("permissions.lua"),
        Some(name) => {
            let stem = name.strip_suffix(".lua").unwrap_or(name);
            directory.join(format!("{stem}.permissions.lua"))
        }
        None => directory.join("permissions.lua"),
    }
}

/// Runs the manifest at `path` as data and reads what it returns.
pub(crate) fn read_manifest<'gc>(
    ctx: Context<'gc>,
    path: &Path,
    limits: Limits,
) -> Result<Declaration, String> {
    let table = run_data(ctx, path, limits, "a manifest")?;
    parse_table(ctx, table).map_err(|error| format!("{}: {error}", path.display()))
}

/// Runs the Lua source at `path` in an empty environment on a manifest's
/// budget and returns the table it gives back; `what` names the file in
/// errors.
pub(crate) fn run_data<'gc>(
    ctx: Context<'gc>,
    path: &Path,
    limits: Limits,
    what: &str,
) -> Result<Table<'gc>, String> {
    let source = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if luna::dump::is_binary_chunk(&source) {
        return Err(format!(
            "{}: {what} is Lua source, not bytecode",
            path.display()
        ));
    }
    let name = format!("@{}", path.display());
    let closure = Closure::load_with_env(ctx, Some(&name), &source, Table::new(&ctx))
        .map_err(|error| error.to_string())?;
    let executor = Executor::start(ctx, closure.into(), ());
    drive_executor(ctx, executor, limits, MANIFEST_FUEL, "manifest")?;
    match executor.take_result::<LuaValue>(ctx) {
        Ok(Ok(LuaValue::Table(table))) => Ok(table),
        Ok(Ok(_)) => Err(format!("{}: {what} must return a table", path.display())),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_table<'gc>(ctx: Context<'gc>, table: Table<'gc>) -> Result<Declaration, String> {
    let mut declaration = Declaration::new();
    for (key, value) in table.iter(ctx) {
        let LuaValue::String(key) = key else {
            return Err("manifest keys must be group names".to_owned());
        };
        let group = key.display_lossy().to_string();
        let names = match value {
            LuaValue::Table(list) => Some(
                list.iter(ctx)
                    .map(|(_, name)| match name {
                        LuaValue::String(name) => Ok(name.display_lossy().to_string()),
                        _ => Err(format!("`{group}` lists names as strings")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            LuaValue::String(name) => Some(vec![name.display_lossy().to_string()]),
            LuaValue::Boolean(true) => None,
            LuaValue::Boolean(false) | LuaValue::Nil => continue,
            _ => return Err(format!("`{group}` must be true, false or a list of names")),
        };
        if let Some(grant) = parse_value(names)? {
            declaration.insert(group, grant);
        }
    }
    Ok(declaration)
}

/// A group's value: `None` is `true` (anything); an empty list declares
/// nothing.
fn parse_value(names: Option<Vec<String>>) -> Result<Option<Grant>, String> {
    let Some(mut names) = names else {
        return Ok(Some(Grant::Any));
    };
    if names.len() > MAX_NAMES {
        return Err(format!("a group lists at most {MAX_NAMES} names"));
    }
    names.retain(|name| !name.trim().is_empty());
    names.sort();
    names.dedup();
    Ok((!names.is_empty()).then_some(Grant::Names(names)))
}
