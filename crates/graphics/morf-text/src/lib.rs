//! Text shaping, measurement, and glyph rasterization for morf.

use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::rc::Rc;

use cosmic_text::{Buffer, Family, FontSystem, SwashCache};
use morf_layout::{TextAlignment, TextElide};
use morf_scene::{FastMap, NodeHandle};

use crate::glyph_fields::FieldImage;

pub(crate) struct CachedBuffer {
    pub(crate) buffer: Buffer,
    input: Option<TextInput>,
    /// What shaping could not be told, applied to every glyph after it.
    pub(crate) word_spacing: f32,
    pub(crate) alignment: TextAlignment,
    /// The runs it was set in, when it was: glyph metadata is an index into
    /// these, plus one.
    pub(crate) rich: Option<std::sync::Arc<morf_scene::RichText>>,
    /// The variable-font axes it was shaped at and its glyphs are drawn at,
    /// besides `wght` (the weight): its own, and `opsz` at its size when
    /// optical sizing is automatic.
    pub(crate) axes: Vec<morf_layout::FontAxis>,
    /// Whether `opsz` among `axes` is the size's rather than the style's, so a
    /// run set at a size of its own is drawn at that size's.
    pub(crate) optical: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResolvedFamily {
    Name(String),
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
}

impl ResolvedFamily {
    pub(crate) fn family(&self) -> Family<'_> {
        match self {
            Self::Name(name) => Family::Name(name),
            Self::Serif => Family::Serif,
            Self::SansSerif => Family::SansSerif,
            Self::Monospace => Family::Monospace,
            Self::Cursive => Family::Cursive,
            Self::Fantasy => Family::Fantasy,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Serif => "serif",
            Self::SansSerif => "sans-serif",
            Self::Monospace => "monospace",
            Self::Cursive => "cursive",
            Self::Fantasy => "fantasy",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TextInput {
    text: String,
    family: String,
    size: u64,
    width: Option<u64>,
    wrap: bool,
    alignment: TextAlignment,
    elide: TextElide,
    font_weight: u16,
    font_source: Option<String>,
    max_lines: usize,
    style: morf_layout::TextStyleKey,
}

/// A morph between two glyphs: the outlines paired, the box every frame is
/// measured over, and the frames themselves, measured as the morph reaches
/// them rather than all at once -- a word of new pairs measured up front
/// was a stall of a tenth of a second on the first frame of its motion.
pub(crate) struct MeasuredPair {
    pub(crate) paired: Vec<morf_vector::Paired>,
    pub(crate) area: glyph_fields::FieldBox,
    pub(crate) spread: f32,
    pub(crate) frames: Vec<Option<Rc<FieldImage>>>,
}

/// Two neighbouring frames of a morph, their atlas keys, and where between
/// them the glyph currently is.
pub(crate) type MorphStep = (Rc<FieldImage>, u64, Rc<FieldImage>, u64, f32);

/// How many shapes are measured along the way from one letter to the next.
///
/// The renderer interpolates the two either side of where it is, so this sets
/// how far apart those two are: enough of them that neighbours differ by a
/// twelfth of the journey, which is close enough that averaging their fields
/// has nothing left to get wrong.
pub(crate) const MORPH_FRAMES: usize = 13;

/// One glyph paired with the glyph it is turning into, if it has one, and how
/// far apart the two shapes are.
pub type GlyphPair = (RasterGlyph, Option<RasterGlyph>, f32);

/// Which of a node's two shaped runs a buffer holds.
///
/// A text node that is morphing has two: the text it is, and the text it is
/// turning into. Both have to stay shaped and measured, because the morph
/// interpolates between the two sets of glyphs rather than between two
/// pictures, so one key is not enough.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BufferKey {
    pub(crate) node: NodeHandle,
    pub(crate) morph: bool,
}

impl BufferKey {
    pub(crate) fn own(node: NodeHandle) -> Self {
        Self { node, morph: false }
    }

    pub(crate) fn target(node: NodeHandle) -> Self {
        Self { node, morph: true }
    }
}

/// Shared font database, per-node shaped buffers, and glyph image cache.
pub struct TextSystem {
    fonts: FontSystem,
    glyphs: SwashCache,
    buffers: FastMap<BufferKey, CachedBuffer>,
    /// Fields measured for a *pair* of glyphs over their shared box.
    ///
    /// Separate from `fields` because the box depends on both glyphs, so the
    /// same letter measured against two different partners is two entries.
    field_pairs: FastMap<(u64, u64), Option<MeasuredPair>>,
    /// Shaped keys for characters used as shapes rather than as text, by face.
    ///
    /// Nested rather than keyed by a `(face, character)` pair, because the pair
    /// cannot be looked up without owning the face name — and this is asked
    /// once per glyph layer per frame.
    outline_keys: FastMap<Box<str>, FastMap<char, Option<cosmic_text::CacheKey>>>,
    font_sources: HashSet<String>,
    /// Fields already measured, by the glyph they belong to.
    ///
    /// The distance transform is far too slow to run per frame, and it does not
    /// have to be: a glyph's shape does not change, so this is filled once the
    /// first time a letter is drawn and read from thereafter however many sizes
    /// it is later drawn at.
    fields: FastMap<u64, Option<Rc<FieldImage>>>,
    /// Cell metrics, shaped characters and drawn cells for terminals.
    terminal: terminal::TerminalCache,
    /// Glyphs at a point in a variable font's design space.
    variations: variations::Variations,
    /// Which of `fields` are at such a point, and how many were measured
    /// since they were last let go of.
    varied_field_keys: HashSet<u64>,
    varied_fields: usize,
}

/// Pixel format of one rasterized glyph image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RasterContent {
    /// One alpha byte per pixel.
    Mask,
    /// Four RGBA bytes per pixel.
    Color,
    /// One distance byte per pixel, measured at a fixed reference size.
    ///
    /// Shares the mask atlas — it is a single channel either way — but is read
    /// as a distance from the glyph edge rather than as coverage of it, so one
    /// entry draws the letter at any size.
    Field,
}

/// Positioned glyph bitmap ready for atlas upload.
#[derive(Clone, Debug, PartialEq)]
pub struct RasterGlyph {
    /// Process-local key identifying the cached raster image.
    pub cache_key: u64,
    /// Physical left edge relative to the render target.
    ///
    /// Fractional. A glyph measured once and drawn at any size has no reason to
    /// land on a whole pixel, and forcing it onto one is what makes letters sit
    /// unevenly apart — the spacing error is the rounding, accumulated across a
    /// word. Subpixel positioning is what a rasterizer used its subpixel bins
    /// for; a field needs only to be told where to go.
    pub x: f32,
    /// Physical top edge relative to the render target.
    pub y: f32,
    /// Bitmap width, in the pixels the bitmap was measured at.
    pub width: u32,
    /// Bitmap height, in the pixels the bitmap was measured at.
    pub height: u32,
    /// Quad width in physical pixels.
    ///
    /// The same as `width` for anything rasterized at the size it is drawn. A
    /// distance field is measured once at a reference size and then drawn at
    /// whatever size is asked for, so for those two this is the only place the
    /// two numbers part company: the atlas holds `width`, the screen gets this.
    pub draw_width: f32,
    /// Quad height in physical pixels.
    pub draw_height: f32,
    /// Bitmap pixel format.
    pub content: RasterContent,
    /// A styled run's own colour, straight RGBA, for text set in runs; `None`
    /// draws in the node's colour.
    pub tint: Option<[u8; 4]>,
    /// The logical size this glyph was set at when a run changed it; zero is
    /// the node's own size.
    pub font_size: f32,
    /// Tightly packed bitmap bytes.
    ///
    /// Shared rather than owned. The atlas reads these only on a miss — once a
    /// glyph is uploaded, every later frame finds it by key and never looks —
    /// but the bytes were copied out of the cache on every frame regardless,
    /// which for distance fields is a several-kilobyte memcpy per visible glyph
    /// per frame to hand over something nobody reads.
    pub data: Rc<Vec<u8>>,
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl TextSystem {
    /// Loads the system font database and initializes empty caches.
    pub fn new() -> Self {
        let mut fonts = FontSystem::new();
        configure_generic_families(&mut fonts);
        let mut system = Self {
            fonts,
            glyphs: SwashCache::new(),
            buffers: FastMap::default(),
            field_pairs: FastMap::default(),
            outline_keys: FastMap::default(),
            font_sources: HashSet::new(),
            fields: FastMap::default(),
            terminal: terminal::TerminalCache::default(),
            variations: variations::Variations::default(),
            varied_field_keys: HashSet::new(),
            varied_fields: 0,
        };
        if let Some(paths) = std::env::var_os("MORF_FONT_PATH") {
            for path in std::env::split_paths(&paths) {
                let _ = system.load_font_path(path);
            }
        }
        system
    }

    /// Loads one font file or every font below a directory.
    pub fn load_font_path(&mut self, path: impl AsRef<Path>) -> io::Result<usize> {
        let path = path.as_ref();
        let before = self.fonts.db().len();
        if path.is_dir() {
            self.fonts.db_mut().load_fonts_dir(path);
        } else {
            self.fonts.db_mut().load_font_file(path)?;
        }
        let loaded = self.fonts.db().len().saturating_sub(before);
        if loaded > 0 {
            configure_generic_families(&mut self.fonts);
            self.buffers.clear();
            self.terminal = terminal::TerminalCache::default();
        }
        Ok(loaded)
    }

    /// Reports whether an exact family name exists in the loaded database.
    pub fn has_family(&self, family: &str) -> bool {
        installed_family(&self.fonts, family).is_some()
    }

    /// Resolves a family stack to an installed family or a generic fallback.
    pub fn resolved_family(&self, family: &str) -> String {
        resolve_family(&self.fonts, family).name().to_owned()
    }

    fn load_font_source(&mut self, source: Option<&str>) {
        let Some(source) = source.filter(|source| !source.is_empty()) else {
            return;
        };
        if self.font_sources.insert(source.to_owned()) {
            let path = source.strip_prefix("file://").unwrap_or(source);
            let _ = self.load_font_path(path);
        }
    }

    /// Returns the shaped buffer retained for a text node.
    pub fn buffer(&self, node: NodeHandle) -> Option<&Buffer> {
        self.buffers
            .get(&BufferKey::own(node))
            .map(|cached| &cached.buffer)
    }

    /// Provides mutable access to the glyph rasterization cache and font database.
    pub fn rasterizer(&mut self) -> (&mut FontSystem, &mut SwashCache) {
        (&mut self.fonts, &mut self.glyphs)
    }

    /// Drops the shaped buffer belonging to a removed scene node.
    pub fn remove(&mut self, node: NodeHandle) {
        self.buffers.remove(&BufferKey::own(node));
        self.buffers.remove(&BufferKey::target(node));
    }
}
pub(crate) fn resolve_family(fonts: &FontSystem, requested: &str) -> ResolvedFamily {
    for candidate in requested
        .split(',')
        .map(clean_family)
        .filter(|name| !name.is_empty())
    {
        let generic = match candidate.to_ascii_lowercase().as_str() {
            "serif" => Some(ResolvedFamily::Serif),
            "sans-serif" | "sans serif" | "sans" => Some(ResolvedFamily::SansSerif),
            "monospace" | "mono" => Some(ResolvedFamily::Monospace),
            "cursive" => Some(ResolvedFamily::Cursive),
            "fantasy" => Some(ResolvedFamily::Fantasy),
            _ => None,
        };
        if let Some(generic) = generic {
            return generic;
        }
        if let Some(installed) = installed_family(fonts, candidate) {
            return ResolvedFamily::Name(installed);
        }
    }
    if looks_monospace(requested) {
        ResolvedFamily::Monospace
    } else {
        ResolvedFamily::SansSerif
    }
}

fn clean_family(family: &str) -> &str {
    family
        .trim()
        .trim_matches(|character| character == '\'' || character == '"')
}

fn installed_family(fonts: &FontSystem, requested: &str) -> Option<String> {
    fonts.db().faces().find_map(|face| {
        face.families
            .iter()
            .find(|(family, _)| family.eq_ignore_ascii_case(requested))
            .map(|(family, _)| family.clone())
    })
}

fn looks_monospace(family: &str) -> bool {
    let family = family.to_ascii_lowercase();
    family.contains("mono")
        || family.contains("iosevka")
        || family.contains("terminal")
        || family.contains("typewriter")
        || family.contains("code")
}

/// The family fontconfig picks for a generic name (`sans-serif`), when it is
/// installed where this font system can see it.
///
/// fontconfig is where a desktop says which face "sans-serif" means (the
/// person's choice, their distribution's default); fontdb has no such idea
/// and falls back to a fixed list, so a shell drew in a different face from
/// every other application. `fc-match` is asked once, and not waited on for
/// long: a missing or hung fontconfig leaves the fixed list in charge.
fn fontconfig_family(fonts: &FontSystem, generic: &str) -> Option<String> {
    // Asked once per process: every font system (a test runner makes one per
    // test) would otherwise start fontconfig again, and one started with a
    // fresh cache directory rescans every installed font before it answers.
    let answers = fontconfig_answers();
    let index = match generic {
        "sans-serif" => 0,
        "serif" => 1,
        _ => 2,
    };
    installed_family(fonts, answers[index].as_deref()?.trim())
}

static FONTCONFIG_ANSWERS: std::sync::OnceLock<[Option<String>; 3]> = std::sync::OnceLock::new();

fn fontconfig_answers() -> &'static [Option<String>; 3] {
    FONTCONFIG_ANSWERS.get_or_init(|| ["sans-serif", "serif", "monospace"].map(ask_fontconfig))
}

/// Asks fontconfig for the generic families now, under the environment as it
/// is. A process about to point `XDG_CACHE_HOME` somewhere empty (a test
/// runner isolating a configuration) calls this first, so fontconfig answers
/// from the person's cache instead of rescanning every font into the new one.
pub fn warm_font_preferences() {
    let _ = fontconfig_answers();
    let _ = subpixel::font_subpixel();
}

/// What `fc-match` names for one generic family, or nothing when it is
/// missing or slow.
fn ask_fontconfig(generic: &str) -> Option<String> {
    fc_match(&["-f", "%{family[0]}", generic])
}

/// `fc-match` with `args`, its answer trimmed, or nothing when it is
/// missing, slow or says nothing.
pub(crate) fn fc_match(args: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut child = Command::new("fc-match")
        .args(args)
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A few seconds at most, once: a large font collection behind an
    // unwarmed cache takes fc-match past a second, and giving up then drew
    // the shell in the wrong face and without subpixel text.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut name = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut()?, &mut name).ok()?;
    Some(name.trim().to_owned()).filter(|name| !name.is_empty())
}

fn configure_generic_families(fonts: &mut FontSystem) {
    let sans = fontconfig_family(fonts, "sans-serif").or_else(|| {
        preferred_family(
            fonts,
            &[
                "Noto Sans",
                "DejaVu Sans",
                "Liberation Sans",
                "Cantarell",
                "Nimbus Sans",
            ],
            |monospaced| !monospaced,
        )
    });
    let serif = fontconfig_family(fonts, "serif").or_else(|| {
        preferred_family(
            fonts,
            &[
                "Noto Serif",
                "DejaVu Serif",
                "Liberation Serif",
                "Nimbus Roman",
            ],
            |monospaced| !monospaced,
        )
    });
    let monospace = fontconfig_family(fonts, "monospace").or_else(|| {
        preferred_family(
            fonts,
            &[
                "Noto Sans Mono",
                "DejaVu Sans Mono",
                "Liberation Mono",
                "Nimbus Mono PS",
            ],
            |monospaced| monospaced,
        )
    });
    let db = fonts.db_mut();
    if let Some(family) = sans {
        db.set_sans_serif_family(family);
    }
    if let Some(family) = serif {
        db.set_serif_family(family);
    }
    if let Some(family) = monospace {
        db.set_monospace_family(family);
    }
}

fn preferred_family(
    fonts: &FontSystem,
    preferred: &[&str],
    fallback: impl Fn(bool) -> bool,
) -> Option<String> {
    preferred
        .iter()
        .find_map(|family| installed_family(fonts, family))
        .or_else(|| {
            fonts.db().faces().find_map(|face| {
                fallback(face.monospaced)
                    .then(|| face.families.first().map(|(family, _)| family.clone()))
                    .flatten()
            })
        })
}

pub(crate) fn normalize_font_weight(weight: f64) -> u16 {
    if weight.is_finite() {
        weight.round().clamp(100.0, 900.0) as u16
    } else {
        400
    }
}

mod caret;
mod edit;
mod elide;
mod families;
mod glyph_fields;
#[cfg(test)]
mod glyph_fields_reference;
mod glyph_morph;
mod glyph_steps;
pub use caret::{CaretLine, CaretMap, CaretRect, SpanRect};
pub use edit::{DEFAULT_HISTORY, EditBuffer};
pub mod fuzzy;
pub use families::{AxisRange, family_axes, family_files, file_axes, installed_families};
pub use glyph_morph::CONTOUR_POINTS as GLYPH_CONTOUR_POINTS;
/// A closed loop of an outline, for a caller pairing letters with shapes that
/// are not letters.
pub use morf_vector::Contour;
mod glyph_runs;
mod measure;
pub(crate) use elide::elided_text;
mod raster_glyph;
mod rich;
pub use rich::{LinkRect, SpanBand, SpanLine};
mod style;
pub use style::LineBand;
mod subpixel;
pub use subpixel::{FontSubpixel, LcdFilter, SubpixelOrder, font_subpixel};
mod terminal;
mod variations;
pub use terminal::{CellFace, CellText, drawn_here};

pub use glyph_fields::{
    FIELD_REFERENCE_PX as GLYPH_FIELD_REFERENCE_PX, FIELD_SPREAD_PX as GLYPH_FIELD_SPREAD_PX,
    field_units_per_logical_px,
};

#[cfg(test)]
mod axes_shaping_tests;
#[cfg(test)]
mod caret_tests;
#[cfg(test)]
mod probe_tests;
#[cfg(test)]
mod rich_tests;
#[cfg(test)]
mod style_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod variations_tests;
