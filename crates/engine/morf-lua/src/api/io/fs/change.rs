//! `morf.fs` calls that change the filesystem: write and append, mkdir,
//! remove, rename, copy, symlink, and reading links back.

use super::*;

/// Installs the changing calls on `fs`.
pub(super) fn install_changes<'gc>(ctx: Context<'gc>, fs: Table<'gc>) {
    let write = |append: bool| {
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (path, data, opts): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let what = if append { "fs.append" } else { "fs.write" };
            let path = path_of(path, what)?;
            let data = bytes_of(data, what)?;
            ops::check_write(data, what).map_err(HostError)?;
            let opts = options(opts, what)?;
            let mode = integer(ctx, opts, "mode")?
                .map(ops::file_mode)
                .transpose()
                .map_err(HostError)?;
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
}
