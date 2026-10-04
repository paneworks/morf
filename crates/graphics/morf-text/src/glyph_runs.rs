// Turning a node's shaped buffer into positioned glyphs.
//
// Shaping says which glyphs and where; this walks that layout and asks for each
// one as either a distance field or a direct rasterization. The pairing for a
// morph lives here too, because a pair is two runs read side by side.

use cosmic_text::PhysicalGlyph;
use morf_scene::NodeHandle;

use crate::glyph_morph::{Contour, contour_points, contours, pair_up, walk};
use crate::raster_glyph::field_raster;
use crate::{BufferKey, FastMap, GlyphPair, RasterGlyph, TextSystem};

#[cfg(test)]
mod probes;

impl TextSystem {
    /// Rasterizes one cached text node at a physical origin and scale.
    /// The glyphs of a laid-out node, positioned.
    ///
    /// `field` asks for distance-field glyphs rather than direct
    /// rasterizations. See `raster_glyph` for when that
    /// is the right thing to want; for ordinary text at its own size it is not.
    pub fn rasterize(
        &mut self,
        node: NodeHandle,
        origin: (f32, f32),
        scale: f32,
        field: bool,
    ) -> Vec<RasterGlyph> {
        self.rasterize_run(BufferKey::own(node), origin, scale, field)
    }

    /// The glyphs of the text a node is morphing *towards*, positioned the same
    /// way. Empty when the node is not morphing, because nothing shaped it.
    pub fn rasterize_target(
        &mut self,
        node: NodeHandle,
        origin: (f32, f32),
        scale: f32,
        field: bool,
    ) -> Vec<RasterGlyph> {
        self.rasterize_run(BufferKey::target(node), origin, scale, field)
    }

    /// A node's glyphs, each with the shape it is part way towards.
    ///
    /// What comes back is not the two letters: it is the two *frames* of the
    /// morph either side of `travel`, and how far between them the glyph is.
    /// The correspondence between the letters was solved in the outline when
    /// the frames were measured, so all that is left here is to pick a pair
    /// that already differ by almost nothing.
    pub fn rasterize_pairs(
        &mut self,
        node: NodeHandle,
        origin: (f32, f32),
        scale: f32,
        travel: f32,
    ) -> Vec<GlyphPair> {
        let own = self.physical_glyphs(BufferKey::own(node), origin, scale);
        let target = self.physical_glyphs(BufferKey::target(node), origin, scale);
        // Every pair's frames for this step, measured together first, so the
        // loop below only ever finds them ready.
        let mut wanted = Vec::with_capacity(own.len());
        for (glyph, partner) in own.iter().zip(target.iter()) {
            if let Some(from_key) = self.morph_frames(glyph, partner) {
                wanted.push((from_key, Self::pair_target_key(partner)));
            }
        }
        self.prepare_morph_frames(&wanted, travel);
        let mut target = target.into_iter();
        own.into_iter()
            .map(|glyph| {
                let Some(partner) = target.next() else {
                    return (self.raster_glyph(&glyph, true), None, 0.0);
                };
                let Some(from_key) = self.morph_frames(&glyph, &partner) else {
                    return (self.raster_glyph(&glyph, true), None, 0.0);
                };
                let to_key = Self::pair_target_key(&partner);
                match self.morph_step(from_key, to_key, travel) {
                    Some((first, first_key, second, second_key, local)) => (
                        Some(field_raster(&glyph, first_key, &first)),
                        Some(field_raster(&glyph, second_key, &second)),
                        local,
                    ),
                    None => (self.raster_glyph(&glyph, true), None, 0.0),
                }
            })
            .filter_map(|(glyph, partner, local)| Some((glyph?, partner, local)))
            .filter(|(glyph, _, _)| glyph.width > 0 && glyph.height > 0)
            .collect()
    }

    /// The glyphs of a node, each with the text offset its cluster starts
    /// at, so a caller can tell which of them a selection covers.
    pub fn rasterize_at(
        &mut self,
        node: NodeHandle,
        origin: (f32, f32),
        scale: f32,
    ) -> Vec<(RasterGlyph, usize)> {
        let Some(cached) = self.buffers.get(&BufferKey::own(node)) else {
            return Vec::new();
        };
        let axes = cached.axes.clone();
        crate::style::physical_glyphs_at(cached, origin, scale)
            .into_iter()
            .filter_map(|(glyph, offset, tint)| {
                let mut raster = self.raster_glyph_in(&glyph, true, &axes)?;
                raster.tint = tint;
                Some((raster, offset))
            })
            .collect()
    }

    fn physical_glyphs(
        &mut self,
        key: BufferKey,
        origin: (f32, f32),
        scale: f32,
    ) -> Vec<PhysicalGlyph> {
        let Some(cached) = self.buffers.get(&key) else {
            return Vec::new();
        };
        crate::style::physical_glyphs(cached, origin, scale)
    }

    fn rasterize_run(
        &mut self,
        key: BufferKey,
        origin: (f32, f32),
        scale: f32,
        field: bool,
    ) -> Vec<RasterGlyph> {
        let Some(cached) = self.buffers.get(&key) else {
            return Vec::new();
        };
        let physical = crate::style::physical_glyphs_styled(cached, origin, scale);
        let axes = cached.axes.clone();
        let optical = cached.optical;
        // A run set at a size of its own was shaped at that size's `opsz`.
        let mut sized = axes.clone();
        physical
            .into_iter()
            .filter_map(|(glyph, tint, font_size)| {
                let axes = if optical && font_size > 0.0 {
                    for axis in &mut sized {
                        if &axis.tag == b"opsz" {
                            axis.value = font_size;
                        }
                    }
                    &sized
                } else {
                    &axes
                };
                let mut raster = self.raster_glyph_in(&glyph, field, axes)?;
                raster.tint = tint;
                raster.font_size = font_size;
                Some(raster)
            })
            .collect()
    }
}

impl TextSystem {
    /// One character's outline as points, optionally part way to another.
    ///
    /// This is how a letter becomes a shape a distance field can compose with.
    /// It is not a picture of a letter sampled from an atlas — it is the
    /// outline itself, so it unions, subtracts and morphs with a circle by the
    /// same arithmetic a circle does, at whatever size it is drawn.
    ///
    /// A morphing pair is walked here rather than in the shader: the
    /// correspondence between the two letters is a property of the outlines and
    /// costs a few hundred multiplications to apply, so what reaches the GPU is
    /// one outline and a morphing letter costs a still one's price.
    pub fn glyph_outline(
        &mut self,
        glyph: char,
        morph_to: Option<char>,
        travel: f32,
        family: &str,
        family_to: &str,
    ) -> Vec<(f32, f32)> {
        let Some(from) = self.outline_points(glyph, family) else {
            return Vec::new();
        };
        // The two ends need not be the same face. Correspondence is geometry —
        // contours paired by position, resampled, rotated onto each other — so
        // one face's letter walks onto another's the same way it walks onto its
        // own. A face change is a morph, not a swap.
        // Not skipped at travel 0: the paired outline has its own point
        // count, and a morph that started from the raw one would change how
        // many points it has on its first frame.
        let target = morph_to
            .filter(|other| *other != glyph || family_to != family)
            .and_then(|other| self.outline_points(other, family_to));
        match target {
            Some(to) => walk(&pair_up(from, to), travel.clamp(0.0, 1.0)),
            None => contour_points(&from),
        }
    }

    /// The cache key one character's outline is measured under.
    ///
    /// A character has to be shaped before a font can be asked for its outline,
    /// and shaping wants a buffer. This keeps one for the purpose rather than
    /// borrowing a node's, since a letter used as a shape belongs to no text.
    fn outline_key(&mut self, glyph: char, family: &str) -> Option<cosmic_text::CacheKey> {
        if let Some(known) = self.outline_keys.get(family).and_then(|by| by.get(&glyph)) {
            return *known;
        }
        let key = self.shape_one_in(glyph, family);
        if let Some(by_glyph) = self.outline_keys.get_mut(family) {
            by_glyph.insert(glyph, key);
        } else {
            let mut by_glyph = FastMap::default();
            by_glyph.insert(glyph, key);
            self.outline_keys.insert(family.into(), by_glyph);
        }
        key
    }

    fn shape_one_in(&mut self, glyph: char, family: &str) -> Option<cosmic_text::CacheKey> {
        let size = crate::glyph_fields::FIELD_REFERENCE_PX;
        // One character on one line: the line height is nothing to it.
        let mut buffer =
            cosmic_text::Buffer::new(&mut self.fonts, cosmic_text::Metrics::new(size, size));
        // A weight rides after a NUL in the family (see morf-render's
        // `weighted`): the letter is cut from that weight of the face.
        let (family, weight) = match family.split_once('\0') {
            Some((name, weight)) => (name, weight.parse::<u16>().ok()),
            None => (family, None),
        };
        let family = crate::resolve_family(&self.fonts, family);
        let mut attrs = cosmic_text::Attrs::new().family(family.family());
        if let Some(weight) = weight {
            attrs = attrs.weight(cosmic_text::Weight(weight));
        }
        buffer.set_text(
            glyph.encode_utf8(&mut [0u8; 4]),
            &attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut key = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| glyph.physical((0.0, 0.0), 1.0).cache_key)
            .next()?;
        key.font_size_bits = size.to_bits();
        key.x_bin = cosmic_text::SubpixelBin::Zero;
        key.y_bin = cosmic_text::SubpixelBin::Zero;
        Some(key)
    }

    /// One letter's closed loops, for a caller pairing them with something that
    /// is not a letter.
    ///
    /// A drawing morphing into an `S` is the same arithmetic as one letter
    /// morphing into another — the correspondence is geometry and does not ask
    /// where either outline came from — but the two sides are cached in
    /// different places, so whoever holds both has to be handed the loops.
    pub fn glyph_contours(&mut self, glyph: char, family: &str) -> Vec<Contour> {
        self.outline_points(glyph, family).unwrap_or_default()
    }

    fn outline_points(&mut self, glyph: char, family: &str) -> Option<Vec<Contour>> {
        let key = self.outline_key(glyph, family)?;
        let commands = self.glyphs.get_outline_commands(&mut self.fonts, key)?;
        let found = contours(commands);
        (!found.is_empty()).then_some(found)
    }
}
