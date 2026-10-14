use crate::LayerMask;
use morf_layout::{Geometry, Transform2D};
use morf_scene::Color;
use morf_text::{RasterContent, RasterGlyph};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use super::backend_types::*;
use super::glyph_upload::{atlas_format, atlas_texture};
pub(crate) use super::glyph_upload::{glyph_pixels, upload_glyph};

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, Default)]
pub(crate) struct GlyphInstance {
    pub(crate) origin: [f32; 2],
    pub(crate) axes: [f32; 4],
    pub(crate) uv: [f32; 4],
    pub(crate) color: [f32; 4],
    pub(crate) color_overlay: [f32; 4],
    pub(crate) mode: [f32; 4],
    pub(crate) surface: [f32; 4],
    pub(crate) mask_bounds: [f32; 4],
    pub(crate) mask_inverse_0: [f32; 4],
    pub(crate) mask_inverse_1: [f32; 4],
    pub(crate) mask_radii: [f32; 4],
    /// Field edge, feathering, outline width, and how far this glyph has
    /// travelled towards the one it is morphing into.
    pub(crate) field: [f32; 4],
    /// Outline colour composited beneath a distance-field fill.
    pub(crate) outline_color: [f32; 4],
    /// Atlas rect of the glyph being morphed towards.
    ///
    /// Last, and it has to stay last: the vertex attributes are laid out by
    /// offset, so a field inserted higher up would be read as whichever
    /// attribute used to sit at that offset.
    ///
    /// A zero size means there is nothing opposite this glyph — the text it is
    /// turning into is shorter — and the shader reads "outside" there instead,
    /// so an unpaired letter dissolves rather than snapping away.
    pub(crate) morph_uv: [f32; 4],
    /// How much the field changes across one device pixel.
    ///
    /// Known exactly from the size the glyph is drawn at, so the edge does not
    /// have to guess it from the gradient of a sampled texture — which is a
    /// noisy thing to measure once the field is minified, and reads as an edge
    /// that will not settle.
    pub(crate) ramp: f32,
}

pub(crate) fn layer_mask_data(
    mask: Option<LayerMask>,
) -> (f32, [f32; 4], [f32; 4], [f32; 4], [f32; 4]) {
    let Some(mask) = mask else {
        return (0.0, [0.0; 4], [0.0; 4], [0.0; 4], [0.0; 4]);
    };
    let [a, b, c, d, tx, ty] = mask.transform.matrix;
    let determinant = a * d - b * c;
    if determinant.abs() <= f64::EPSILON {
        return (0.0, [0.0; 4], [0.0; 4], [0.0; 4], [0.0; 4]);
    }
    let inverse_a = d / determinant;
    let inverse_b = -b / determinant;
    let inverse_c = -c / determinant;
    let inverse_d = a / determinant;
    let inverse_tx = -(inverse_a * tx + inverse_c * ty);
    let inverse_ty = -(inverse_b * tx + inverse_d * ty);
    (
        1.0,
        [
            mask.bounds.x as f32,
            mask.bounds.y as f32,
            mask.bounds.width as f32,
            mask.bounds.height as f32,
        ],
        [inverse_a as f32, inverse_c as f32, inverse_tx as f32, 0.0],
        [inverse_b as f32, inverse_d as f32, inverse_ty as f32, 0.0],
        mask.radii.map(|radius| radius as f32),
    )
}

pub(crate) fn transformed_quad(
    transform: Transform2D,
    bounds: Geometry,
    scale: f64,
    target_size: (u32, u32),
) -> ([f32; 2], [f32; 4]) {
    let origin = transform.point(bounds.x, bounds.y);
    let horizontal = transform.point(bounds.x + bounds.width, bounds.y);
    let vertical = transform.point(bounds.x, bounds.y + bounds.height);
    let (target_width, target_height) = target_size;
    let clip = |point: (f64, f64)| {
        [
            (point.0 * scale) as f32 / target_width as f32 * 2.0 - 1.0,
            1.0 - (point.1 * scale) as f32 / target_height as f32 * 2.0,
        ]
    };
    let origin_clip = clip(origin);
    let horizontal_clip = clip(horizontal);
    let vertical_clip = clip(vertical);
    (
        origin_clip,
        [
            horizontal_clip[0] - origin_clip[0],
            horizontal_clip[1] - origin_clip[1],
            vertical_clip[0] - origin_clip[0],
            vertical_clip[1] - origin_clip[1],
        ],
    )
}

pub(crate) struct GlyphBatch {
    pub(crate) instances: Vec<GlyphInstance>,
    pub(crate) command_spans: Vec<Vec<GlyphSpan>>,
    /// Per instance: a plain glyph from a field -- not colour, not a band,
    /// only moved, not morphing, no outline -- which subpixel text may draw.
    pub(crate) plain: Vec<bool>,
}

pub(crate) struct GlyphSpan {
    pub(crate) range: Range<u32>,
    pub(crate) color: bool,
    /// Drawn with the subpixel pipeline (lcd_spans.rs).
    pub(crate) lcd: bool,
}

pub(crate) const GLYPH_ATLAS_SIZE: u32 = 2048;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GlyphKey {
    pub(crate) id: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl GlyphKey {
    pub(crate) fn from_glyph(glyph: &RasterGlyph) -> Self {
        Self {
            id: glyph.cache_key,
            width: glyph.width,
            height: glyph.height,
        }
    }
}

impl PreparedGlyph {
    /// Every glyph this one needs in the atlas — its own, and the one it is
    /// morphing into. A partner that is never uploaded is a partner the shader
    /// samples as empty space.
    pub(crate) fn sources(&self) -> impl Iterator<Item = &RasterGlyph> {
        std::iter::once(&self.glyph).chain(self.morph.iter())
    }
}

pub(crate) struct GlyphAtlasEntry {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) last_used: u64,
    pub(crate) pixels: Vec<u8>,
    /// The byte the one-pixel gutter around this entry is filled with.
    ///
    /// Not always zero, because the atlas holds two kinds of thing that
    /// disagree about what an empty byte means. For coverage, zero is "no
    /// ink" and the gutter is simply blank. For a distance field, zero is the
    /// *inside* of the glyph — so a zeroed gutter reads as solid ink, and
    /// linear filtering at the quad's edge drags it into frame as a bright
    /// rectangle around every glyph drawn from a field.
    pub(crate) outside: u8,
}

/// The byte that means "nothing here" for a given kind of atlas content.
pub(crate) fn outside_byte(content: RasterContent) -> u8 {
    match content {
        // Furthest outside the glyph the encoded spread can express.
        RasterContent::Field => u8::MAX,
        RasterContent::Mask | RasterContent::Color => 0,
    }
}

pub(crate) struct PreparedGlyph {
    pub(crate) glyph: RasterGlyph,
    /// The glyph this one is turning into, when it has a partner.
    pub(crate) morph: Option<RasterGlyph>,
    /// How far between the two, zero at `glyph` and one at `morph`.
    ///
    /// A glyph with no partner still carries a progress: it interpolates
    /// towards the far-outside value instead, which is how a letter with
    /// nothing opposite it dissolves.
    pub(crate) morph_progress: f32,
    /// The field's change across one device pixel.
    pub(crate) ramp: f32,
    pub(crate) color: Color,
    pub(crate) color_overlay: Color,
    pub(crate) transform: Transform2D,
    pub(crate) command_index: usize,
    /// Edge, softness and outline width, already in sampled-field units.
    pub(crate) field: [f32; 4],
    /// Outline colour, composited beneath the fill.
    pub(crate) outline_color: [f32; 4],
}

#[derive(Clone, Copy, Default)]
pub(crate) struct ShelfAllocator {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) row_height: u32,
    /// How many atlas pages tall the space is; none counts as one.
    pub(crate) pages: u32,
}

impl ShelfAllocator {
    pub(crate) fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let width = width.checked_add(2)?;
        let height = height.checked_add(2)?;
        let bottom = GLYPH_ATLAS_SIZE * self.pages.max(1);
        if width > GLYPH_ATLAS_SIZE || height > GLYPH_ATLAS_SIZE {
            return None;
        }
        if self.x + width > GLYPH_ATLAS_SIZE {
            self.x = 0;
            self.y = self.y.checked_add(self.row_height)?;
            self.row_height = 0;
        }
        if self.y + height > bottom {
            return None;
        }
        let placement = (self.x + 1, self.y + 1);
        self.x += width;
        self.row_height = self.row_height.max(height);
        Some(placement)
    }
}

pub(crate) struct GlyphAtlas {
    pub(crate) texture: wgpu::Texture,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) content: RasterContent,
    pub(crate) bytes_per_pixel: u32,
    pub(crate) entries: HashMap<GlyphKey, GlyphAtlasEntry>,
    pub(crate) allocator: ShelfAllocator,
    pub(crate) clock: u64,
    /// How many pages tall the texture is: one, or two once the first filled.
    pub(crate) pages: u32,
    device: wgpu::Device,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

/// The most pages an atlas grows to before it is rebuilt instead.
const GLYPH_ATLAS_PAGES: u32 = 2;

impl GlyphAtlas {
    /// Whether this atlas is the one a glyph of that kind belongs in.
    ///
    /// Coverage and distance are both one byte a pixel, so they share the
    /// single-channel atlas and differ only in how the shader reads them;
    /// colour glyphs need four and have their own.
    pub(crate) fn accepts(&self, content: RasterContent) -> bool {
        match self.content {
            RasterContent::Mask | RasterContent::Field => {
                matches!(content, RasterContent::Mask | RasterContent::Field)
            }
            RasterContent::Color => content == RasterContent::Color,
        }
    }

    pub(crate) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        content: RasterContent,
    ) -> Self {
        let (texture, bind_group) = atlas_texture(device, layout, sampler, content, 1);
        Self {
            texture,
            bind_group,
            content,
            bytes_per_pixel: atlas_format(content).1,
            entries: HashMap::new(),
            allocator: ShelfAllocator::default(),
            clock: 0,
            pages: 1,
            device: device.clone(),
            layout: layout.clone(),
            sampler: sampler.clone(),
        }
    }

    /// The texture's height in texels, which atlas coordinates are read over.
    pub(crate) fn height(&self) -> u32 {
        GLYPH_ATLAS_SIZE * self.pages
    }

    /// Adds a second page below the first, keeping every entry where it is.
    ///
    /// CJK or emoji text can want more glyphs at once than one page holds;
    /// rebuilding then evicts and re-uploads on every frame. Growing once is
    /// cheaper, and every entry keeps its pixels to upload again.
    fn grow(&mut self, queue: &wgpu::Queue) -> bool {
        let pages = self.pages + 1;
        if pages > GLYPH_ATLAS_PAGES
            || GLYPH_ATLAS_SIZE * pages > self.device.limits().max_texture_dimension_2d
        {
            return false;
        }
        let (texture, bind_group) = atlas_texture(
            &self.device,
            &self.layout,
            &self.sampler,
            self.content,
            pages,
        );
        for entry in self.entries.values() {
            upload_glyph(queue, &texture, entry, self.bytes_per_pixel);
        }
        self.texture = texture;
        self.bind_group = bind_group;
        self.pages = pages;
        self.allocator.pages = pages;
        true
    }

    /// Places every missing glyph, or none of them.
    fn place(&mut self, missing: &[(GlyphKey, Vec<u8>, u8)]) -> Option<Vec<(u32, u32)>> {
        let mut allocator = self.allocator;
        let placements = missing
            .iter()
            .map(|(key, _, _)| allocator.allocate(key.width, key.height))
            .collect::<Option<Vec<_>>>()?;
        self.allocator = allocator;
        Some(placements)
    }
}

impl GlyphAtlas {
    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        glyphs: &[PreparedGlyph],
    ) -> Result<(), GpuError> {
        self.clock = self.clock.wrapping_add(1);
        let mut requested = HashSet::new();
        let mut missing = Vec::new();
        for glyph in glyphs.iter().flat_map(PreparedGlyph::sources) {
            if !self.accepts(glyph.content) {
                continue;
            }
            let key = GlyphKey::from_glyph(glyph);
            if !requested.insert(key) {
                continue;
            }
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.last_used = self.clock;
            } else {
                missing.push((key, glyph_pixels(glyph), outside_byte(glyph.content)));
            }
        }
        let placements = match self.place(&missing) {
            Some(placements) => placements,
            None => match self.grow(queue).then(|| self.place(&missing)).flatten() {
                Some(placements) => placements,
                None => return self.rebuild(queue, glyphs, &requested),
            },
        };
        for ((key, pixels, outside), (x, y)) in missing.into_iter().zip(placements) {
            let entry = GlyphAtlasEntry {
                x,
                y,
                width: key.width,
                height: key.height,
                last_used: self.clock,
                pixels,
                outside,
            };
            upload_glyph(queue, &self.texture, &entry, self.bytes_per_pixel);
            self.entries.insert(key, entry);
        }
        Ok(())
    }

    pub(crate) fn rebuild(
        &mut self,
        queue: &wgpu::Queue,
        glyphs: &[PreparedGlyph],
        requested: &HashSet<GlyphKey>,
    ) -> Result<(), GpuError> {
        let old = std::mem::take(&mut self.entries);
        let mut requested_entries = Vec::new();
        let mut seen = HashSet::new();
        for glyph in glyphs.iter().flat_map(PreparedGlyph::sources) {
            if !self.accepts(glyph.content) {
                continue;
            }
            let key = GlyphKey::from_glyph(glyph);
            if !seen.insert(key) {
                continue;
            }
            let pixels = old
                .get(&key)
                .map_or_else(|| glyph_pixels(glyph), |entry| entry.pixels.clone());
            requested_entries.push((key, pixels, outside_byte(glyph.content)));
        }
        let mut retained: Vec<_> = old
            .into_iter()
            .filter(|(key, _)| !requested.contains(key))
            .collect();
        retained.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.last_used));
        self.allocator = ShelfAllocator {
            pages: self.pages,
            ..ShelfAllocator::default()
        };
        for (key, pixels, outside) in requested_entries {
            let Some((x, y)) = self.allocator.allocate(key.width, key.height) else {
                return Err(GpuError(
                    "visible glyphs exceed the persistent atlas capacity".to_owned(),
                ));
            };
            let entry = GlyphAtlasEntry {
                x,
                y,
                width: key.width,
                height: key.height,
                last_used: self.clock,
                pixels,
                outside,
            };
            upload_glyph(queue, &self.texture, &entry, self.bytes_per_pixel);
            self.entries.insert(key, entry);
        }
        for (key, mut entry) in retained {
            let Some((x, y)) = self.allocator.allocate(key.width, key.height) else {
                continue;
            };
            entry.x = x;
            entry.y = y;
            upload_glyph(queue, &self.texture, &entry, self.bytes_per_pixel);
            self.entries.insert(key, entry);
        }
        Ok(())
    }
}
