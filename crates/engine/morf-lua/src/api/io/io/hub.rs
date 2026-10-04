//! An owed callback run: what `morf_io::IoHub` collected, handed to Lua.

use super::*;

/// Runs one owed callback, unless its handle was closed meanwhile.
pub(crate) fn execute_io_call(
    ctx: Context<'_>,
    call: &IoCall,
    limits: Limits,
) -> Result<(), String> {
    if !call.live() {
        return Ok(());
    }
    let args = match &call.args {
        CallArgs::None => Vec::new(),
        CallArgs::Bytes(bytes) => vec![LuaValue::String(ctx.intern(bytes))],
        CallArgs::Text(text) => vec![LuaValue::String(ctx.intern(text.as_bytes()))],
        CallArgs::Exit {
            code,
            signal,
            timed_out,
        } => vec![
            optional_int(*code),
            optional_int(*signal),
            LuaValue::Boolean(*timed_out),
        ],
        CallArgs::Reply(Ok(reply)) => vec![LuaValue::String(ctx.intern(reply)), LuaValue::Nil],
        CallArgs::Reply(Err(error)) => {
            vec![
                LuaValue::Nil,
                LuaValue::String(ctx.intern(error.as_bytes())),
            ]
        }
        CallArgs::Run(result) => {
            let table = Table::new(&ctx);
            table.set_field(ctx, "ok", result.ok());
            table.set_field(ctx, "code", optional_int(result.code));
            table.set_field(ctx, "signal", optional_int(result.signal));
            table.set_field(ctx, "stdout", ctx.intern(&result.stdout));
            table.set_field(ctx, "stderr", ctx.intern(&result.stderr));
            table.set_field(ctx, "timed_out", result.timed_out);
            table.set_field(ctx, "truncated", result.truncated);
            if let Some(error) = &result.error {
                table.set_field(ctx, "error", error.as_str());
            }
            vec![LuaValue::Table(table)]
        }
    };
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(&call.callback))
            .into(),
        Variadic(args),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn optional_int<'gc>(value: Option<i32>) -> LuaValue<'gc> {
    value.map_or(LuaValue::Nil, |value| LuaValue::Integer(i64::from(value)))
}
