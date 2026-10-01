use crate::image_cache::{normalize_source, source_dimensions};
use crate::ops::decode_bounded;
use crate::{ImageData, ImageError, PaletteEntry, palette_of};
use image::GenericImageView;
use std::path::Path;

/// The longest side a picture is shrunk to before its palette is taken.
///
/// A palette is a statistic, and 160 pixels a side is some 25000 samples —
/// plenty to rank colours by — for a fraction of the work of every pixel of
/// a photograph.
const PALETTE_SIDE: u32 = 160;

/// One pixel's straight RGBA, refusing pictures of more than `max_pixels`.
///
/// The bound is the caller's because the right one depends on where this
/// runs: a worker can afford the full limit, the thread drawing the shell
/// cannot.
pub fn pixel_at(
    source: impl AsRef<Path>,
    x: u32,
    y: u32,
    max_pixels: u64,
) -> Result<[u8; 4], ImageError> {
    if let Some(image) = crate::ops_memory::source(source.as_ref())? {
        if u64::from(image.width) * u64::from(image.height) > max_pixels
            || x >= image.width
            || y >= image.height
        {
            return Err(ImageError::Refused(
                "pixel is outside the image or synchronous pixel limit".into(),
            ));
        }
        let offset = (y as usize * image.width as usize + x as usize) * 4;
        return Ok(image.rgba[offset..offset + 4].try_into().unwrap());
    }
    let source = normalize_source(source.as_ref())?;
    let (width, height) = source_dimensions(&source)?;
    if u64::from(width) * u64::from(height) > max_pixels {
        return Err(ImageError::Refused(format!(
            "image is {width}x{height}, over the {max_pixels} pixels this call reads"
        )));
    }
    if x >= width || y >= height {
        return Err(ImageError::Refused(format!(
            "pixel {x},{y} is outside a {width}x{height} image"
        )));
    }
    let image = decode_bounded(&source)?;
    if x >= image.width() || y >= image.height() {
        return Err(ImageError::InvalidSize);
    }
    Ok(image.get_pixel(x, y).0)
}

/// The `count` dominant colours of a picture, most common first.
pub fn palette(source: impl AsRef<Path>, count: usize) -> Result<Vec<PaletteEntry>, ImageError> {
    let image = decode_bounded(source)?;
    let image = if image.width().max(image.height()) > PALETTE_SIDE {
        image.thumbnail(PALETTE_SIDE, PALETTE_SIDE)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    let data = ImageData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    };
    Ok(palette_of(&data, count))
}
