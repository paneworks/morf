//! Pictures that move: GIF, animated PNG and animated WebP.
//!
//! Every frame is decoded once, whole (the codecs composite each onto the
//! canvas), at the picture's own size, and kept: playing it back is then an
//! upload per frame and no decoding. What that costs in memory is bounded:
//! frames past [`MAX_ANIMATION_BYTES`] or [`MAX_FRAMES`] are not decoded, and
//! the animation plays the frames that fit.

use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, Frames};

use crate::image_cache::{ImageError, read_source};

/// The most decoded bytes one animation may hold.
pub const MAX_ANIMATION_BYTES: usize = 64 * 1024 * 1024;
/// The most frames one animation may hold.
pub const MAX_FRAMES: usize = 1024;
/// A frame delay shorter than this is read as [`DEFAULT_DELAY`], as browsers
/// do: a GIF saying "0" or "10 ms" means "as fast as you like", and is drawn
/// at ten a second everywhere it was made to be looked at.
const SHORTEST_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_DELAY: Duration = Duration::from_millis(100);

/// The frames of a moving picture.
#[derive(Debug)]
pub struct Animation {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Each frame's straight RGBA pixels, `width * height * 4` bytes.
    pub frames: Vec<Vec<u8>>,
    /// How long each frame shows.
    pub delays: Vec<Duration>,
    /// Frames were left out to stay inside the memory bound.
    pub truncated: bool,
}

impl Animation {
    /// How many bytes the frames occupy.
    pub fn bytes(&self) -> usize {
        self.frames.iter().map(Vec::len).sum()
    }
}

/// What a source's first bytes say it is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Gif,
    Png,
    WebP,
}

fn kind(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Kind::Gif)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Kind::Png)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Kind::WebP)
    } else {
        None
    }
}

/// Whether a path's first bytes are a format that can move; cheap, for
/// deciding whether to read the whole file at all.
pub(crate) fn may_move(source: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 12];
    std::fs::File::open(source)
        .and_then(|mut file| file.read_exact(&mut head))
        .is_ok_and(|()| kind(&head).is_some())
}

/// Decodes a source's frames, or `None` when it does not move (a still
/// picture, a single-frame GIF, a PNG without an animation chunk).
pub fn decode_animation(source: &Path) -> Result<Option<Animation>, ImageError> {
    let (bytes, svg) = read_source(source)?;
    if svg {
        return Ok(None);
    }
    decode_animation_bytes(&bytes)
}

/// [`decode_animation`] over bytes already read.
pub fn decode_animation_bytes(bytes: &[u8]) -> Result<Option<Animation>, ImageError> {
    decode_bounded(bytes, MAX_ANIMATION_BYTES)
}

fn decode_bounded(bytes: &[u8], max_bytes: usize) -> Result<Option<Animation>, ImageError> {
    let cursor = Cursor::new(bytes);
    let (frames, size) = match kind(bytes) {
        Some(Kind::Gif) => {
            let decoder = GifDecoder::new(cursor)?;
            let size = image::ImageDecoder::dimensions(&decoder);
            (decoder.into_frames(), size)
        }
        Some(Kind::Png) => {
            let decoder = PngDecoder::new(cursor)?;
            if !decoder.is_apng()? {
                return Ok(None);
            }
            let size = image::ImageDecoder::dimensions(&decoder);
            (decoder.apng()?.into_frames(), size)
        }
        Some(Kind::WebP) => {
            let decoder = WebPDecoder::new(cursor)?;
            if !decoder.has_animation() {
                return Ok(None);
            }
            let size = image::ImageDecoder::dimensions(&decoder);
            (decoder.into_frames(), size)
        }
        None => return Ok(None),
    };
    collect(frames, size, max_bytes)
}

fn collect(
    frames: Frames<'_>,
    (width, height): (u32, u32),
    max_bytes: usize,
) -> Result<Option<Animation>, ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidSize);
    }
    let frame_bytes = width as usize * height as usize * 4;
    if frame_bytes > max_bytes {
        return Err(ImageError::Refused(format!(
            "a {width}x{height} animation is larger than one frame may be"
        )));
    }
    let most = (max_bytes / frame_bytes).clamp(1, MAX_FRAMES);
    let mut animation = Animation {
        width,
        height,
        frames: Vec::new(),
        delays: Vec::new(),
        truncated: false,
    };
    for frame in frames {
        if animation.frames.len() == most {
            animation.truncated = true;
            break;
        }
        let frame = match frame {
            Ok(frame) => frame,
            // A file cut short still plays what it has, as a browser does;
            // one with no good frame at all is an error.
            Err(_) if !animation.frames.is_empty() => break,
            Err(error) => return Err(error.into()),
        };
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay =
            Duration::from_secs_f64(f64::from(numerator) / f64::from(denominator.max(1)) / 1_000.0);
        let buffer = frame.into_buffer();
        if buffer.width() != width || buffer.height() != height {
            // Every codec here composites onto the canvas; a frame that is
            // not canvas-sized is a file this does not understand.
            return Err(ImageError::Refused(
                "an animation frame is not the canvas size".to_owned(),
            ));
        }
        animation.delays.push(if delay < SHORTEST_DELAY {
            DEFAULT_DELAY
        } else {
            delay
        });
        animation.frames.push(buffer.into_raw());
    }
    Ok((animation.frames.len() > 1).then_some(animation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, RgbaImage};
    use std::sync::Arc;

    pub(crate) fn gif(frames: &[[u8; 4]], side: u32, delay_ms: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut out);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for colour in frames {
                let image = RgbaImage::from_pixel(side, side, image::Rgba(*colour));
                encoder
                    .encode_frame(Frame::from_parts(
                        image,
                        0,
                        0,
                        Delay::from_numer_denom_ms(delay_ms, 1),
                    ))
                    .unwrap();
            }
        }
        out
    }

    #[test]
    fn the_cache_remembers_what_moves_and_what_failed() {
        let dir = std::env::temp_dir().join(format!("morf-anim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let moving = dir.join("spin.gif");
        std::fs::write(&moving, gif(&[[255, 0, 0, 255], [0, 255, 0, 255]], 3, 40)).unwrap();
        let broken = dir.join("broken.png");
        // A PNG cut short: the size reads, the pixels do not.
        let mut png = Vec::new();
        RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 0, 255]))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(&broken, &png[..png.len() / 2 + 20]).unwrap();
        let mut cache = crate::ImageCache::default();
        let moving = moving.to_str().unwrap();
        let first = cache.animation(moving).unwrap();
        assert_eq!(first.frames.len(), 2);
        assert!(Arc::ptr_eq(&first, &cache.animation(moving).unwrap()));
        assert_eq!(cache.animation_usage().0, 1);
        let broken = broken.to_str().unwrap();
        assert!(cache.animation(broken).is_none());
        assert_eq!(cache.intrinsic_size(broken).unwrap(), (8, 8));
        assert!(cache.failure(broken).is_none());
        assert!(cache.load(broken, 8, 8, 120).is_err());
        assert!(
            cache.failure(broken).unwrap().contains("decode"),
            "{:?}",
            cache.failure(broken)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_gif_decodes_every_frame_with_its_delay() {
        let bytes = gif(
            &[[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]],
            4,
            70,
        );
        let animation = decode_animation_bytes(&bytes).unwrap().unwrap();
        assert_eq!((animation.width, animation.height), (4, 4));
        assert_eq!(animation.frames.len(), 3);
        assert_eq!(&animation.frames[1][..4], &[0, 0, 255, 255]);
        assert_eq!(animation.delays[0], Duration::from_millis(70));
        assert!(!animation.truncated);
    }

    #[test]
    fn a_still_gif_and_a_png_do_not_move_and_junk_is_an_error() {
        let still = gif(&[[1, 2, 3, 255]], 2, 0);
        assert!(decode_animation_bytes(&still).unwrap().is_none());
        let mut png = Vec::new();
        RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(decode_animation_bytes(&png).unwrap().is_none());
        assert!(decode_animation_bytes(b"GIF89a broken").is_err());
        // A zero delay plays at ten frames a second.
        let fast = gif(&[[0; 4], [255; 4]], 2, 0);
        assert_eq!(
            decode_animation_bytes(&fast).unwrap().unwrap().delays,
            vec![DEFAULT_DELAY; 2]
        );
    }

    #[test]
    fn an_animation_past_the_memory_bound_keeps_the_frames_that_fit() {
        // 8x8 is 256 bytes a frame: five fit in 1300.
        let colours: Vec<[u8; 4]> = (0..8).map(|index| [index as u8 * 10, 0, 0, 255]).collect();
        let bytes = gif(&colours, 8, 50);
        let animation = decode_bounded(&bytes, 1300).unwrap().unwrap();
        assert_eq!(animation.frames.len(), 5);
        assert!(animation.truncated);
        assert!(animation.bytes() <= 1300);
        assert!(
            decode_bounded(&bytes, 100).is_err(),
            "not even one frame fits"
        );
    }
}
