//! `morf.image.from_rgba`, `from_dbus`, `release` and `encode_png`:
//! pictures that arrive as pixels rather than as files.
//!
//! A notification's `image-data`, an album cover sent as bytes, a chart
//! drawn pixel by pixel. The pixels are published under a `memory:` source
//! every surface can draw (see `morf_image::published`), and held until the
//! configuration releases them, republishes the same name, or goes (a
//! reload, an exit). A source is never reused for other pixels: publishing a
//! name again releases the old picture and answers with a new source.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use luna::{Callback, CallbackReturn, Context, Table, Value as LuaValue};
use morf_image::ImageData;
use morf_image::ops::{self, OutputFormat};
use morf_image::published::{self, PixelFormat};

use crate::api_image_ops::{source_of, uint};
use crate::scene_bindings::HostError;

/// The most pixel bytes one configuration may hold published at once.
pub(crate) const MAX_PUBLISHED_BYTES: usize = 256 * 1024 * 1024;
/// The most pictures one configuration may hold published at once.
pub(crate) const MAX_PUBLISHED: usize = 4096;

/// What one runtime has published, released when the runtime goes.
struct Held {
    scope: String,
    /// The configuration's name for each picture, and its source and size.
    by_name: HashMap<String, (String, usize)>,
    bytes: usize,
    next: u64,
}

impl Held {
    fn new() -> Self {
        static SCOPE: AtomicU64 = AtomicU64::new(1);
        Self {
            scope: format!(
                "{}-{}",
                std::process::id(),
                SCOPE.fetch_add(1, Ordering::Relaxed)
            ),
            by_name: HashMap::new(),
            bytes: 0,
            next: 1,
        }
    }

    fn publish(&mut self, name: Option<String>, image: ImageData) -> Result<String, String> {
        let name = name.unwrap_or_else(|| {
            let name = format!("image-{}", self.next);
            self.next += 1;
            name
        });
        let size = image.rgba.len();
        let replaced = self.by_name.get(&name).map_or(0, |(_, size)| *size);
        if self.bytes - replaced + size > MAX_PUBLISHED_BYTES {
            return Err(format!(
                "published images would exceed {MAX_PUBLISHED_BYTES} bytes; release some first"
            ));
        }
        if replaced == 0 && !self.by_name.contains_key(&name) && self.by_name.len() >= MAX_PUBLISHED
        {
            return Err(format!(
                "at most {MAX_PUBLISHED} images may be published; release some first"
            ));
        }
        let source = published::publish(&self.scope, &name, image);
        if let Some((old, old_size)) = self.by_name.insert(name, (source.clone(), size)) {
            published::release(&old);
            self.bytes -= old_size;
        }
        self.bytes += size;
        Ok(source)
    }

    fn release(&mut self, source: &str) -> bool {
        let Some(name) = self
            .by_name
            .iter()
            .find(|(_, (held, _))| held == source)
            .map(|(name, _)| name.clone())
        else {
            return false;
        };
        let (source, size) = self.by_name.remove(&name).expect("found above");
        self.bytes -= size;
        published::release(&source)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        for (source, _) in self.by_name.values() {
            published::release(source);
        }
    }
}

/// Pixel bytes as a string, or as a list of byte values (what `morf.dbus`
/// hands over for an `ay`).
fn pixel_bytes<'gc>(ctx: Context<'gc>, value: LuaValue<'gc>) -> Result<Vec<u8>, HostError> {
    const MOST: usize = MAX_PUBLISHED_BYTES;
    match value {
        LuaValue::String(text) if text.as_bytes().len() <= MOST => Ok(text.as_bytes().to_vec()),
        LuaValue::Table(list) => {
            let length = usize::try_from(list.length(&ctx)).unwrap_or(0);
            if length > MOST {
                return Err(HostError(format!("pixel list exceeds {MOST} bytes")));
            }
            let mut out = Vec::with_capacity(length);
            for index in 1..=length {
                let byte = match list.get_value(ctx, index as i64) {
                    LuaValue::Integer(value) => u8::try_from(value).ok(),
                    LuaValue::Number(value) if value.fract() == 0.0 => {
                        u8::try_from(value as i64).ok()
                    }
                    _ => None,
                }
                .ok_or_else(|| HostError(format!("pixel byte {index} is not 0..255")))?;
                out.push(byte);
            }
            Ok(out)
        }
        LuaValue::String(_) => Err(HostError(format!("pixels exceed {MOST} bytes"))),
        _ => Err(HostError(
            "pixels must be a string or a list of bytes".into(),
        )),
    }
}

fn option<'gc>(options: Option<Table<'gc>>, ctx: Context<'gc>, field: &str) -> LuaValue<'gc> {
    options.map_or(LuaValue::Nil, |options| options.get_value(ctx, field))
}

fn format_option<'gc>(value: LuaValue<'gc>) -> Result<PixelFormat, HostError> {
    match value {
        LuaValue::Nil => Ok(PixelFormat::Rgba),
        LuaValue::String(name) => {
            PixelFormat::parse(&name.display_lossy().to_string()).ok_or_else(|| {
                HostError("format must be \"rgba\", \"rgb\", \"bgra\" or \"argb\"".into())
            })
        }
        _ => Err(HostError("format must be a string".into())),
    }
}

fn bool_option(value: LuaValue<'_>, what: &str) -> Result<bool, HostError> {
    match value {
        LuaValue::Nil => Ok(false),
        LuaValue::Boolean(value) => Ok(value),
        _ => Err(HostError(format!("{what} must be a boolean"))),
    }
}

fn name_option(value: LuaValue<'_>) -> Result<Option<String>, HostError> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::String(name) if !name.as_bytes().is_empty() && name.as_bytes().len() <= 256 => {
            Ok(Some(name.display_lossy().to_string()))
        }
        _ => Err(HostError("name must be a string of 1..256 bytes".into())),
    }
}

fn stride_of(value: LuaValue<'_>) -> Result<Option<usize>, HostError> {
    match value {
        LuaValue::Nil => Ok(None),
        value => Ok(Some(uint(value, "stride")? as usize)),
    }
}

/// A D-Bus `(iiibiiay)` image, as the positional struct `morf.dbus` gives
/// or as a table with the spec's field names.
struct DbusImage {
    width: u32,
    height: u32,
    rowstride: usize,
    has_alpha: bool,
    bits: u32,
    channels: u32,
}

fn dbus_image<'gc>(ctx: Context<'gc>, data: Table<'gc>) -> Result<(DbusImage, Vec<u8>), HostError> {
    let field = |index: i64, name: &str| {
        let value = data.get_value(ctx, index);
        if matches!(value, LuaValue::Nil) {
            data.get_value(ctx, name)
        } else {
            value
        }
    };
    let has_alpha = match field(4, "has_alpha") {
        LuaValue::Boolean(value) => value,
        LuaValue::Integer(value) => value != 0,
        _ => return Err(HostError("image data has_alpha must be a boolean".into())),
    };
    let image = DbusImage {
        width: uint(field(1, "width"), "image data width")?,
        height: uint(field(2, "height"), "image data height")?,
        rowstride: uint(field(3, "rowstride"), "image data rowstride")? as usize,
        has_alpha,
        bits: uint(field(5, "bits_per_sample"), "image data bits_per_sample")?,
        channels: uint(field(6, "channels"), "image data channels")?,
    };
    let bytes = pixel_bytes(ctx, field(7, "data"))?;
    Ok((image, bytes))
}

pub(crate) fn install_raw_image_api<'gc>(ctx: Context<'gc>, image: Table<'gc>) {
    let held = Rc::new(RefCell::new(Held::new()));

    // from_rgba(bytes, width, height, stride, { format, premultiplied, name })
    let from_rgba_held = Rc::clone(&held);
    image.set_field(
        ctx,
        "from_rgba",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (bytes, width, height, stride, options): (
                LuaValue,
                LuaValue,
                LuaValue,
                LuaValue,
                Option<Table>,
            ) = stack.consume(ctx)?;
            let bytes = pixel_bytes(ctx, bytes)?;
            let width = uint(width, "width")?;
            let height = uint(height, "height")?;
            let stride = stride_of(stride)?;
            let format = format_option(option(options, ctx, "format"))?;
            let premultiplied =
                bool_option(option(options, ctx, "premultiplied"), "premultiplied")?;
            let name = name_option(option(options, ctx, "name"))?;
            let result = published::from_raw(&bytes, width, height, stride, format, premultiplied)
                .map_err(|error| error.to_string())
                .and_then(|image| from_rgba_held.borrow_mut().publish(name, image));
            match result {
                Ok(source) => stack.replace(ctx, source),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    // from_dbus(image_data, { name }): a notification's `image-data`.
    let from_dbus_held = Rc::clone(&held);
    image.set_field(
        ctx,
        "from_dbus",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (data, options): (LuaValue, Option<Table>) = stack.consume(ctx)?;
            let LuaValue::Table(data) = data else {
                return Err(HostError("image.from_dbus takes an (iiibiiay) image".into()).into());
            };
            let name = name_option(option(options, ctx, "name"))?;
            let (info, bytes) = dbus_image(ctx, data)?;
            let format = match (info.bits, info.channels, info.has_alpha) {
                (8, 4, _) => Ok(PixelFormat::Rgba),
                (8, 3, false) => Ok(PixelFormat::Rgb),
                (bits, channels, alpha) => Err(format!(
                    "image data of {channels} channels at {bits} bits (alpha {alpha}) is not one this reads"
                )),
            };
            let result = format
                .and_then(|format| {
                    published::from_raw(
                        &bytes,
                        info.width,
                        info.height,
                        Some(info.rowstride),
                        format,
                        false,
                    )
                    .map_err(|error| error.to_string())
                })
                .and_then(|image| from_dbus_held.borrow_mut().publish(name, image));
            match result {
                Ok(source) => stack.replace(ctx, source),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error)),
            }
            Ok(CallbackReturn::Return)
        }),
    );

    let release_held = Rc::clone(&held);
    image.set_field(
        ctx,
        "release",
        Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let source: luna::String = stack.consume(ctx)?;
            let released = release_held
                .borrow_mut()
                .release(&source.display_lossy().to_string());
            stack.replace(ctx, released);
            Ok(CallbackReturn::Return)
        }),
    );

    // encode_png(bytes, width, height, path, { stride, format, premultiplied })
    image.set_field(
        ctx,
        "encode_png",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (bytes, width, height, path, options): (
                LuaValue,
                LuaValue,
                LuaValue,
                LuaValue,
                Option<Table>,
            ) = stack.consume(ctx)?;
            let bytes = pixel_bytes(ctx, bytes)?;
            let width = uint(width, "width")?;
            let height = uint(height, "height")?;
            let path = source_of(path, "image.encode_png path")?;
            let stride = stride_of(option(options, ctx, "stride"))?;
            let format = format_option(option(options, ctx, "format"))?;
            let premultiplied =
                bool_option(option(options, ctx, "premultiplied"), "premultiplied")?;
            let result = published::from_raw(&bytes, width, height, stride, format, premultiplied)
                .and_then(|image| {
                    ops::save_rgba(
                        image.width,
                        image.height,
                        image.rgba,
                        &path,
                        OutputFormat::Png,
                        100,
                    )
                });
            match result {
                Ok(_) => stack.replace(ctx, true),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error.to_string())),
            }
            Ok(CallbackReturn::Return)
        }),
    );
}
