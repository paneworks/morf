//! `morf.fs` calls that change the filesystem: write and append, mkdir,
//! remove, rename, copy, symlink, and reading links back.

use super::*;
use crate::runtime::state::{ReactiveState, StateCell};
use crate::vm::permissions::gated_write;

/// Installs the changing calls on `fs`.
pub(super) fn install_changes<'gc>(
    ctx: Context<'gc>,
    fs: Table<'gc>,
    state: &std::rc::Rc<StateCell<ReactiveState>>,
) {
    let write = gated_write(
        ctx,
        state,
        "morf.fs.write",
        &[0],
        move |ctx, _, mut stack| {
            let (path, data, opts): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let what = "fs.write";
            let path = path_of(path, what)?;
            let data = bytes_of(data, what)?;
            ops::check_write(data, what).map_err(HostError::from)?;
            let opts = options(opts, what)?;
            let mode = integer(ctx, opts, "mode")?
                .map(ops::file_mode)
                .transpose()
                .map_err(HostError::from)?;
            let append = flag(ctx, opts, "append", false)?;
            let write_options = ops::WriteOptions {
                append,
                atomic: flag(ctx, opts, "atomic", !append)?,
                parents: flag(ctx, opts, "parents", true)?,
                mode,
            };
            match ops::write(&path, data, write_options) {
                Ok(()) => stack.replace(ctx, true),
                Err(error) => fail!(stack, ctx, error),
            }
            Ok(CallbackReturn::Return)
        },
    );
    fs.set_field(ctx, "write", write);

    fs.set_field(
        ctx,
        "mkdir",
        gated_write(ctx, state, "morf.fs.mkdir", &[0], |ctx, _, mut stack| {
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
        gated_write(ctx, state, "morf.fs.remove", &[0], |ctx, _, mut stack| {
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
        gated_write(
            ctx,
            state,
            "morf.fs.rename",
            &[0, 1],
            |ctx, _, mut stack| {
                let (from, to): (LuaValue, LuaValue) = stack.consume(ctx)?;
                let from = path_of(from, "fs.rename source")?;
                let to = path_of(to, "fs.rename target")?;
                match ops::rename(&from, &to) {
                    Ok(()) => stack.replace(ctx, true),
                    Err(error) => fail!(stack, ctx, error),
                }
                Ok(CallbackReturn::Return)
            },
        ),
    );

    fs.set_field(
        ctx,
        "copy",
        gated_write(ctx, state, "morf.fs.copy", &[1], |ctx, _, mut stack| {
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
        gated_write(ctx, state, "morf.fs.symlink", &[1], |ctx, _, mut stack| {
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

    {
        let name = "read_link";
        fs.set_field(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let path: LuaValue = stack.consume(ctx)?;
                let path = path_of(path, name)?;
                let result = ops::read_link(&path);
                match result {
                    Ok(found) => stack.replace(ctx, text(&found)),
                    Err(error) => fail!(stack, ctx, error),
                }
                Ok(CallbackReturn::Return)
            }),
        );
    }
}
