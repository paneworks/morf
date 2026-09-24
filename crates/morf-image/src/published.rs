//! Pictures a configuration made from pixels, shared by every renderer.
//!
//! A notification's `image-data`, an album cover a player sent as bytes, a
//! chart drawn pixel by pixel: none of them is a file, and before this the
//! only way to show one was to encode it to a file and name the file. Here a
//! configuration publishes the pixels under a `memory:` source, and every
//! surface's [`crate::ImageCache`] resolves it — there is one renderer per
//! surface, and the picture has to be drawable on each of them.
//!
//! A source is never reused for different pixels. The GPU keeps what it
//! uploaded under the source string, so a name that changed its picture would
//! keep drawing the old one; a republished name gets a new source instead.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::image_cache::ImageError;
use crate::quantize::ImageData;

/// The prefix every published source starts with.
pub const PUBLISHED_PREFIX: &str = "memory:image/";

fn registry() -> &'static Mutex<HashMap<String, Arc<ImageData>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<ImageData>>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Publishes pixels and returns the source `ui.Image` can draw them by.
///
/// `scope` keeps one owner's names apart from another's (a runtime, say);
/// `name` is the owner's own label for it. Each call returns a new source.
pub fn publish(scope: &str, name: &str, image: ImageData) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    let source = format!("{PUBLISHED_PREFIX}{scope}/{name}#{serial}");
    let key = source["memory:".len()..].to_owned();
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, Arc::new(image));
    source
}

/// The pixels behind a published name (the part after `memory:`).
pub fn published(name: &str) -> Option<Arc<ImageData>> {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(name)
        .cloned()
}

/// Drops a published source (`memory:image/...`); whether it was held.
pub fn release(source: &str) -> bool {
    let Some(name) = source.strip_prefix("memory:") else {
        return false;
    };
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(name)
        .is_some()
}

/// How a raw buffer lays its pixels out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    /// Red, green, blue, alpha; one byte each.
    Rgba,
    /// Red, green, blue; one byte each, opaque.
    Rgb,
    /// Blue, green, red, alpha, as Cairo and most capture buffers are in
    /// memory on a little-endian machine.
    Bgra,
    /// Alpha, red, green, blue.
    Argb,
}

impl PixelFormat {
    /// Reads a format as a configuration names it.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "rgba" => Self::Rgba,
            "rgb" => Self::Rgb,
            "bgra" => Self::Bgra,
            "argb" => Self::Argb,
            _ => return None,
        })
    }

    /// Bytes per pixel.
    pub fn channels(self) -> usize {
        match self {
            Self::Rgb => 3,
            _ => 4,
        }
    }
}

/// The largest side a raw picture may have.
pub const MAX_RAW_SIDE: u32 = 8192;

/// Unpacks raw rows into straight RGBA, checking every size against the
/// buffer. `stride` is the bytes from one row's start to the next (at least
/// `width * channels`); `None` means rows are packed. `premultiplied`
/// divides the colour back out of the alpha.
pub fn from_raw(
    bytes: &[u8],
    width: u32,
    height: u32,
    stride: Option<usize>,
    format: PixelFormat,
    premultiplied: bool,
) -> Result<ImageData, ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidSize);
    }
    if width > MAX_RAW_SIDE || height > MAX_RAW_SIDE {
        return Err(ImageError::Refused(format!(
            "a raw image may be at most {MAX_RAW_SIDE} pixels a side, not {width}x{height}"
        )));
    }
    let channels = format.channels();
    let row = width as usize * channels;
    let stride = stride.unwrap_or(row);
    if stride < row {
        return Err(ImageError::Refused(format!(
            "stride {stride} is shorter than a row of {width} pixels ({row} bytes)"
        )));
    }
    // The last row need not be padded out to the stride.
    let needed = stride * (height as usize - 1) + row;
    if bytes.len() < needed {
        return Err(ImageError::Refused(format!(
            "{width}x{height} pixels with stride {stride} need {needed} bytes, got {}",
            bytes.len()
        )));
    }
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        let line = &bytes[y * stride..y * stride + row];
        for pixel in line.chunks_exact(channels) {
            let [r, g, b, a] = match format {
                PixelFormat::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
                PixelFormat::Rgb => [pixel[0], pixel[1], pixel[2], 255],
                PixelFormat::Bgra => [pixel[2], pixel[1], pixel[0], pixel[3]],
                PixelFormat::Argb => [pixel[1], pixel[2], pixel[3], pixel[0]],
            };
            if premultiplied && a != 0 && a != 255 {
                let un = |channel: u8| {
                    ((u32::from(channel) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                };
                rgba.extend_from_slice(&[un(r), un(g), un(b), a]);
            } else {
                rgba.extend_from_slice(&[r, g, b, a]);
            }
        }
    }
    Ok(ImageData {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_rows_unpack_with_stride_and_order() {
        // Two pixels a row, a padding byte after each row but the last.
        let bytes = [1, 2, 3, 4, 5, 6, 0, 7, 8, 9, 10, 11, 12];
        let image = from_raw(&bytes, 2, 2, Some(7), PixelFormat::Rgb, false).unwrap();
        assert_eq!(
            image.rgba,
            [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
        let bgra = from_raw(&[3, 2, 1, 255], 1, 1, None, PixelFormat::Bgra, false).unwrap();
        assert_eq!(bgra.rgba, [1, 2, 3, 255]);
        let argb = from_raw(&[128, 64, 32, 16], 1, 1, None, PixelFormat::Argb, true).unwrap();
        assert_eq!(argb.rgba, [128, 64, 32, 128]);
    }

    #[test]
    fn raw_sizes_are_checked_against_the_buffer() {
        assert!(from_raw(&[0; 11], 2, 2, Some(6), PixelFormat::Rgb, false).is_err());
        assert!(from_raw(&[0; 12], 2, 2, Some(5), PixelFormat::Rgb, false).is_err());
        assert!(from_raw(&[0; 4], 0, 1, None, PixelFormat::Rgba, false).is_err());
        assert!(from_raw(&[0; 4], 1, MAX_RAW_SIDE + 1, None, PixelFormat::Rgba, false).is_err());
    }

    #[test]
    fn a_published_source_resolves_until_released_and_is_never_reused() {
        let pixel = || ImageData {
            width: 1,
            height: 1,
            rgba: vec![9, 8, 7, 255],
        };
        let first = publish("test", "cover", pixel());
        let second = publish("test", "cover", pixel());
        assert_ne!(first, second);
        assert!(first.starts_with(PUBLISHED_PREFIX));
        let mut cache = crate::ImageCache::default();
        assert_eq!(cache.intrinsic_size(&first).unwrap(), (1, 1));
        assert_eq!(cache.load(&first, 4, 4, 120).unwrap().rgba, [9, 8, 7, 255]);
        assert!(release(&first));
        assert!(!release(&first));
        assert!(cache.load(&first, 4, 4, 120).is_err());
        assert!(release(&second));
    }
}
