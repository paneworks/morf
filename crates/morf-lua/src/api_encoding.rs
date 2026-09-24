//! `morf.encoding`: bytes to text and back, and names for content.
//!
//! Every function takes and returns Lua strings, which are bytes; nothing
//! here assumes UTF-8. Decoding refuses malformed input by returning
//! `nil, message`, since the input usually came from outside.

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_io::{archive, codec};

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

/// Largest output a decompression may produce unless `max_size` says less.
const DEFAULT_MAX_OUTPUT: usize = 64 * 1024 * 1024;
/// Largest output a caller may ask a decompression for.
const MAX_OUTPUT: usize = 512 * 1024 * 1024;
const DEFAULT_MAX_ENTRIES: usize = 100_000;
const MAX_ENTRIES: usize = 1_000_000;

fn count_option<'gc>(
    ctx: Context<'gc>,
    options: LuaValue<'gc>,
    field: &str,
    default: usize,
    most: usize,
) -> Result<usize, HostError> {
    let LuaValue::Table(table) = options else {
        return Ok(default);
    };
    match table.get_value(ctx, field) {
        LuaValue::Nil => Ok(default),
        LuaValue::Integer(value) if value > 0 && value as u64 <= most as u64 => Ok(value as usize),
        LuaValue::Number(value) if value >= 1.0 && value <= most as f64 => Ok(value as usize),
        _ => Err(HostError(format!("{field} must be 1..{most}"))),
    }
}

/// `format` as given (`nil` or `"auto"` detects), then the bytes inflated.
fn inflate(input: &[u8], format: Option<&str>, max_output: usize) -> Result<Vec<u8>, String> {
    let format = match format {
        None | Some("auto") => archive::detect(input).ok_or_else(|| {
            "unknown compression format (give one: gzip, zlib, deflate, zstd, xz, lzma)".to_string()
        })?,
        Some(name) => archive::Compression::parse(name)
            .ok_or_else(|| format!("unknown compression format {name:?}"))?,
    };
    archive::decompress(input, format, max_output)
}

fn format_name(value: LuaValue<'_>) -> Result<Option<String>, HostError> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::String(name) => Ok(Some(String::from_utf8_lossy(name.as_bytes()).into_owned())),
        _ => Err(HostError("format must be a string".into())),
    }
}

/// A tar archive's bytes: inflated first when a magic number says so.
fn tar_bytes(input: &[u8], max_output: usize) -> Result<std::borrow::Cow<'_, [u8]>, String> {
    match archive::detect(input) {
        Some(format) => archive::decompress(input, format, max_output).map(std::borrow::Cow::Owned),
        None => Ok(std::borrow::Cow::Borrowed(input)),
    }
}

fn entry_table<'gc>(
    ctx: Context<'gc>,
    entry: &archive::TarEntry,
    data: Option<&[u8]>,
) -> Table<'gc> {
    let row = Table::new(&ctx);
    row.set_field(
        ctx,
        "name",
        luna::String::from_slice(&ctx, entry.name.as_bytes()),
    );
    row.set_field(ctx, "type", entry.kind);
    row.set_field(ctx, "size", entry.size as i64);
    row.set_field(ctx, "mode", i64::from(entry.mode));
    row.set_field(ctx, "mtime", entry.mtime);
    if !entry.link.is_empty() {
        row.set_field(
            ctx,
            "link",
            luna::String::from_slice(&ctx, entry.link.as_bytes()),
        );
    }
    if let Some(data) = data {
        row.set_field(ctx, "data", luna::String::from_slice(&ctx, data));
    }
    row
}

pub(crate) fn install_archive_api<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
    let module = Table::new(&ctx);

    // tar(bytes, { contents = false, max_size, max_entries }): every member,
    // `data` included when `contents` is true. A gzip, zstd or xz archive is
    // inflated first.
    module.set_field(
        ctx,
        "tar",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
            let input = bytes(input, "archive.tar")?;
            let contents = flag(ctx, options, "contents", false)?;
            let max_output =
                count_option(ctx, options, "max_size", DEFAULT_MAX_OUTPUT, MAX_OUTPUT)?;
            let max_entries = count_option(
                ctx,
                options,
                "max_entries",
                DEFAULT_MAX_ENTRIES,
                MAX_ENTRIES,
            )?;
            let listed = tar_bytes(input, max_output).and_then(|tar| {
                archive::tar_entries(&tar, max_entries).map(|entries| (tar, entries))
            });
            match listed {
                Ok((tar, entries)) => {
                    let list = Table::new(&ctx);
                    for (index, entry) in entries.iter().enumerate() {
                        let data = contents.then(|| &tar[entry.data.clone()]);
                        list.set(ctx, index as i64 + 1, entry_table(ctx, entry, data))?;
                    }
                    stack.replace(ctx, list);
                }
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    // tar_read(bytes, name, options): one member's bytes, or nil and why.
    module.set_field(
        ctx,
        "tar_read",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, name, options): (LuaValue, luna::String, LuaValue) = stack.consume(ctx)?;
            let input = bytes(input, "archive.tar_read")?;
            let max_output =
                count_option(ctx, options, "max_size", DEFAULT_MAX_OUTPUT, MAX_OUTPUT)?;
            let max_entries = count_option(
                ctx,
                options,
                "max_entries",
                DEFAULT_MAX_ENTRIES,
                MAX_ENTRIES,
            )?;
            let wanted = String::from_utf8_lossy(name.as_bytes()).into_owned();
            let found = tar_bytes(input, max_output).and_then(|tar| {
                let entries = archive::tar_entries(&tar, max_entries)?;
                let entry = entries
                    .iter()
                    .find(|entry| entry.name == wanted && entry.kind == "file")
                    .ok_or_else(|| format!("{wanted}: no such file in the archive"))?;
                Ok(tar[entry.data.clone()].to_vec())
            });
            match found {
                Ok(data) => stack.replace(ctx, luna::String::from_slice(&ctx, data)),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    morf.set_field(ctx, "archive", module);
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

    // decompress(bytes, format, { max_size }): gzip, zlib, deflate, zstd, xz
    // or lzma; `format` nil or "auto" goes by the magic number.
    encoding.set_field(
        ctx,
        "decompress",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (input, format, options): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let input = bytes(input, "decompress")?;
            let format = format_name(format)?;
            let max_output =
                count_option(ctx, options, "max_size", DEFAULT_MAX_OUTPUT, MAX_OUTPUT)?;
            match inflate(input, format.as_deref(), max_output) {
                Ok(out) => stack.replace(ctx, luna::String::from_slice(&ctx, out)),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );
    // compression(bytes): the format a magic number names, or nil.
    encoding.set_field(
        ctx,
        "compression",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let input: LuaValue = stack.consume(ctx)?;
            let input = bytes(input, "compression")?;
            match archive::detect(input) {
                Some(format) => stack.replace(ctx, format.name()),
                None => stack.replace(ctx, LuaValue::Nil),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    morf.set_field(ctx, "encoding", encoding);
}
