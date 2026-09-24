//! `morf.image`: reading and editing pictures from a configuration.
//!
//! `info` reads a header and answers at once. Everything that decodes a
//! whole picture — `process`, `palette`, and `pixel` on a large one — runs on
//! the image workers and answers through a callback on the main loop, as
//! `on_done(true, result)` or `on_done(false, message)`, so a wallpaper
//! picker cutting thumbnails does not stall the frames around it.
//!
//! The call itself returns `true` once the work is queued, or `nil, message`
//! when it could not be (the queue is full). A malformed call — an unknown
//! operation, a negative size, a callback that is not a function — raises:
//! no amount of waiting fixes it.
//!
//! Sources are what `ui.Image` takes: a path, a `file://` URI, or the picture
//! written inline as SVG text or a `data:` URI.

use luna::{Callback, CallbackReturn, Closure, Context, Function, Table, Value as LuaValue};
use morf_image::ops::{self, ImageOp, OutputFormat, ProcessRequest, ResizeMode};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::rc::Rc;

use crate::api_color::color_userdata;
use crate::image_jobs::{ImageJob, Region};
use crate::{scene_bindings::HostError, state::ReactiveState};

/// The largest picture `pixel` decodes on the Lua thread, in pixels.
///
/// About four megapixels decodes in a few milliseconds for a PNG and not
/// much more for a JPEG — a frame's worth at worst. Anything larger has to
/// be asked for with a callback, which moves the decode to a worker.
pub(crate) const SYNC_PIXEL_LIMIT: u64 = 4 * 1024 * 1024;
/// The most operations one `process` call may chain.
const MAX_OPS: usize = 32;
/// The most colours `palette` names.
const MAX_PALETTE: i64 = 64;

pub(crate) fn source_of(value: LuaValue<'_>, what: &str) -> Result<PathBuf, HostError> {
    match value {
        LuaValue::String(text) if !text.as_bytes().is_empty() => {
            Ok(PathBuf::from(OsStr::from_bytes(text.as_bytes())))
        }
        LuaValue::String(_) => Err(HostError(format!("{what} is empty"))),
        _ => Err(HostError(format!("{what} must be a string"))),
    }
}

pub(crate) fn uint(value: LuaValue<'_>, what: &str) -> Result<u32, HostError> {
    let number = match value {
        LuaValue::Integer(value) => Some(value),
        LuaValue::Number(value) if value.fract() == 0.0 && value.is_finite() => Some(value as i64),
        _ => None,
    };
    number
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| HostError(format!("{what} must be a non-negative integer")))
}

pub(crate) fn callback_of<'gc>(
    value: LuaValue<'gc>,
    what: &str,
) -> Result<Option<Closure<'gc>>, HostError> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::Function(Function::Closure(closure)) => Ok(Some(closure)),
        _ => Err(HostError(format!("{what} must be a Lua function"))),
    }
}

pub(crate) fn quality_of(value: LuaValue<'_>) -> Result<u8, HostError> {
    match value {
        LuaValue::Nil => Ok(90),
        value => uint(value, "quality")
            .ok()
            .and_then(|quality| u8::try_from(quality).ok())
            .filter(|quality| (1..=100).contains(quality))
            .ok_or_else(|| HostError("quality must be 1..100".into())),
    }
}

/// The format asked for, or the one the output's extension names.
pub(crate) fn format_of(
    value: LuaValue<'_>,
    output: &std::path::Path,
) -> Result<OutputFormat, HostError> {
    match value {
        LuaValue::Nil => OutputFormat::from_path(output).ok_or_else(|| {
            HostError(format!(
                "cannot tell a format from `{}`; say format = \"png\", \"jpeg\" or \"webp\"",
                output.display()
            ))
        }),
        LuaValue::String(name) => OutputFormat::parse(&name.display_lossy().to_string())
            .ok_or_else(|| HostError("format must be \"png\", \"jpeg\" or \"webp\"".into())),
        _ => Err(HostError("format must be a string".into())),
    }
}

pub(crate) fn region_of<'gc>(
    ctx: Context<'gc>,
    value: LuaValue<'gc>,
) -> Result<Option<Region>, HostError> {
    let table = match value {
        LuaValue::Nil => return Ok(None),
        LuaValue::Table(table) => table,
        _ => return Err(HostError("region must be a table {x, y, w, h}".into())),
    };
    // Positional or named, whichever reads better where it is written.
    let field = |index: i64, name: &str, long: &str| {
        let value = table.get_value(ctx, index);
        let value = if matches!(value, LuaValue::Nil) {
            table.get_value(ctx, name)
        } else {
            value
        };
        if matches!(value, LuaValue::Nil) {
            table.get_value(ctx, long)
        } else {
            value
        }
    };
    let region = Region {
        x: uint(field(1, "x", "x"), "region x")?,
        y: uint(field(2, "y", "y"), "region y")?,
        width: uint(field(3, "w", "width"), "region width")?,
        height: uint(field(4, "h", "height"), "region height")?,
    };
    if region.width == 0 || region.height == 0 {
        return Err(HostError("region must have a width and a height".into()));
    }
    Ok(Some(region))
}

fn parse_op<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<ImageOp, HostError> {
    let LuaValue::Table(op) = value else {
        return Err(HostError(
            "each image op must be a table like {\"resize\", 64, 64}".into(),
        ));
    };
    let name = match op.get_value(ctx, 1i64) {
        LuaValue::String(name) => name.display_lossy().to_string(),
        _ => return Err(HostError("an image op starts with its name".into())),
    };
    let arg = |index: i64| op.get_value(ctx, index);
    Ok(match name.as_str() {
        "crop" => ImageOp::Crop {
            x: uint(arg(2), "crop x")?,
            y: uint(arg(3), "crop y")?,
            width: uint(arg(4), "crop width")?,
            height: uint(arg(5), "crop height")?,
        },
        "square" => ImageOp::Square,
        "resize" => ImageOp::Resize {
            width: uint(arg(2), "resize width")?,
            height: uint(arg(3), "resize height")?,
            mode: match arg(4) {
                LuaValue::Nil => ResizeMode::Fit,
                LuaValue::String(mode) => match mode.display_lossy().to_string().as_str() {
                    "fit" => ResizeMode::Fit,
                    "fill" => ResizeMode::Fill,
                    "exact" => ResizeMode::Exact,
                    other => {
                        return Err(HostError(format!(
                            "resize mode must be fit, fill or exact, not `{other}`"
                        )));
                    }
                },
                _ => return Err(HostError("resize mode must be a string".into())),
            },
        },
        "rotate" => match uint(arg(2), "rotate degrees")? {
            degrees @ (90 | 180 | 270) => ImageOp::Rotate(degrees as u16),
            other => {
                return Err(HostError(format!(
                    "rotate takes 90, 180 or 270, not {other}"
                )));
            }
        },
        "flip" => match arg(2) {
            LuaValue::String(axis) => match axis.display_lossy().to_string().as_str() {
                "h" | "horizontal" => ImageOp::Flip { horizontal: true },
                "v" | "vertical" => ImageOp::Flip { horizontal: false },
                _ => return Err(HostError("flip takes \"h\" or \"v\"".into())),
            },
            _ => return Err(HostError("flip takes \"h\" or \"v\"".into())),
        },
        "blur" => {
            let sigma = match arg(2) {
                LuaValue::Integer(value) => value as f64,
                LuaValue::Number(value) => value,
                _ => return Err(HostError("blur takes a sigma".into())),
            };
            if !(sigma > 0.0 && sigma <= 100.0) {
                return Err(HostError(
                    "blur sigma must be over 0 and at most 100".into(),
                ));
            }
            ImageOp::Blur(sigma as f32)
        }
        "grayscale" | "greyscale" => ImageOp::Grayscale,
        other => return Err(HostError(format!("unknown image op `{other}`"))),
    })
}

fn parse_ops<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Vec<ImageOp>, HostError> {
    let table = match value {
        LuaValue::Nil => return Ok(Vec::new()),
        LuaValue::Table(table) => table,
        _ => return Err(HostError("ops must be a list of operations".into())),
    };
    let mut ops = Vec::new();
    for index in 1..=(MAX_OPS as i64 + 1) {
        let value = table.get_value(ctx, index);
        if matches!(value, LuaValue::Nil) {
            return Ok(ops);
        }
        if ops.len() == MAX_OPS {
            break;
        }
        ops.push(parse_op(ctx, value)?);
    }
    Err(HostError(format!("at most {MAX_OPS} image ops per call")))
}

/// Queues a job, answering the call with `true` or `nil, message`.
fn queue<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    job: ImageJob,
    callback: Option<Closure<'gc>>,
) -> (LuaValue<'gc>, LuaValue<'gc>) {
    let callback = callback.map(|callback| ctx.stash(callback));
    match state.borrow_mut().image_jobs.submit(job, callback) {
        Ok(()) => (LuaValue::Boolean(true), LuaValue::Nil),
        Err(message) => (
            LuaValue::Nil,
            LuaValue::String(ctx.intern(message.as_bytes())),
        ),
    }
}

pub(crate) fn install_image_ops_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let image = Table::new(&ctx);

    image.set_field(
        ctx,
        "info",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let source: LuaValue = stack.consume(ctx)?;
            let source = source_of(source, "image.info source")?;
            match ops::image_info(&source) {
                Ok(info) => {
                    let table = Table::new(&ctx);
                    table.set_field(ctx, "width", i64::from(info.width));
                    table.set_field(ctx, "height", i64::from(info.height));
                    table.set_field(ctx, "format", info.format.as_str());
                    stack.replace(ctx, table);
                }
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error.to_string())),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    let process_state = Rc::clone(&state);
    image.set_field(
        ctx,
        "process",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let options: LuaValue = stack.consume(ctx)?;
            let LuaValue::Table(options) = options else {
                return Err(HostError("image.process takes a table of options".into()).into());
            };
            let source = source_of(options.get_value(ctx, "source"), "image.process source")?;
            let output = source_of(options.get_value(ctx, "output"), "image.process output")?;
            let request = ProcessRequest {
                ops: parse_ops(ctx, options.get_value(ctx, "ops"))?,
                format: format_of(options.get_value(ctx, "format"), &output)?,
                quality: quality_of(options.get_value(ctx, "quality"))?,
                source,
                output,
            };
            let callback = callback_of(options.get_value(ctx, "on_done"), "on_done")?;
            stack.replace(
                ctx,
                queue(ctx, &process_state, ImageJob::Process(request), callback),
            );
            Ok(CallbackReturn::Return)
        }),
    );

    // Synchronous for a small picture, because reading one colour out of an
    // icon is the kind of thing a binding wants answered on the spot. Given a
    // callback it goes to a worker instead, and then any size within the
    // bounds is fine.
    let pixel_state = Rc::clone(&state);
    image.set_field(
        ctx,
        "pixel",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (source, x, y, callback): (LuaValue, LuaValue, LuaValue, LuaValue) =
                stack.consume(ctx)?;
            let source = source_of(source, "image.pixel source")?;
            let (x, y) = (uint(x, "pixel x")?, uint(y, "pixel y")?);
            if matches!(callback, LuaValue::Nil) {
                match ops::pixel_at(&source, x, y, SYNC_PIXEL_LIMIT) {
                    Ok(rgba) => stack.replace(ctx, rgba_color(ctx, rgba)),
                    Err(error) => stack.replace(ctx, (LuaValue::Nil, error.to_string())),
                }
                return Ok(CallbackReturn::Return);
            }
            let callback = callback_of(callback, "image.pixel callback")?;
            stack.replace(
                ctx,
                queue(
                    ctx,
                    &pixel_state,
                    ImageJob::Pixel { source, x, y },
                    callback,
                ),
            );
            Ok(CallbackReturn::Return)
        }),
    );

    let palette_state = Rc::clone(&state);
    image.set_field(
        ctx,
        "palette",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (source, count, callback): (LuaValue, LuaValue, LuaValue) = stack.consume(ctx)?;
            let source = source_of(source, "image.palette source")?;
            let count = i64::from(uint(count, "palette size")?);
            if !(1..=MAX_PALETTE).contains(&count) {
                return Err(HostError(format!("palette size must be 1..{MAX_PALETTE}")).into());
            }
            let Some(callback) = callback_of(callback, "image.palette callback")? else {
                return Err(HostError("image.palette needs a callback".into()).into());
            };
            let job = ImageJob::Palette {
                source,
                count: count as usize,
            };
            stack.replace(ctx, queue(ctx, &palette_state, job, Some(callback)));
            Ok(CallbackReturn::Return)
        }),
    );

    let limits = Table::new(&ctx);
    limits.set_field(ctx, "max_dimension", i64::from(ops::MAX_DIMENSION));
    limits.set_field(ctx, "max_decoded_bytes", ops::MAX_DECODED_BYTES as i64);
    limits.set_field(ctx, "sync_pixel_limit", SYNC_PIXEL_LIMIT as i64);
    limits.set_field(
        ctx,
        "max_in_flight",
        crate::image_jobs::MAX_IN_FLIGHT as i64,
    );
    image.set_field(ctx, "limits", limits);

    morf.set_field(ctx, "image", image);
}

/// Straight RGBA bytes as a colour value.
pub(crate) fn rgba_color(ctx: Context<'_>, rgba: [u8; 4]) -> LuaValue<'_> {
    color_userdata(
        ctx,
        pastel::Color::from_rgba(rgba[0], rgba[1], rgba[2], f64::from(rgba[3]) / 255.0),
    )
}
