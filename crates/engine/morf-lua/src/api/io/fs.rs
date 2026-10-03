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

/// The most a single read or write moves, in bytes.
const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// What a read returns without being told otherwise.
const DEFAULT_READ: u64 = 16 * 1024 * 1024;

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
    let is_dir =
        entry.kind == ops::EntryKind::Dir || entry.target_kind == Some(ops::EntryKind::Dir);
    let is_file =
        entry.kind == ops::EntryKind::File || entry.target_kind == Some(ops::EntryKind::File);
    table.set_field(ctx, "is_dir", is_dir);
    table.set_field(ctx, "is_file", is_file);
    let extension = entry
        .path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    table.set_field(ctx, "extension", extension);
    table
}

/// `nil, message` for an I/O failure.
macro_rules! fail {
    ($stack:ident, $ctx:ident, $error:expr) => {{
        $stack.replace($ctx, (LuaValue::Nil, $error.to_string()));
        return Ok(CallbackReturn::Return);
    }};
}

/// The most files one `fs.read_async` reads.
const MAX_ASYNC_FILES: usize = 256;

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
                LuaValue::Integer(limit) => u64::try_from(limit)
                    .ok()
                    .filter(|limit| *limit <= MAX_BYTES)
                    .ok_or_else(|| {
                        HostError(format!("fs.read_async limit must be 0..{MAX_BYTES}"))
                    })?,
                _ => return Err(HostError("fs.read_async limit must be an integer".into()).into()),
            };
            let job = crate::image_jobs::ImageJob::ReadFiles { paths, limit };
            let submitted = state
                .borrow_mut()
                .image_jobs
                .submit(job, Some(ctx.stash(callback)));
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
                depth: usize::try_from(depth.clamp(0, ops::MAX_DEPTH as i64)).unwrap_or(0),
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
        ("exists", 0u8),
        ("is_dir", 1),
        ("is_file", 2),
        ("is_link", 3),
    ] {
        fs.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let path: LuaValue = stack.consume(ctx)?;
                let path = path_of(path, name)?;
                let answer = match test {
                    0 => std::fs::symlink_metadata(&path).is_ok(),
                    1 => path.is_dir(),
                    2 => path.is_file(),
                    _ => path.is_symlink(),
                };
                stack.replace(ctx, answer);
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
                let offset = integer(ctx, Some(window), "offset")?.unwrap_or(0);
                let length = integer(ctx, Some(window), "length")?.unwrap_or(DEFAULT_READ as i64);
                let offset = u64::try_from(offset)
                    .map_err(|_| HostError("fs.read offset must not be negative".into()))?;
                let length = u64::try_from(length)
                    .ok()
                    .filter(|length| *length <= MAX_BYTES)
                    .ok_or_else(|| HostError(format!("fs.read length must be 0..{MAX_BYTES}")))?;
                match ops::read_range(&path, offset, length) {
                    Ok(bytes) => stack.replace(ctx, luna::String::from_slice(&ctx, bytes)),
                    Err(error) => fail!(stack, ctx, error),
                }
                return Ok(CallbackReturn::Return);
            }
            let limit = match limit {
                LuaValue::Nil => DEFAULT_READ,
                LuaValue::Integer(limit) => u64::try_from(limit)
                    .ok()
                    .filter(|limit| *limit <= MAX_BYTES)
                    .ok_or_else(|| HostError(format!("fs.read limit must be 0..{MAX_BYTES}")))?,
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
                    // A final newline ends the last line; it does not start
                    // an empty one.
                    let body = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
                    let lines = if bytes.is_empty() {
                        Vec::new()
                    } else {
                        body.split(|byte| *byte == b'\n').collect::<Vec<_>>()
                    };
                    for (index, line) in lines.into_iter().enumerate().take(most) {
                        let line = line.strip_suffix(b"\r").unwrap_or(line);
                        out.set(ctx, index as i64 + 1, luna::String::from_slice(&ctx, line))?;
                    }
                    stack.replace(ctx, out);
                }
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    let write = |append: bool| {
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (path, data, opts): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let what = if append { "fs.append" } else { "fs.write" };
            let path = path_of(path, what)?;
            let data = bytes_of(data, what)?;
            if data.len() as u64 > MAX_BYTES {
                return Err(HostError(format!("{what} exceeds {MAX_BYTES} bytes")).into());
            }
            let opts = options(opts, what)?;
            let mode = integer(ctx, opts, "mode")?
                .map(|mode| {
                    u32::try_from(mode)
                        .ok()
                        .filter(|mode| *mode <= 0o7777)
                        .ok_or_else(|| HostError("fs mode must be 0..0o7777".into()))
                })
                .transpose()?;
            let write_options = ops::WriteOptions {
                append: append || flag(ctx, opts, "append", false)?,
                atomic: flag(ctx, opts, "atomic", !append)?,
                parents: flag(ctx, opts, "parents", true)?,
                mode,
            };
            match ops::write(&path, data, write_options) {
                Ok(()) => stack.replace(ctx, true),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        })
    };
    fs.set_field(ctx, "write", write(false));
    fs.set_field(ctx, "append", write(true));

    fs.set_field(
        ctx,
        "mkdir",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (path, opts): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let path = path_of(path, "fs.mkdir")?;
            let opts = options(opts, "fs.mkdir")?;
            match ops::mkdir(&path, flag(ctx, opts, "parents", true)?) {
                Ok(()) => stack.replace(ctx, true),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "remove",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (path, opts): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let path = path_of(path, "fs.remove")?;
            let opts = options(opts, "fs.remove")?;
            match ops::remove(&path, flag(ctx, opts, "recursive", false)?) {
                Ok(()) => stack.replace(ctx, true),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "rename",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (from, to): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let from = path_of(from, "fs.rename source")?;
            let to = path_of(to, "fs.rename target")?;
            match ops::rename(&from, &to) {
                Ok(()) => stack.replace(ctx, true),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "copy",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (from, to, opts): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let from = path_of(from, "fs.copy source")?;
            let to = path_of(to, "fs.copy target")?;
            let opts = options(opts, "fs.copy")?;
            match ops::copy(&from, &to, flag(ctx, opts, "recursive", false)?) {
                Ok(bytes) => stack.replace(ctx, bytes as i64),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "symlink",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (target, link): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let target = path_of(target, "fs.symlink target")?;
            let link = path_of(link, "fs.symlink link")?;
            match ops::symlink(&target, &link) {
                Ok(()) => stack.replace(ctx, true),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    for (name, realpath) in [("read_link", false), ("realpath", true)] {
        fs.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let path: LuaValue = stack.consume(ctx)?;
                let path = path_of(path, name)?;
                let result = if realpath {
                    ops::realpath(&path)
                } else {
                    ops::read_link(&path)
                };
                match result {
                    Ok(found) => stack.replace(ctx, text(&found)),
                    Err(error) => fail!(stack, ctx, error),
                }
                Ok(CallbackReturn::Return)
            }),
        );
    }

    fs.set_field(
        ctx,
        "glob",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let pattern: String = stack.consume(ctx)?;
            if pattern.is_empty() || pattern.len() > 4096 {
                return Err(HostError("fs.glob pattern must be 1..4096 bytes".into()).into());
            }
            let (found, truncated) = ops::glob(&pattern);
            let out = Table::new(&ctx);
            for (index, path) in found.iter().enumerate() {
                out.set(ctx, index as i64 + 1, text(path))?;
            }
            stack.replace(ctx, (out, truncated));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "matches",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (pattern, name): (String, String) = stack.consume(ctx)?;
            stack.replace(ctx, ops::matches(&pattern, &name));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "expand",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let path: String = stack.consume(ctx)?;
            stack.replace(ctx, ops::expand(&path));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "normalize",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let path: String = stack.consume(ctx)?;
            stack.replace(ctx, text(&ops::normalize(Path::new(&path))));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "join",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let mut joined = PathBuf::new();
            for index in 0..stack.len() {
                match stack.get(index) {
                    LuaValue::String(part) => joined.push(OsStr::from_bytes(part.as_bytes())),
                    LuaValue::Integer(part) => joined.push(part.to_string()),
                    _ => return Err(HostError("fs.join takes strings".into()).into()),
                }
            }
            stack.replace(ctx, text(&joined));
            Ok(CallbackReturn::Return)
        }),
    );

    for (name, part) in [
        ("basename", 0u8),
        ("dirname", 1),
        ("extension", 2),
        ("stem", 3),
    ] {
        fs.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let path: String = stack.consume(ctx)?;
                let path = Path::new(&path);
                let answer = match part {
                    0 => path.file_name().map(|s| s.to_string_lossy().into_owned()),
                    1 => path.parent().map(|p| {
                        if p.as_os_str().is_empty() {
                            ".".to_owned()
                        } else {
                            text(p)
                        }
                    }),
                    2 => path.extension().map(|s| s.to_string_lossy().into_owned()),
                    _ => path.file_stem().map(|s| s.to_string_lossy().into_owned()),
                };
                stack.replace(ctx, answer.unwrap_or_default());
                Ok(CallbackReturn::Return)
            }),
        );
    }

    fs.set_field(
        ctx,
        "home",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            stack.replace(ctx, ops::home_dir().map(|home| text(&home)));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "dir",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let kind: String = stack.consume(ctx)?;
            stack.replace(ctx, ops::user_dir(&kind).map(|dir| text(&dir)));
            Ok(CallbackReturn::Return)
        }),
    );

    fs.set_field(
        ctx,
        "disk",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let path: LuaValue = stack.consume(ctx)?;
            let path = path_of(path, "fs.disk")?;
            match ops::disk_usage(&path) {
                Ok((total, free, available)) => {
                    let out = Table::new(&ctx);
                    out.set_field(ctx, "total", total as i64);
                    out.set_field(ctx, "free", free as i64);
                    out.set_field(ctx, "available", available as i64);
                    out.set_field(ctx, "used", total.saturating_sub(free) as i64);
                    stack.replace(ctx, out);
                }
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    morf.set_field(ctx, "fs", fs);
}
