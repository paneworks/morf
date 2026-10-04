//! `morf.fs`: the filesystem, directly.
//!
//! Failures a configuration should expect — a missing file, a folder it may
//! not read — come back as `nil, message`, so `local text, err =
//! morf.fs.read(path)` is the idiom. Mistakes in the call itself — a
//! number where a path goes, an unknown option — raise, because no amount
//! of retrying fixes them.

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_io::fs as ops;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::scene_bindings::*;

use ops::{DEFAULT_READ, MAX_ASYNC_FILES};

fn path_of(value: LuaValue<'_>, what: &str) -> Result<PathBuf, HostError> {
    match value {
        LuaValue::String(text) if !text.as_bytes().is_empty() => {
            Ok(PathBuf::from(OsStr::from_bytes(text.as_bytes())))
        }
        LuaValue::String(_) => Err(HostError(format!("{what} is an empty path"))),
        _ => Err(HostError(format!("{what} must be a path string"))),
    }
}

fn bytes_of<'gc>(value: LuaValue<'gc>, what: &str) -> Result<&'gc [u8], HostError> {
    match value {
        LuaValue::String(text) => Ok(text.as_bytes()),
        _ => Err(HostError(format!("{what} must be a string"))),
    }
}

fn options<'gc>(value: LuaValue<'gc>, what: &str) -> Result<Option<Table<'gc>>, HostError> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::Table(table) => Ok(Some(table)),
        _ => Err(HostError(format!("{what} options must be a table"))),
    }
}

fn flag<'gc>(
    ctx: Context<'gc>,
    table: Option<Table<'gc>>,
    field: &str,
    default: bool,
) -> Result<bool, HostError> {
    let Some(table) = table else {
        return Ok(default);
    };
    match table.get_value(ctx, field) {
        LuaValue::Nil => Ok(default),
        LuaValue::Boolean(value) => Ok(value),
        _ => Err(HostError(format!("fs option {field} must be boolean"))),
    }
}

fn integer<'gc>(
    ctx: Context<'gc>,
    table: Option<Table<'gc>>,
    field: &str,
) -> Result<Option<i64>, HostError> {
    let Some(table) = table else {
        return Ok(None);
    };
    match table.get_value(ctx, field) {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(value) => Ok(Some(value)),
        LuaValue::Number(value) if value.fract() == 0.0 => Ok(Some(value as i64)),
        _ => Err(HostError(format!("fs option {field} must be an integer"))),
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn entry_table<'gc>(ctx: Context<'gc>, entry: &ops::Entry) -> Table<'gc> {
    let table = Table::new(&ctx);
    table.set_field(ctx, "name", entry.name.as_str());
    table.set_field(ctx, "path", text(&entry.path));
    table.set_field(ctx, "type", entry.kind.name());
    if let Some(target) = entry.target_kind {
        table.set_field(ctx, "target_type", target.name());
    }
    table.set_field(ctx, "size", entry.size as i64);
    table.set_field(ctx, "modified", entry.modified);
    table.set_field(ctx, "accessed", entry.accessed);
    if let Some(created) = entry.created {
        table.set_field(ctx, "created", created);
    }
    table.set_field(ctx, "mode", i64::from(entry.mode));
    table.set_field(ctx, "hidden", entry.hidden);
    table.set_field(ctx, "is_dir", entry.is_dir());
    table.set_field(ctx, "is_file", entry.is_file());
    table.set_field(ctx, "extension", entry.extension());
    table
}

/// `nil, message` for an I/O failure.
macro_rules! fail {
    ($stack:ident, $ctx:ident, $error:expr) => {{
        $stack.replace($ctx, (LuaValue::Nil, $error.to_string()));
        return Ok(CallbackReturn::Return);
    }};
}

mod change;
mod paths;

pub(crate) fn install_fs_api<'gc>(
    ctx: Context<'gc>,
    morf: Table<'gc>,
    state: std::rc::Rc<std::cell::RefCell<crate::state::ReactiveState>>,
) {
    let fs = Table::new(&ctx);

    // `read_async({ path, ... }, function(ok, contents) end)`: the files are
    // read on a worker, and the callback gets, on a later turn, a list in
    // the same order -- each file's text, or `false` where it could not be
    // read. For files whose read may block: a sensor, a slow mount.
    fs.set_field(
        ctx,
        "read_async",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (paths, callback, limit): (LuaValue, luna::Closure, LuaValue) =
                stack.consume(ctx)?;
            let paths = match paths {
                LuaValue::Table(list) => {
                    let mut out = Vec::new();
                    for index in 1..=MAX_ASYNC_FILES as i64 + 1 {
                        let value = list.get_value(ctx, index);
                        if value.is_nil() {
                            break;
                        }
                        if out.len() == MAX_ASYNC_FILES {
                            return Err(HostError(format!(
                                "fs.read_async reads at most {MAX_ASYNC_FILES} files"
                            ))
                            .into());
                        }
                        out.push(path_of(value, "fs.read_async")?);
                    }
                    out
                }
                value => vec![path_of(value, "fs.read_async")?],
            };
            let limit = match limit {
                LuaValue::Nil => DEFAULT_READ,
                LuaValue::Integer(limit) => {
                    ops::read_limit(limit, "fs.read_async").map_err(HostError)?
                }
                _ => return Err(HostError("fs.read_async limit must be an integer".into()).into()),
            };
            let job = crate::image_jobs::ImageJob::ReadFiles { paths, limit };
            let submitted = state.borrow_mut().image_jobs.submit(
                job,
                Some(crate::vm::handler_store::register(ctx.stash(callback))),
            );
            match submitted {
                Ok(()) => stack.replace(ctx, true),
                Err(message) => fail!(stack, ctx, message),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "list",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (path, opts): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let path = path_of(path, "fs.list")?;
            let opts = options(opts, "fs.list")?;
            let depth = integer(ctx, opts, "depth")?.unwrap_or(0);
            let list_options = ops::ListOptions {
                hidden: flag(ctx, opts, "hidden", false)?,
                depth: ops::list_depth(depth),
                follow: flag(ctx, opts, "follow", false)?,
            };
            match ops::list(&path, list_options) {
                Ok((entries, truncated)) => {
                    let out = Table::new(&ctx);
                    for (index, entry) in entries.iter().enumerate() {
                        out.set(ctx, index as i64 + 1, entry_table(ctx, entry))?;
                    }
                    stack.replace(ctx, (out, truncated));
                }
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "stat",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let path: LuaValue = stack.consume(ctx)?;
            let path = path_of(path, "fs.stat")?;
            match ops::stat(&path) {
                Ok(entry) => stack.replace(ctx, entry_table(ctx, &entry)),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    for (name, test) in [
        ("exists", ops::PathTest::Exists),
        ("is_dir", ops::PathTest::Dir),
        ("is_file", ops::PathTest::File),
        ("is_link", ops::PathTest::Link),
    ] {
        fs.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let path: LuaValue = stack.consume(ctx)?;
                let path = path_of(path, name)?;
                stack.replace(ctx, ops::path_is(&path, test));
                Ok(CallbackReturn::Return)
            }),
        );
    }

    fs.set_field(
        ctx,
        "read",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (path, limit): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let path = path_of(path, "fs.read")?;
            // `read(path, { offset, length })` reads a window of the file:
            // what was appended since a remembered offset, the tail of a
            // log. `length` defaults to the rest, bounded like a whole read.
            if let LuaValue::Table(window) = limit {
                let offset = integer(ctx, Some(window), "offset")?;
                let length = integer(ctx, Some(window), "length")?;
                let (offset, length) = ops::read_window(offset, length).map_err(HostError)?;
                match ops::read_range(&path, offset, length) {
                    Ok(bytes) => stack.replace(ctx, luna::String::from_slice(&ctx, bytes)),
                    Err(error) => fail!(stack, ctx, error),
                }
                return Ok(CallbackReturn::Return);
            }
            let limit = match limit {
                LuaValue::Nil => DEFAULT_READ,
                LuaValue::Integer(limit) => ops::read_limit(limit, "fs.read").map_err(HostError)?,
                _ => {
                    return Err(HostError(
                        "fs.read takes a byte limit or { offset, length }".into(),
                    )
                    .into());
                }
            };
            match ops::read(&path, limit) {
                Ok(bytes) => stack.replace(ctx, luna::String::from_slice(&ctx, bytes)),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "lines",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (path, limit): (LuaValue, Option<i64>) = stack.consume(ctx)?;
            let path = path_of(path, "fs.lines")?;
            let most = limit.unwrap_or(i64::MAX).max(0) as usize;
            match ops::read(&path, DEFAULT_READ) {
                Ok(bytes) => {
                    let out = Table::new(&ctx);
                    for (index, line) in ops::split_lines(&bytes, most).into_iter().enumerate() {
                        out.set(ctx, index as i64 + 1, luna::String::from_slice(&ctx, line))?;
                    }
                    stack.replace(ctx, out);
                }
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    change::install_changes(ctx, fs);
    paths::install_paths(ctx, fs);
    morf.set_field(ctx, "fs", fs);
}
