//! Handlers for the system's services: D-Bus signals, replies and calls,
//! greetd and PAM conversations, and udev and status notifier values.

use super::*;

pub(crate) fn execute_dbus_handler(
    ctx: Context<'_>,
    closure: &Handler,
    value: DbusValue,
    limits: Limits,
) -> Result<(), String> {
    let argument = dbus_value_to_lua(ctx, value)?;
    let executor = Executor::start(
        ctx,
        ctx.fetch(&stashed(closure)).into(),
        Variadic(vec![argument]),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Hands one signal to a subscription's handler.
///
/// A plain subscription is called `(body, info)`: the body first, exactly as
/// it has always been passed, so every handler written before `info` existed
/// still works; `info` is `{ sender, path, interface, member, arguments }`,
/// and `sender` — the unique name — is what tells two services emitting on
/// the same path apart. A name-owner subscription is called
/// `(old_owner, new_owner, name)`, with `""` for nobody.
pub(crate) fn execute_dbus_signal_handler(
    ctx: Context<'_>,
    closure: &Handler,
    event: morf_io::DbusSignalEvent,
    kind: DbusSignalKind,
    limits: Limits,
) -> Result<(), String> {
    let arguments = event.arguments?;
    let args = match kind {
        DbusSignalKind::OwnerChanged => {
            let text = |value: Option<&DbusValue>| match value {
                Some(DbusValue::String(text)) => text.clone(),
                _ => String::new(),
            };
            let DbusValue::List(values) = &arguments else {
                return Err("NameOwnerChanged carried no arguments".to_owned());
            };
            let name = text(values.first());
            let old = text(values.get(1));
            let new = text(values.get(2));
            vec![
                LuaValue::String(ctx.intern(old.as_bytes())),
                LuaValue::String(ctx.intern(new.as_bytes())),
                LuaValue::String(ctx.intern(name.as_bytes())),
            ]
        }
        DbusSignalKind::Signal => {
            let body = dbus_value_to_lua(ctx, arguments.clone())?;
            let info = Table::new(&ctx);
            info.set_field(ctx, "sender", event.sender.as_str());
            info.set_field(ctx, "path", event.path.as_str());
            info.set_field(ctx, "interface", event.interface.as_str());
            info.set_field(ctx, "member", event.member.as_str());
            info.set_field(ctx, "arguments", dbus_value_to_lua(ctx, arguments)?);
            vec![body, LuaValue::Table(info)]
        }
    };
    let executor = Executor::start(ctx, ctx.fetch(&stashed(closure)).into(), Variadic(args));
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Hands a `call_async` answer to its callback: `(true, reply)` or
/// `(false, error)`.
pub(crate) fn execute_dbus_reply_handler(
    ctx: Context<'_>,
    closure: &Handler,
    reply: Result<DbusValue, String>,
    limits: Limits,
) -> Result<(), String> {
    let args = match reply {
        Ok(value) => vec![LuaValue::Boolean(true), dbus_value_to_lua(ctx, value)?],
        Err(error) => vec![
            LuaValue::Boolean(false),
            LuaValue::String(ctx.intern(error.as_bytes())),
        ],
    };
    let executor = Executor::start(ctx, ctx.fetch(&stashed(closure)).into(), Variadic(args));
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Hands one arriving method call to its Lua handler.
///
/// The call arrives as a table rather than as positional arguments because most
/// handlers dispatch on `member` and ignore the rest, and a handler that has to
/// name five parameters to read the second reads worse than one that does not.
/// `id` is opaque and only meaningful to `service:reply`.
pub(crate) fn execute_dbus_call_handler(
    ctx: Context<'_>,
    closure: &Handler,
    call: DbusCall,
    limits: Limits,
) -> Result<(), String> {
    let table = Table::new(&ctx);
    table.set_field(ctx, "id", call.id as i64);
    table.set_field(ctx, "interface", call.interface.as_str());
    table.set_field(ctx, "member", call.member.as_str());
    table.set_field(ctx, "path", call.path.as_str());
    table.set_field(ctx, "sender", call.sender.as_str());
    table.set_field(ctx, "signature", call.signature.as_str());
    table.set_field(ctx, "arguments", dbus_value_to_lua(ctx, call.arguments)?);
    let executor = Executor::start(
        ctx,
        ctx.fetch(&stashed(closure)).into(),
        Variadic(vec![LuaValue::Table(table)]),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Shows a configuration one thing greetd said.
///
/// Keyed on `kind` like a PAM message: `auth` carries `auth_type` — `secret`,
/// `visible`, `info` or `error` — and `text`; `success` carries nothing;
/// `error` carries `text` and whether it was `authentication` that failed;
/// `failed` carries `text` and means the connection is gone.
pub(crate) fn execute_greetd_handler(
    ctx: Context<'_>,
    closure: &Handler,
    event: GreetdEvent,
    limits: Limits,
) -> Result<(), String> {
    let table = Table::new(&ctx);
    match event {
        GreetdEvent::Response(GreetdResponse::AuthMessage { kind, message }) => {
            table.set_field(ctx, "kind", "auth");
            let auth_type = match kind {
                AuthMessageType::Secret => "secret",
                AuthMessageType::Visible => "visible",
                AuthMessageType::Info => "info",
                AuthMessageType::Error => "error",
            };
            table.set_field(ctx, "auth_type", auth_type);
            table.set_field(ctx, "text", message.as_str());
        }
        GreetdEvent::Response(GreetdResponse::Success) => {
            table.set_field(ctx, "kind", "success");
        }
        GreetdEvent::Response(GreetdResponse::Error {
            authentication,
            description,
        }) => {
            table.set_field(ctx, "kind", "error");
            table.set_field(ctx, "authentication", authentication);
            table.set_field(ctx, "text", description.as_str());
        }
        GreetdEvent::Failed(text) => {
            table.set_field(ctx, "kind", "failed");
            table.set_field(ctx, "text", text.as_str());
        }
    }
    let executor = Executor::start(
        ctx,
        ctx.fetch(&stashed(closure)).into(),
        Variadic(vec![LuaValue::Table(table)]),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Shows a configuration one thing a PAM module said.
///
/// A table rather than positional arguments, keyed on `kind`, because a
/// handler dispatches on what happened and reads the rest: `prompt` carries
/// `text` and `echo`, `info` and `error` carry `text`, and `finished` carries
/// `ok` with `error` and `code` when it is not.
pub(crate) fn execute_pam_session_handler(
    ctx: Context<'_>,
    closure: &Handler,
    event: PamEvent,
    limits: Limits,
) -> Result<(), String> {
    let table = Table::new(&ctx);
    match event {
        PamEvent::Message(PamPrompt::Prompt { text, echo }) => {
            table.set_field(ctx, "kind", "prompt");
            table.set_field(ctx, "text", text.as_str());
            table.set_field(ctx, "echo", echo);
        }
        PamEvent::Message(PamPrompt::Info(text)) => {
            table.set_field(ctx, "kind", "info");
            table.set_field(ctx, "text", text.as_str());
        }
        PamEvent::Message(PamPrompt::Error(text)) => {
            table.set_field(ctx, "kind", "error");
            table.set_field(ctx, "text", text.as_str());
        }
        PamEvent::Finished(verdict) => {
            table.set_field(ctx, "kind", "finished");
            table.set_field(ctx, "ok", verdict.is_ok());
            if let Err(error) = verdict {
                table.set_field(ctx, "error", error.to_string().as_str());
                if let Some(code) = error.code() {
                    table.set_field(ctx, "code", i64::from(code));
                }
            }
        }
    }
    let executor = Executor::start(
        ctx,
        ctx.fetch(&stashed(closure)).into(),
        Variadic(vec![LuaValue::Table(table)]),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn udev_event_value(event: UdevEvent) -> DbusValue {
    let properties = event
        .properties
        .into_iter()
        .map(|(key, value)| (key, DbusValue::String(value)))
        .collect();
    DbusValue::Map(BTreeMap::from([
        ("action".to_owned(), DbusValue::String(event.action)),
        ("devpath".to_owned(), DbusValue::String(event.devpath)),
        (
            "subsystem".to_owned(),
            event.subsystem.map_or(DbusValue::Nil, DbusValue::String),
        ),
        (
            "devname".to_owned(),
            event.devname.map_or(DbusValue::Nil, DbusValue::String),
        ),
        ("properties".to_owned(), DbusValue::Map(properties)),
    ]))
}

pub(crate) fn status_notifier_value(items: Vec<StatusNotifierAddress>) -> DbusValue {
    DbusValue::List(
        items
            .into_iter()
            .map(|item| {
                DbusValue::Map(BTreeMap::from([
                    ("service".to_owned(), DbusValue::String(item.service)),
                    ("path".to_owned(), DbusValue::String(item.path)),
                ]))
            })
            .collect(),
    )
}
