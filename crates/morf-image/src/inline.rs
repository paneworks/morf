//! Pictures written into the source string itself.
//!
//! A configuration that wants to draw a path it computed — a sparkline, a
//! pie slice, a waveform — had to write an SVG to a temporary file and name
//! the file, which means a writable directory, a name nobody else uses, and
//! cleaning up afterwards. Here the source *is* the document: text starting
//! with `<svg` (or an XML prolog), or a `data:` URI carrying SVG or a raster
//! format, plain or base64.
//!
//! Recognised by content rather than by any flag, which is safe because none
//! of these can be a file path a configuration would write: a path does not
//! start with `<` or `data:`. Caches keyed on the source string are then
//! keyed on the content, so two nodes drawing the same text share one decode
//! and a changed path is a new entry rather than a stale hit.

use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

use crate::image_cache::ImageError;

/// The largest inline document accepted, in bytes after decoding.
///
/// An inline source lives in the scene as a string and is re-hashed by every
/// cache it passes through, so a large one is a cost paid per frame. A
/// picture bigger than this belongs in a file.
pub const MAX_INLINE_BYTES: usize = 8 * 1024 * 1024;

/// What an inline source holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InlineSource {
    /// SVG document text.
    Svg(Vec<u8>),
    /// Encoded raster bytes (PNG, JPEG, WebP), format guessed from content.
    Raster(Vec<u8>),
}

impl InlineSource {
    /// The bytes, whichever kind they are.
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Svg(bytes) | Self::Raster(bytes) => bytes,
        }
    }
}

/// Whether a source string is inline content rather than a path or name.
///
/// Cheap: it looks at the first few bytes only, so callers on a per-frame
/// path can branch on it without decoding anything.
pub fn is_inline_source(source: &str) -> bool {
    let trimmed = source.trim_start();
    trimmed.starts_with('<') || starts_with_ignore_case(trimmed, "data:")
}

/// Decodes an inline source, or `None` when the string is not one.
pub fn inline_source(source: &str) -> Option<Result<InlineSource, ImageError>> {
    let trimmed = source.trim_start();
    if trimmed.starts_with('<') {
        if trimmed.len() > MAX_INLINE_BYTES {
            return Some(Err(too_large()));
        }
        return Some(Ok(InlineSource::Svg(trimmed.as_bytes().to_vec())));
    }
    if !starts_with_ignore_case(trimmed, "data:") {
        return None;
    }
    Some(data_uri(&trimmed[5..]))
}

fn data_uri(rest: &str) -> Result<InlineSource, ImageError> {
    let invalid = || ImageError::InvalidSource("malformed data: URI".to_owned());
    let (header, payload) = rest.split_once(',').ok_or_else(invalid)?;
    let mut parameters = header.split(';');
    let mime = parameters
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let base64 = parameters.any(|parameter| parameter.trim().eq_ignore_ascii_case("base64"));
    let bytes = if base64 {
        let compact: Vec<u8> = payload
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        if compact.len() / 4 * 3 > MAX_INLINE_BYTES {
            return Err(too_large());
        }
        BASE64.decode(compact).map_err(|error| {
            ImageError::InvalidSource(format!("data: URI is not valid base64: {error}"))
        })?
    } else {
        percent_decode(payload).ok_or_else(invalid)?
    };
    if bytes.len() > MAX_INLINE_BYTES {
        return Err(too_large());
    }
    match mime.as_str() {
        "image/svg+xml" | "image/svg" => Ok(InlineSource::Svg(bytes)),
        "image/png" | "image/jpeg" | "image/jpg" | "image/webp" => Ok(InlineSource::Raster(bytes)),
        // No type is a guess from the content, which is what a browser does
        // too. Text that opens like markup is a drawing; anything else is
        // left to the raster decoders to recognise or refuse.
        "" if bytes.trim_ascii_start().starts_with(b"<") => Ok(InlineSource::Svg(bytes)),
        "" => Ok(InlineSource::Raster(bytes)),
        other => Err(ImageError::InvalidSource(format!(
            "data: URI of type `{other}` is not an image this can draw"
        ))),
    }
}

const BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(decoded)
}

fn hex(value: u8) -> Option<u8> {
    (value as char).to_digit(16).map(|digit| digit as u8)
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn too_large() -> ImageError {
    ImageError::InvalidSource(format!(
        "inline image is larger than {MAX_INLINE_BYTES} bytes; write it to a file"
    ))
}
