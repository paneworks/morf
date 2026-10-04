//! Editing a picture: decode, a list of operations, encode.
//!
//! Everything here is plain blocking work with no state, so a caller can run
//! it wherever blocking is allowed — `morf-lua` runs it on worker threads,
//! because decoding a photograph takes longer than a frame and the thread a
//! configuration runs on is the one drawing the frames.
//!
//! Every entry checks the size a picture *claims* in its header before
//! decoding it. A few hundred bytes of PNG can declare itself 60000 pixels
//! square, and decoding that first and checking afterwards is how a
//! wallpaper picker takes the shell down with it.

use std::fs;
use std::io::{BufWriter, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, Limits, RgbaImage};

use crate::image_cache::{
    ImageError, decode_svg, is_svg_path, normalize_source, read_source, source_dimensions, svg_tree,
};
use crate::inline::is_inline_source;
use crate::quantize::ImageData;

/// The widest or tallest picture accepted, in pixels.
pub const MAX_DIMENSION: u32 = 16_384;
/// The most memory one decoded picture may take, in bytes of RGBA.
pub const MAX_DECODED_BYTES: u64 = 256 * 1024 * 1024;
/// What a picture is, from its header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    /// Lowercase format name: `png`, `jpeg`, `webp`, `svg`, …
    pub format: String,
}

/// How a resize treats the aspect ratio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeMode {
    /// Inside the box, aspect kept; one side may come out shorter.
    Fit,
    /// Covering the box, aspect kept, the overflow cropped from the centre.
    Fill,
    /// Exactly the box, aspect ignored.
    Exact,
}

/// One step of an edit.
#[derive(Clone, Debug, PartialEq)]
pub enum ImageOp {
    /// Keep a rectangle; the part outside the picture is dropped.
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// Keep the largest centred square.
    Square,
    /// Scale; a zero side follows from the other and the aspect ratio.
    Resize {
        width: u32,
        height: u32,
        mode: ResizeMode,
    },
    /// Turn clockwise by 90, 180 or 270 degrees.
    Rotate(u16),
    /// Mirror, horizontally (left-right) or not (top-bottom).
    Flip { horizontal: bool },
    /// Gaussian-like blur of this standard deviation, in pixels.
    Blur(f32),
    /// Drop the colour, keeping the alpha.
    Grayscale,
    /// Composite a raster or SVG, preserving alpha.
    Overlay { source: PathBuf, x: u32, y: u32 },
    /// Typed drawing commands, rasterised without building or parsing SVG.
    Annotations(Vec<crate::annotation::Annotation>),
    /// Apply an effect only inside a rectangle.
    Region {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        effect: RegionEffect,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RegionEffect {
    Blur(f32),
    Pixelate(u32),
    Zoom(f32),
}

/// What an edit is written as.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Png,
    Jpeg,
    /// Lossless: the encoder this build has does not do lossy WebP, so
    /// `quality` does not apply to it.
    Webp,
}

impl OutputFormat {
    /// By name, as a configuration writes it.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpeg" | "jpg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    /// From a file name's extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        Self::parse(path.extension()?.to_str()?)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}

/// A whole edit: read one picture, change it, write another.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRequest {
    pub source: PathBuf,
    pub ops: Vec<ImageOp>,
    pub output: PathBuf,
    pub format: OutputFormat,
    /// 1..=100; JPEG only.
    pub quality: u8,
}

/// Refuses a size nothing here should hold.
pub fn check_size(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidSize);
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageError::Refused(format!(
            "image is {width}x{height}; the limit is {MAX_DIMENSION} a side"
        )));
    }
    if u64::from(width) * u64::from(height) * 4 > MAX_DECODED_BYTES {
        return Err(ImageError::Refused(format!(
            "image is {width}x{height}; decoded it would pass {} MiB",
            MAX_DECODED_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// A picture's size and format, reading only its header.
pub fn image_info(source: impl AsRef<Path>) -> Result<ImageInfo, ImageError> {
    if let Some(image) = crate::ops_memory::source(source.as_ref())? {
        return Ok(ImageInfo {
            width: image.width,
            height: image.height,
            format: "rgba".into(),
        });
    }
    let source = normalize_source(source.as_ref())?;
    let inline = source.to_str().is_some_and(is_inline_source);
    if !inline && !is_svg_path(&source) {
        let reader = ImageReader::open(&source)?.with_guessed_format()?;
        let format = format_name(reader.format());
        let (width, height) = reader.into_dimensions()?;
        return Ok(ImageInfo {
            width,
            height,
            format,
        });
    }
    let (bytes, svg) = read_source(&source)?;
    if svg {
        let size = svg_tree(&bytes)?.size();
        return Ok(ImageInfo {
            width: size.width().ceil() as u32,
            height: size.height().ceil() as u32,
            format: "svg".to_owned(),
        });
    }
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let format = format_name(reader.format());
    let (width, height) = reader.into_dimensions()?;
    Ok(ImageInfo {
        width,
        height,
        format,
    })
}

fn format_name(format: Option<ImageFormat>) -> String {
    match format {
        Some(ImageFormat::Jpeg) => "jpeg".to_owned(),
        Some(format) => format!("{format:?}").to_ascii_lowercase(),
        None => "unknown".to_owned(),
    }
}

/// Decodes a whole picture at its own size, within the bounds.
///
/// An SVG is drawn at the size its document gives.
pub fn decode_bounded(source: impl AsRef<Path>) -> Result<DynamicImage, ImageError> {
    if let Some(image) = crate::ops_memory::source(source.as_ref())? {
        check_size(image.width, image.height)?;
        return Ok(DynamicImage::ImageRgba8(rgba_image((*image).clone())?));
    }
    let source = normalize_source(source.as_ref())?;
    let (width, height) = source_dimensions(&source)?;
    check_size(width, height)?;
    let (bytes, svg) = read_source(&source)?;
    if svg {
        let data = decode_svg(&bytes, width, height)?;
        return Ok(DynamicImage::ImageRgba8(rgba_image(data)?));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    // The header was checked above; these hold the decoder to it, for a file
    // whose frames disagree with its header.
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_BYTES * 2);
    reader.limits(limits);
    Ok(reader.decode()?)
}

fn rgba_image(data: ImageData) -> Result<RgbaImage, ImageError> {
    RgbaImage::from_raw(data.width, data.height, data.rgba).ok_or(ImageError::InvalidSize)
}
/// Runs an edit's operations in order.
pub fn apply_ops(mut image: DynamicImage, ops: &[ImageOp]) -> Result<DynamicImage, ImageError> {
    for op in ops {
        image = apply_op(image, op.clone())?;
        check_size(image.width(), image.height())?;
    }
    Ok(image)
}
fn apply_op(image: DynamicImage, op: ImageOp) -> Result<DynamicImage, ImageError> {
    let (width, height) = (image.width(), image.height());
    Ok(match op {
        ImageOp::Crop {
            x,
            y,
            width: crop_width,
            height: crop_height,
        } => {
            let right = x.saturating_add(crop_width).min(width);
            let bottom = y.saturating_add(crop_height).min(height);
            if x >= right || y >= bottom {
                return Err(ImageError::Refused(format!(
                    "crop {x},{y} {crop_width}x{crop_height} misses a {width}x{height} image"
                )));
            }
            image.crop_imm(x, y, right - x, bottom - y)
        }
        ImageOp::Square => {
            let side = width.min(height);
            image.crop_imm((width - side) / 2, (height - side) / 2, side, side)
        }
        ImageOp::Resize {
            width: target_width,
            height: target_height,
            mode,
        } => {
            let (target_width, target_height) =
                resolve_target(width, height, target_width, target_height)?;
            match mode {
                ResizeMode::Fit => image.resize(target_width, target_height, FilterType::Lanczos3),
                ResizeMode::Fill => {
                    image.resize_to_fill(target_width, target_height, FilterType::Lanczos3)
                }
                ResizeMode::Exact => {
                    image.resize_exact(target_width, target_height, FilterType::Lanczos3)
                }
            }
        }
        ImageOp::Rotate(90) => image.rotate90(),
        ImageOp::Rotate(180) => image.rotate180(),
        ImageOp::Rotate(270) => image.rotate270(),
        ImageOp::Rotate(degrees) => {
            return Err(ImageError::Refused(format!(
                "rotate takes 90, 180 or 270, not {degrees}"
            )));
        }
        ImageOp::Flip { horizontal: true } => image.fliph(),
        ImageOp::Flip { horizontal: false } => image.flipv(),
        ImageOp::Blur(sigma) => image.fast_blur(sigma),
        ImageOp::Grayscale => image.grayscale(),
        ImageOp::Overlay { source, x, y } => {
            let overlay = decode_bounded(source)?.to_rgba8();
            let mut base = image.into_rgba8();
            image::imageops::overlay(&mut base, &overlay, i64::from(x), i64::from(y));
            DynamicImage::ImageRgba8(base)
        }
        ImageOp::Annotations(marks) => crate::annotation::draw(image, &marks)?,
        ImageOp::Region {
            x,
            y,
            width: rw,
            height: rh,
            effect,
        } => {
            let rw = rw.min(width.saturating_sub(x));
            let rh = rh.min(height.saturating_sub(y));
            if rw == 0 || rh == 0 {
                return Err(ImageError::Refused("effect region misses the image".into()));
            }
            let region = image.crop_imm(x, y, rw, rh);
            let edited = match effect {
                RegionEffect::Blur(sigma) if sigma.is_finite() && sigma > 0.0 && sigma <= 100.0 => {
                    region.fast_blur(sigma)
                }
                RegionEffect::Pixelate(block) if (1..=256).contains(&block) => region
                    .resize_exact(rw.div_ceil(block), rh.div_ceil(block), FilterType::Triangle)
                    .resize_exact(rw, rh, FilterType::Nearest),
                RegionEffect::Zoom(factor)
                    if factor.is_finite() && (1.0..=10.0).contains(&factor) =>
                {
                    let sw = (rw as f32 / factor).round().max(1.0) as u32;
                    let sh = (rh as f32 / factor).round().max(1.0) as u32;
                    region
                        .crop_imm((rw - sw) / 2, (rh - sh) / 2, sw, sh)
                        .resize_exact(rw, rh, FilterType::Lanczos3)
                }
                _ => return Err(ImageError::Refused("invalid region effect strength".into())),
            };
            let mut base = image.into_rgba8();
            image::imageops::replace(&mut base, &edited.to_rgba8(), i64::from(x), i64::from(y));
            DynamicImage::ImageRgba8(base)
        }
    })
}

/// A resize target with a zero side filled in from the aspect ratio.
fn resolve_target(
    width: u32,
    height: u32,
    to_width: u32,
    to_height: u32,
) -> Result<(u32, u32), ImageError> {
    let scaled = |value: u32, numerator: u32, denominator: u32| {
        ((u64::from(value) * u64::from(numerator)).div_ceil(u64::from(denominator))).max(1)
    };
    let (to_width, to_height) = match (to_width, to_height) {
        (0, 0) => {
            return Err(ImageError::Refused(
                "resize needs a width or a height".into(),
            ));
        }
        (0, h) => (scaled(width, h, height), u64::from(h)),
        (w, 0) => (u64::from(w), scaled(height, w, width)),
        (w, h) => (u64::from(w), u64::from(h)),
    };
    let to_width = u32::try_from(to_width).unwrap_or(u32::MAX);
    let to_height = u32::try_from(to_height).unwrap_or(u32::MAX);
    check_size(to_width, to_height)?;
    Ok((to_width, to_height))
}

/// Writes a picture, replacing the file only once the whole of it is written.
///
/// Through a temporary name beside the target and a rename, so that a
/// wallpaper being regenerated is never seen half-written by whatever is
/// showing it — and a failed encode leaves the old one in place.
pub fn encode_to_file(
    image: &DynamicImage,
    path: &Path,
    format: OutputFormat,
    quality: u8,
) -> Result<(), ImageError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or_else(|| ImageError::Refused(format!("`{}` is not a file path", path.display())))?;
    let temporary = path.with_file_name(format!(
        ".{}.{}-{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = write_encoded(image, &temporary, format, quality)
        .and_then(|()| fs::rename(&temporary, path).map_err(ImageError::Io));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

fn write_encoded(
    image: &DynamicImage,
    path: &Path,
    format: OutputFormat,
    quality: u8,
) -> Result<(), ImageError> {
    let mut writer = BufWriter::new(fs::File::create(path)?);
    let rgba = || match image {
        DynamicImage::ImageRgba8(rgba) => std::borrow::Cow::Borrowed(rgba),
        _ => std::borrow::Cow::Owned(image.to_rgba8()),
    };
    match format {
        OutputFormat::Png => rgba().write_with_encoder(PngEncoder::new(&mut writer))?,
        // JPEG has no alpha; the channel is dropped rather than composited,
        // since there is no one right colour to put behind it.
        OutputFormat::Jpeg => image
            .to_rgb8()
            .write_with_encoder(JpegEncoder::new_with_quality(
                &mut writer,
                quality.clamp(1, 100),
            ))?,
        OutputFormat::Webp => rgba().write_with_encoder(WebPEncoder::new_lossless(&mut writer))?,
    }
    writer.flush()?;
    Ok(())
}

/// Runs a whole edit and says what was written.
pub fn process(request: &ProcessRequest) -> Result<ImageInfo, ImageError> {
    let image = decode_bounded(&request.source)?;
    let image = apply_ops(image, &request.ops)?;
    encode_to_file(&image, &request.output, request.format, request.quality)?;
    Ok(ImageInfo {
        width: image.width(),
        height: image.height(),
        format: request.format.name().to_owned(),
    })
}

/// Writes raw RGBA pixels to a file — a capture, say.
pub fn save_rgba(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    path: &Path,
    format: OutputFormat,
    quality: u8,
) -> Result<ImageInfo, ImageError> {
    check_size(width, height)?;
    let image = RgbaImage::from_raw(width, height, rgba).ok_or(ImageError::InvalidSize)?;
    encode_to_file(&DynamicImage::ImageRgba8(image), path, format, quality)?;
    Ok(ImageInfo {
        width,
        height,
        format: format.name().to_owned(),
    })
}

pub use crate::ops_queries::{palette, pixel_at};
