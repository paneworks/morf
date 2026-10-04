//! A source becomes pixels: paths and `file://` URIs normalised, bytes
//! read, sizes probed, and rasters and SVG documents decoded.

use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use image::ImageReader;
use resvg::{tiny_skia, usvg};

use super::ImageError;
use crate::inline::{InlineSource, inline_source, is_inline_source};
use crate::quantize::ImageData;

pub(crate) fn normalize_source(source: &Path) -> Result<PathBuf, ImageError> {
    let Some(value) = source.to_str() else {
        return Ok(source.to_path_buf());
    };
    let Some(uri) = value.strip_prefix("file://") else {
        return Ok(source.to_path_buf());
    };
    let uri = uri.strip_prefix("localhost").unwrap_or(uri);
    if !uri.starts_with('/') {
        return Err(ImageError::InvalidSource(value.to_owned()));
    }
    let bytes = uri.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(high) = bytes.get(index + 1).and_then(|value| hex_digit(*value)) else {
                return Err(ImageError::InvalidSource(value.to_owned()));
            };
            let Some(low) = bytes.get(index + 2).and_then(|value| hex_digit(*value)) else {
                return Err(ImageError::InvalidSource(value.to_owned()));
            };
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Ok(std::ffi::OsString::from_vec(decoded).into())
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

/// A source's bytes, and whether they are an SVG document.
///
/// The one place a source becomes bytes, so that everything reading one —
/// the decoder, the size probe, the quantiser, `morf.image` — accepts the
/// same things: a path, a `file://` URI, or inline content.
pub(crate) fn read_source(source: &Path) -> Result<(Vec<u8>, bool), ImageError> {
    if let Some(inline) = source.to_str().and_then(inline_source) {
        return Ok(match inline? {
            InlineSource::Svg(bytes) => (bytes, true),
            InlineSource::Raster(bytes) => (bytes, false),
        });
    }
    let bytes = fs::read(source)?;
    Ok((bytes, is_svg_path(source)))
}

/// Whether a path names an SVG document by its extension.
pub(crate) fn is_svg_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

/// A source's pixel size, from its header alone where it has one.
pub(crate) fn source_dimensions(source: &Path) -> Result<(u32, u32), ImageError> {
    let inline = source.to_str().is_some_and(is_inline_source);
    if !inline && !is_svg_path(source) {
        return Ok(image::image_dimensions(source)?);
    }
    let (bytes, svg) = read_source(source)?;
    if svg {
        let size = svg_tree(&bytes)?.size();
        Ok((size.width().ceil() as u32, size.height().ceil() as u32))
    } else {
        Ok(ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()?
            .into_dimensions()?)
    }
}

pub(crate) fn svg_tree(bytes: &[u8]) -> Result<usvg::Tree, ImageError> {
    let mut options = usvg::Options::default();
    // Icons need no font discovery. Text-bearing SVGs share a lazily loaded,
    // memory-mapped database instead of rescanning fonts on every preview.
    if bytes.windows(5).any(|part| part == b"<text") {
        options.fontdb = crate::svg_fonts::database();
    }
    usvg::Tree::from_data(bytes, &options).map_err(|error| ImageError::Svg(error.to_string()))
}

pub(crate) fn decode_path(path: &Path, width: u32, height: u32) -> Result<ImageData, ImageError> {
    let (bytes, svg) = read_source(path)?;
    if svg {
        decode_svg(&bytes, width, height)
    } else {
        decode_raster(&bytes, width, height)
    }
}

fn decode_raster(bytes: &[u8], width: u32, height: u32) -> Result<ImageData, ImageError> {
    let image = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?
        .resize_exact(width, height, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    Ok(ImageData {
        width,
        height,
        rgba: image.into_raw(),
    })
}

pub(crate) fn decode_svg(bytes: &[u8], width: u32, height: u32) -> Result<ImageData, ImageError> {
    let tree = svg_tree(bytes)?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(ImageError::InvalidSize)?;
    let size = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        width as f32 / size.width(),
        height as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut rgba = pixmap.take();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        // Not `checked_div`: the guard skips the whole channel loop for a
        // fully transparent pixel, so checking once here is the point of it.
        // `unknown_lints` because the lint below is newer than some toolchains
        // this builds on, and an allow for a lint that does not exist yet is
        // itself an error under `-D warnings`.
        #[allow(unknown_lints, clippy::manual_checked_ops)]
        if alpha != 0 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    Ok(ImageData {
        width,
        height,
        rgba,
    })
}
