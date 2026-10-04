use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::ImageReader;
use resvg::{tiny_skia, usvg};

use crate::animation::Animation;
use crate::distance_field::distance_field_from_alpha;
use crate::icons::IconResolver;
use crate::inline::{InlineSource, inline_source, is_inline_source};
use crate::quantize::ImageData;

/// Image or icon loading failure.
#[derive(Debug)]
pub enum ImageError {
    /// The source could not be read.
    Io(std::io::Error),
    /// The raster format could not be decoded.
    Raster(image::ImageError),
    /// The SVG document could not be parsed or rasterized.
    Svg(String),
    /// No matching icon was found.
    IconNotFound(String),
    /// The requested output size was invalid.
    InvalidSize,
    /// The source URI did not identify a local file.
    InvalidSource(String),
    /// The source alpha mask had no detectable boundary.
    DistanceFieldEmpty,
    /// A request was refused before any work: too large, or out of range.
    Refused(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "image I/O failed: {error}"),
            Self::Raster(error) => write!(f, "image decode failed: {error}"),
            Self::Svg(error) => write!(f, "SVG decode failed: {error}"),
            Self::IconNotFound(name) => write!(f, "icon `{name}` was not found"),
            Self::InvalidSize => f.write_str("image size must be greater than zero"),
            Self::InvalidSource(source) => write!(f, "invalid local image source `{source}`"),
            Self::DistanceFieldEmpty => f.write_str("distance-field source has no alpha edge"),
            Self::Refused(why) => f.write_str(why),
        }
    }
}

impl StdError for ImageError {}

impl From<std::io::Error> for ImageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<image::ImageError> for ImageError {
    fn from(error: image::ImageError) -> Self {
        Self::Raster(error)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    source: PathBuf,
    width: u32,
    height: u32,
    scale_120: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DistanceFieldKey {
    image: CacheKey,
    spread: u32,
}

/// Cache for decoded images and resolved icon names.
#[derive(Default)]
pub struct ImageCache {
    images: HashMap<CacheKey, Arc<ImageData>>,
    /// Images that never came from a file.
    ///
    /// A screen capture arrives as pixels, and until this existed there was
    /// nothing to do with them: `ui.Image` resolves a path, and the only way to
    /// show a capture was to encode it to a file and read it back — megabytes
    /// of PNG per thumbnail, per refresh, to move pixels that were already in
    /// memory. Named sources beginning with `memory:` resolve here instead.
    memory: HashMap<String, Arc<ImageData>>,
    icons: HashMap<(String, String, u32, u32), PathBuf>,
    intrinsic: HashMap<PathBuf, (u32, u32)>,
    distance_fields: HashMap<DistanceFieldKey, Arc<ImageData>>,
    /// Why a source could not be drawn, by source, for a node's `status`.
    failures: HashMap<String, String>,
    /// Whether each source moves, and its frames when it does.
    animations: HashMap<String, Option<Arc<Animation>>>,
    /// Least recently asked-for last: which moving pictures to drop first.
    animation_use: Vec<String>,
}

impl ImageCache {
    /// Loads a source at a logical size and protocol scale in 120ths.
    pub fn load(
        &mut self,
        source: impl AsRef<Path>,
        logical_width: u32,
        logical_height: u32,
        scale_120: u32,
    ) -> Result<Arc<ImageData>, ImageError> {
        // A named image, if this is one. Handed back at its own size: it was
        // not decoded from anything, so there is no larger original to resample
        // from, and the node's `fill_mode` is what decides how it lands in the
        // rectangle — the same as for a file whose intrinsic size differs.
        let result = self.load_uncached(source.as_ref(), logical_width, logical_height, scale_120);
        if let Err(error) = &result
            && let Some(text) = source.as_ref().to_str()
        {
            self.note_failure(text, error);
        }
        result
    }

    fn load_uncached(
        &mut self,
        source: &Path,
        logical_width: u32,
        logical_height: u32,
        scale_120: u32,
    ) -> Result<Arc<ImageData>, ImageError> {
        if let Some(name) = source.to_str().and_then(memory_name) {
            return self.memory_image(name);
        }
        let source = normalize_source(source)?;
        let width = physical_size(logical_width, scale_120)?;
        let height = physical_size(logical_height, scale_120)?;
        let key = CacheKey {
            source: source.clone(),
            width,
            height,
            scale_120,
        };
        if let Some(image) = self.images.get(&key) {
            return Ok(Arc::clone(image));
        }
        let image = Arc::new(decode_path(&source, width, height)?);
        self.images.insert(key, Arc::clone(&image));
        Ok(image)
    }

    fn note_failure(&mut self, source: &str, error: &ImageError) {
        if self.failures.len() >= MAX_INTRINSIC {
            self.failures.clear();
        }
        self.failures.insert(source.to_owned(), error.to_string());
    }

    /// Why the last attempt to draw `source` failed, if it did.
    pub fn failure(&self, source: &str) -> Option<&str> {
        self.failures.get(source).map(String::as_str)
    }

    /// The frames of a moving source (GIF, animated PNG or WebP), or `None`
    /// for a still one or one that cannot be read (see [`Self::failure`]).
    ///
    /// Asked every frame for every image, so the answer is remembered: a
    /// still picture costs a lookup after its first twelve bytes were read,
    /// and a moving one is decoded once. At most [`MAX_ANIMATIONS`] moving
    /// pictures and [`MAX_ANIMATIONS_BYTES`] of their frames are kept; the
    /// least recently drawn go first.
    pub fn animation(&mut self, source: &str) -> Option<Arc<Animation>> {
        // Inline pictures and named ones are drawn still: an inline source is
        // its own cache key, and remembering one per frame's worth of text
        // would hold every sparkline a configuration ever drew.
        if source.is_empty()
            || memory_name(source).is_some()
            || source.starts_with("gpu:")
            || is_inline_source(source)
        {
            return None;
        }
        if let Some(known) = self.animations.get(source) {
            let known = known.clone();
            if known.is_some()
                && let Some(index) = self.animation_use.iter().position(|used| used == source)
            {
                let used = self.animation_use.remove(index);
                self.animation_use.push(used);
            }
            return known;
        }
        let decoded = match normalize_source(Path::new(source)) {
            Ok(path) if crate::animation::may_move(&path) => {
                crate::animation::decode_animation(&path).map(|found| found.map(Arc::new))
            }
            Ok(_) => Ok(None),
            Err(error) => Err(error),
        };
        let found = match decoded {
            Ok(found) => found,
            Err(error) => {
                self.note_failure(source, &error);
                None
            }
        };
        let stills = self
            .animations
            .values()
            .filter(|known| known.is_none())
            .count();
        if found.is_none() && stills >= MAX_INTRINSIC {
            self.animations.retain(|_, known| known.is_some());
        }
        if found.is_some() {
            self.animation_use.push(source.to_owned());
        }
        self.animations.insert(source.to_owned(), found.clone());
        self.evict_animations();
        found
    }

    fn evict_animations(&mut self) {
        let mut bytes: usize = self
            .animations
            .values()
            .flatten()
            .map(|animation| animation.bytes())
            .sum();
        while self.animation_use.len() > 1
            && (self.animation_use.len() > MAX_ANIMATIONS || bytes > MAX_ANIMATIONS_BYTES)
        {
            let oldest = self.animation_use.remove(0);
            if let Some(Some(animation)) = self.animations.remove(&oldest) {
                bytes -= animation.bytes();
            }
        }
    }

    /// How many moving pictures are held, and the bytes of their frames.
    pub fn animation_usage(&self) -> (usize, usize) {
        let held = self.animations.values().flatten();
        (
            self.animation_use.len(),
            held.map(|animation| animation.bytes()).sum(),
        )
    }

    /// Publishes pixels under a name that `ui.Image` can resolve.
    ///
    /// Replaces whatever that name held. A capture refreshed every second is
    /// the expected case, and a caller that had to invent a new name each time
    /// would be leaking one image per refresh.
    pub fn insert_memory(&mut self, name: impl Into<String>, image: ImageData) {
        self.memory.insert(name.into(), Arc::new(image));
    }

    /// Drops a named image.
    ///
    /// Nothing collects these on their own: they are as large as the pictures
    /// they hold, and only the caller knows when a window it was showing has
    /// gone.
    pub fn forget_memory(&mut self, name: &str) -> bool {
        self.memory.remove(name).is_some()
    }

    /// How many named images are held, and how many bytes they occupy.
    pub fn memory_usage(&self) -> (usize, usize) {
        (
            self.memory.len(),
            self.memory.values().map(|image| image.rgba.len()).sum(),
        )
    }

    /// Resolves and loads an icon into a logical rectangle.
    pub fn load_icon_sized(
        &mut self,
        name: &str,
        theme: &str,
        logical_width: u32,
        logical_height: u32,
        scale_120: u32,
    ) -> Result<Arc<ImageData>, ImageError> {
        let physical = physical_size(logical_width.max(logical_height), scale_120)?;
        let path = self.resolve_icon(name, theme, physical, scale_120)?;
        self.load(path, logical_width, logical_height, scale_120)
    }

    /// Loads an image alpha mask and caches its normalized signed distance field.
    pub fn load_distance_field(
        &mut self,
        source: impl AsRef<Path>,
        logical_width: u32,
        logical_height: u32,
        scale_120: u32,
        spread: f32,
    ) -> Result<Arc<ImageData>, ImageError> {
        let source = normalize_source(source.as_ref())?;
        let width = physical_size(logical_width, scale_120)?;
        let height = physical_size(logical_height, scale_120)?;
        let key = DistanceFieldKey {
            image: CacheKey {
                source: source.clone(),
                width,
                height,
                scale_120,
            },
            spread: spread.max(0.5).to_bits(),
        };
        if let Some(image) = self.distance_fields.get(&key) {
            return Ok(Arc::clone(image));
        }
        let image = self.load(source, logical_width, logical_height, scale_120)?;
        let field = Arc::new(distance_field_from_alpha(&image, spread)?);
        self.distance_fields.insert(key, Arc::clone(&field));
        Ok(field)
    }

    /// Resolves an icon and caches a signed distance field from its alpha mask.
    pub fn load_icon_distance_field_sized(
        &mut self,
        name: &str,
        theme: &str,
        logical_width: u32,
        logical_height: u32,
        scale_120: u32,
        spread: f32,
    ) -> Result<Arc<ImageData>, ImageError> {
        let physical = physical_size(logical_width.max(logical_height), scale_120)?;
        let path = self.resolve_icon(name, theme, physical, scale_120)?;
        self.load_distance_field(path, logical_width, logical_height, scale_120, spread)
    }

    /// A named image: one this cache was handed, or one a configuration
    /// published for every renderer.
    fn memory_image(&self, name: &str) -> Result<Arc<ImageData>, ImageError> {
        self.memory
            .get(name)
            .cloned()
            .or_else(|| crate::published::published(name))
            .ok_or_else(|| ImageError::InvalidSource(format!("memory:{name} is not held")))
    }

    /// Returns a source's unscaled pixel dimensions.
    pub fn intrinsic_size(&mut self, source: impl AsRef<Path>) -> Result<(u32, u32), ImageError> {
        if let Some(name) = source.as_ref().to_str().and_then(memory_name) {
            return self
                .memory_image(name)
                .map(|image| (image.width, image.height));
        }
        let source = normalize_source(source.as_ref())?;
        if let Some(size) = self.intrinsic.get(&source) {
            return Ok(*size);
        }
        let size = source_dimensions(&source)?;
        if size.0 == 0 || size.1 == 0 {
            return Err(ImageError::InvalidSize);
        }
        // Paths are few and fixed; inline drawings are not. A configuration
        // redrawing a sparkline every second mints a new source each time,
        // and this map would keep every one of them for the life of the
        // shell. Dropping the lot now and then costs a header read each.
        if self.intrinsic.len() >= MAX_INTRINSIC {
            self.intrinsic.clear();
        }
        self.intrinsic.insert(source, size);
        Ok(size)
    }

    /// Resolves an icon and returns its source dimensions.
    pub fn icon_intrinsic_size(
        &mut self,
        name: &str,
        theme: &str,
        preferred_size: u32,
    ) -> Result<(u32, u32), ImageError> {
        let path = self.resolve_icon(name, theme, preferred_size, 120)?;
        self.intrinsic_size(path)
    }

    /// Finds the file backing one themed icon, remembering the answer.
    ///
    /// Walking a theme index is the expensive half of drawing an icon, and this
    /// block was written out three times — twice byte for byte — so a change to
    /// how icons are found had three places to be made and two chances to be
    /// forgotten.
    fn resolve_icon(
        &mut self,
        name: &str,
        theme: &str,
        physical: u32,
        scale_120: u32,
    ) -> Result<PathBuf, ImageError> {
        let key = (name.to_owned(), theme.to_owned(), physical, scale_120);
        if let Some(path) = self.icons.get(&key) {
            return Ok(path.clone());
        }
        let path = IconResolver::from_environment().find(name, theme, physical)?;
        self.icons.insert(key, path.clone());
        Ok(path)
    }

    /// How many decoded images are held right now.
    pub fn decoded_len(&self) -> usize {
        self.images.len() + self.distance_fields.len()
    }

    /// Removes all decoded and resolved entries.
    pub fn clear(&mut self) {
        self.images.clear();
        self.icons.clear();
        self.intrinsic.clear();
        self.distance_fields.clear();
        self.failures.clear();
        self.animations.clear();
        self.animation_use.clear();
    }

    /// Drops decoded pixels once the cache has grown past what a shell needs.
    ///
    /// Decoded images are keyed on the pixel size they were rasterised at, and
    /// that size comes off live geometry — so animating an icon's width mints
    /// one decode per step and keeps every one of them. Nothing ever called
    /// `clear`, so this grew for the life of the process.
    ///
    /// The resolved icon paths and intrinsic sizes stay: they are small, and
    /// they are the expensive half to rebuild, being a theme-index walk rather
    /// than a decode.
    pub fn shrink(&mut self) {
        if self.images.len() > MAX_DECODED_IMAGES
            || self
                .images
                .values()
                .map(|image| image.rgba.len())
                .sum::<usize>()
                > MAX_CACHED_BYTES
        {
            self.images.clear();
        }
        if self.distance_fields.len() > MAX_DECODED_IMAGES
            || self
                .distance_fields
                .values()
                .map(|image| image.rgba.len())
                .sum::<usize>()
                > MAX_CACHED_BYTES
        {
            self.distance_fields.clear();
        }
    }
}

/// Converts one logical dimension to physical pixels for a decode request.
///
/// This crate depends on nothing, so it cannot share the surface-sizing
/// conversion in `morf-app`, and it deliberately answers differently: a
/// zero-sized image is a request that cannot be satisfied, where a zero-sized
/// surface has to be rounded up to something drawable.
///
/// The width matters. Multiplying in `u32` and saturating first gives a
/// *wrong* answer rather than a clamped one — `u32::MAX` divided by 120 — for
/// any size big enough to overflow, so the multiply happens in `u64`.
fn physical_size(logical: u32, scale_120: u32) -> Result<u32, ImageError> {
    if logical == 0 || scale_120 == 0 {
        return Err(ImageError::InvalidSize);
    }
    let physical = (u64::from(logical) * u64::from(scale_120)).div_ceil(120);
    u32::try_from(physical).map_err(|_| ImageError::InvalidSize)
}

/// How many decoded images to hold before dropping them.
const MAX_DECODED_IMAGES: usize = 128;
const MAX_CACHED_BYTES: usize = 64 * 1024 * 1024;
/// How many moving pictures to keep decoded.
pub const MAX_ANIMATIONS: usize = 16;
/// How many bytes of decoded frames to keep, across every moving picture.
pub const MAX_ANIMATIONS_BYTES: usize = 256 * 1024 * 1024;

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

/// How many intrinsic sizes to remember before starting over.
const MAX_INTRINSIC: usize = 1024;

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

/// The name inside a `memory:` source, if it is one.
///
/// A scheme rather than a path so that a named image can never be confused with
/// a file: `memory:` is not a valid start to an absolute path, and a
/// configuration that writes one has said what it meant.
fn memory_name(source: &str) -> Option<&str> {
    source.strip_prefix("memory:")
}
