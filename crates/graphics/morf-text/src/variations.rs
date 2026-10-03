// Glyphs of a variable font set at axes of the configuration's choosing.
//
// Upstream cosmic-text shapes and rasterises a variable font at one point of
// its design space: the default on every axis but `wght`, which it takes from
// the weight. A text node's `axes` reach further -- `FILL`, `GRAD`, `opsz`,
// `wdth`, anything a font defines -- and so does optical sizing, which sets
// `opsz` from the size. The vendored cosmic-text shapes a run at those axes
// (`Attrs::font_variations`), so an axis that changes advances changes the
// layout; the glyphs are rasterised and their outlines measured here, with
// the font's own swash scaler, at the point the shaper used: both ask
// `Font::variation_coords`.
//
// The atlas and the field cache key on the point in design space as well as on
// the glyph, so that point is quantised: in normalized units (-1 to 1 across
// each axis, whatever its range in the font's own), to 1/64 of the way from
// the default to either end. An animation of `FILL` from 0 to 1 then asks for
// at most 65 pictures of each glyph however many frames it takes, and each
// step is a quarter of a percent of the axis -- nothing an eye can see.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use cosmic_text::{CacheKey, CacheKeyFlags, SwashContent, SwashImage};
use morf_layout::FontAxis;
use morf_scene::FastMap;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Angle, Command, Format, Transform, Vector};

/// Pictures and outlines kept at most, each; past it the cache starts over.
/// An animation through the whole of an axis asks for 65 per glyph, so this
/// is a few thousand glyph-steps -- a screenful of icons moving at once.
const CAPACITY: usize = 4096;

/// Distance fields measured at chosen points kept at most before they are
/// all let go of. A field is a few kilobytes; this is a few megabytes.
pub(crate) const FIELD_CAPACITY: usize = 2048;

/// A point in one face's design space, quantised: normalized coordinates in
/// the face's axis order.
pub(crate) type Coords = Rc<[i16]>;

/// The rasteriser and caches for glyphs at a chosen point in design space.
#[derive(Default)]
pub(crate) struct Variations {
    context: ScaleContext,
    /// The quantised coordinates of a face for a node's axes, by face and
    /// axes: asked once per glyph per frame, worked out once.
    coords: FastMap<u64, Option<Coords>>,
    images: FastMap<u64, Option<Rc<SwashImage>>>,
    outlines: FastMap<u64, Option<Rc<[Command]>>>,
}

/// Axes as the shaper takes them.
pub(crate) fn font_variations(axes: &[FontAxis]) -> cosmic_text::FontVariations {
    let mut variations = cosmic_text::FontVariations::new();
    for axis in axes {
        variations.set(axis.tag, axis.value);
    }
    variations
}

/// A key for a glyph at a point: its own key and the point.
pub(crate) fn varied_key(key: &CacheKey, coords: &[i16]) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    coords.hash(&mut hasher);
    0x5641_5249_u64.hash(&mut hasher);
    hasher.finish()
}

impl Variations {
    /// Where `axes` put the face a glyph is set in, or `None` when the face
    /// has no axes, or none of these: the ordinary rasteriser draws it then.
    pub(crate) fn coords(
        &mut self,
        fonts: &mut cosmic_text::FontSystem,
        key: &CacheKey,
        axes: &[FontAxis],
    ) -> Option<Coords> {
        let mut hasher = DefaultHasher::new();
        key.font_id.hash(&mut hasher);
        key.font_weight.hash(&mut hasher);
        for axis in axes {
            axis.tag.hash(&mut hasher);
            axis.value.to_bits().hash(&mut hasher);
        }
        let id = hasher.finish();
        if let Some(known) = self.coords.get(&id) {
            return known.clone();
        }
        if self.coords.len() >= CAPACITY {
            self.coords.clear();
        }
        // The shaper asks the same question of the same face, so the glyphs
        // are drawn at exactly the point their advances were measured at.
        let coords = fonts
            .get_font(key.font_id, key.font_weight)
            .and_then(|font| font.variation_coords(key.font_weight, &font_variations(axes)))
            .map(Rc::from);
        self.coords.insert(id, coords.clone());
        coords
    }

    /// The glyph rasterised at its size and subpixel offset, at `coords`.
    pub(crate) fn image(
        &mut self,
        fonts: &mut cosmic_text::FontSystem,
        key: &CacheKey,
        coords: &[i16],
    ) -> Option<Rc<SwashImage>> {
        let id = varied_key(key, coords);
        if let Some(known) = self.images.get(&id) {
            return known.clone();
        }
        if self.images.len() >= CAPACITY {
            self.images.clear();
        }
        let image = fonts
            .get_font(key.font_id, key.font_weight)
            .and_then(|font| {
                let mut scaler = self
                    .context
                    .builder(font.as_swash())
                    .size(f32::from_bits(key.font_size_bits))
                    .hint(!key.flags.contains(CacheKeyFlags::DISABLE_HINTING))
                    .normalized_coords(coords.iter().copied())
                    .build();
                let offset = if key.flags.contains(CacheKeyFlags::PIXEL_FONT) {
                    Vector::new(key.x_bin.as_float().round(), key.y_bin.as_float().round())
                } else {
                    Vector::new(key.x_bin.as_float(), key.y_bin.as_float())
                };
                Render::new(&[
                    Source::ColorOutline(0),
                    Source::ColorBitmap(StrikeWith::BestFit),
                    Source::Outline,
                ])
                .format(Format::Alpha)
                .offset(offset)
                .transform(fake_italic(key))
                .render(&mut scaler, key.glyph_id)
                .map(Rc::new)
            });
        self.images.insert(id, image.clone());
        image
    }

    /// The glyph's outline at its size, at `coords`.
    pub(crate) fn outline(
        &mut self,
        fonts: &mut cosmic_text::FontSystem,
        key: &CacheKey,
        coords: &[i16],
    ) -> Option<Rc<[Command]>> {
        let id = varied_key(key, coords);
        if let Some(known) = self.outlines.get(&id) {
            return known.clone();
        }
        if self.outlines.len() >= CAPACITY {
            self.outlines.clear();
        }
        let outline = fonts
            .get_font(key.font_id, key.font_weight)
            .and_then(|font| {
                use swash::zeno::PathData as _;
                let mut scaler = self
                    .context
                    .builder(font.as_swash())
                    .size(f32::from_bits(key.font_size_bits))
                    .hint(!key.flags.contains(CacheKeyFlags::DISABLE_HINTING))
                    .normalized_coords(coords.iter().copied())
                    .build();
                let mut outline = scaler
                    .scale_outline(key.glyph_id)
                    .or_else(|| scaler.scale_color_outline(key.glyph_id))?;
                if let Some(transform) = fake_italic(key) {
                    outline.transform(&transform);
                }
                Some(outline.path().commands().collect::<Rc<[Command]>>())
            });
        self.outlines.insert(id, outline.clone());
        outline
    }
}

fn fake_italic(key: &CacheKey) -> Option<Transform> {
    key.flags
        .contains(CacheKeyFlags::FAKE_ITALIC)
        .then(|| Transform::skew(Angle::from_degrees(14.0), Angle::from_degrees(0.0)))
}

/// Whether an image holds colour rather than coverage.
pub(crate) fn is_color(image: &SwashImage) -> bool {
    matches!(image.content, SwashContent::Color)
}
