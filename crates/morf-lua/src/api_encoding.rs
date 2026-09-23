//! `morf.encoding`: bytes to text and back, and names for content.
//!
//! Every function takes and returns Lua strings, which are bytes; nothing
//! here assumes UTF-8. Decoding refuses malformed input by returning
//! `nil, message`, since the input usually came from outside.

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_io::codec;

use crate::scene_bindings::*;

const MAX_INPUT: usize = 64 * 1024 * 1024;

fn bytes<'gc>(value: LuaValue<'gc>, what: &str) -> Result<&'gc [u8], HostError> {
    match value {
        LuaValue::String(text) if text.as_bytes().len() <= MAX_INPUT => Ok(text.as_bytes()),
        LuaValue::String(_) => Err(HostError(format!("{what} input exceeds {MAX_INPUT} bytes"))),
        _ => Err(HostError(format!("{what} takes a string"))),
    }
}

fn flag<'gc>(
    ctx: Context<'gc>,
    options: LuaValue<'gc>,
    field: &str,
    default: bool,
) -> Result<bool, HostError> {
    match options {
        LuaValue::Nil => Ok(default),
        LuaValue::Table(table) => match table.get_value(ctx, field) {
            LuaValue::Nil => Ok(default),
            LuaValue::Boolean(value) => Ok(value),
            _ => Err(HostError(format!(
                "encoding option {field} must be boolean"
            ))),
        },
        _ => Err(HostError("encoding options must be a table".into())),
    }
}

pub(crate) fn install_encoding_api<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
    let encoding = Table::new(&ctx);

    // base64_encode(bytes, { url = false, pad = true })
    encoding.set_field(
        ctx,
        "base64_encode",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let input = bytes(input, "base64_encode")?;
            let url = flag(ctx, options, "url", false)?;
            let pad = flag(ctx, options, "pad", !url)?;
            stack.replace(ctx, codec::base64_encode(input, url, pad));
            Ok(CallbackReturn::Return)
        }),
    );
    encoding.set_field(
        ctx,
        "base64_decode",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let input: LuaValue = stack.consume(ctx)?;
            match codec::base64_decode(bytes(input, "base64_decode")?) {
                Ok(out) => stack.replace(ctx, luna::String::from_slice(&ctx, out)),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    // hex_encode(bytes, { upper = false })
    encoding.set_field(
        ctx,
        "hex_encode",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let input = bytes(input, "hex_encode")?;
            stack.replace(
                ctx,
                codec::hex_encode(input, flag(ctx, options, "upper", false)?),
            );
            Ok(CallbackReturn::Return)
        }),
    );
    encoding.set_field(
        ctx,
        "hex_decode",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let input: LuaValue = stack.consume(ctx)?;
            match codec::hex_decode(bytes(input, "hex_decode")?) {
                Ok(out) => stack.replace(ctx, luna::String::from_slice(&ctx, out)),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    // url_encode(text, { component = true, plus = false }): a query value
    // by default; `component = false` keeps a path's `/` and `:`.
    encoding.set_field(
        ctx,
        "url_encode",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let input = bytes(input, "url_encode")?;
            let component = flag(ctx, options, "component", true)?;
            let plus = flag(ctx, options, "plus", false)?;
            stack.replace(ctx, codec::url_encode(input, component, plus));
            Ok(CallbackReturn::Return)
        }),
    );
    encoding.set_field(
        ctx,
        "url_decode",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let input = bytes(input, "url_decode")?;
            let out = codec::url_decode(input, flag(ctx, options, "plus", false)?);
            stack.replace(ctx, luna::String::from_slice(&ctx, out));
            Ok(CallbackReturn::Return)
        }),
    );

    // sha256(bytes, raw) / sha1(bytes, raw): lower-case hex, or the raw
    // digest when `raw` is true.
    encoding.set_field(
        ctx,
        "sha256",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, raw): (LuaValue, Option<bool>) = stack.consume(ctx)?;
            let out = codec::sha256(bytes(input, "sha256")?);
            if raw.unwrap_or(false) {
                stack.replace(ctx, luna::String::from_slice(&ctx, out));
            } else {
                stack.replace(ctx, codec::hex_encode(&out, false));
            }
            Ok(CallbackReturn::Return)
        }),
    );
    encoding.set_field(
        ctx,
        "sha1",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, raw): (LuaValue, Option<bool>) = stack.consume(ctx)?;
            let out = codec::sha1(bytes(input, "sha1")?);
            if raw.unwrap_or(false) {
                stack.replace(ctx, luna::String::from_slice(&ctx, out));
            } else {
                stack.replace(ctx, codec::hex_encode(&out, false));
            }
            Ok(CallbackReturn::Return)
        }),
    );
    encoding.set_field(
        ctx,
        "crc32",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let input: LuaValue = stack.consume(ctx)?;
            stack.replace(ctx, i64::from(codec::crc32(bytes(input, "crc32")?)));
            Ok(CallbackReturn::Return)
        }),
    );

    encoding.set_field(
        ctx,
        "random_bytes",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let count: i64 = stack.consume(ctx)?;
            let count = usize::try_from(count)
                .ok()
                .filter(|count| *count <= 1024 * 1024)
                .ok_or_else(|| HostError("random_bytes count must be 0..1048576".into()))?;
            let out = codec::random_bytes(count).map_err(|error| HostError(error.to_string()))?;
            stack.replace(ctx, luna::String::from_slice(&ctx, out));
            Ok(CallbackReturn::Return)
        }),
    );
    encoding.set_field(
        ctx,
        "uuid",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let id = codec::uuid_v4().map_err(|error| HostError(error.to_string()))?;
            stack.replace(ctx, id);
            Ok(CallbackReturn::Return)
        }),
    );

    morf.set_field(ctx, "encoding", encoding);
}
