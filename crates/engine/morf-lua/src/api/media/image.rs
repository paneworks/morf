use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_image::IconResolver;
use std::process::{Command as StdCommand, Stdio};

use crate::{
    api::ui::scene::*, runtime::state::*, value::table::*, vm::permissions::gated, vm::types::*,
};

pub(crate) fn install_image_api<'gc>(
    ctx: Context<'gc>,
    state: std::rc::Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let icon_path = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (name, theme, size): (String, Option<String>, Option<i64>) = stack.consume(ctx)?;
        let (theme, size) = icon_lookup_options(&name, theme, size).map_err(HostError)?;
        match IconResolver::from_environment().find(&name, &theme, size) {
            Ok(path) => stack.replace(ctx, path.to_string_lossy().as_ref()),
            Err(_) => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    crate::api::namespace::table(ctx, morf, "image").set_field(ctx, "icon", icon_path);
    let has_icon = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (name, theme, size): (String, Option<String>, Option<i64>) = stack.consume(ctx)?;
        let (theme, size) = icon_lookup_options(&name, theme, size).map_err(HostError)?;
        stack.replace(
            ctx,
            IconResolver::from_environment()
                .find(&name, &theme, size)
                .is_ok(),
        );
        Ok(CallbackReturn::Return)
    });
    crate::api::namespace::table(ctx, morf, "image").set_field(ctx, "has_icon", has_icon);
    let exec_detached = gated(
        ctx,
        &state,
        "run",
        "morf.process.detach",
        move |ctx, _, mut stack| {
            let command: Table = stack.consume(ctx)?;
            let mut command = table_string_array(ctx, command, 64).map_err(HostError)?;
            if command.is_empty() {
                return Err(HostError("detached command cannot be empty".into()).into());
            }
            let program = command.remove(0);
            let mut child = StdCommand::new(program)
                .args(command)
                // What is started detached outlives the moment and is often an
                // application; it must not inherit a wrapper's library path.
                .env_remove("LD_LIBRARY_PATH")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| HostError(error.to_string()))?;
            let id = child.id();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            stack.replace(ctx, i64::from(id));
            Ok(CallbackReturn::Return)
        },
    );
    crate::api::namespace::table(ctx, morf, "process").set_field(ctx, "detach", exec_detached);
}
