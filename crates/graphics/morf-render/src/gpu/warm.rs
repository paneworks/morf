//! Getting hidden text ready to draw before it is shown.
//!
//! The first frame a piece of text appears in pays for its glyphs: each one
//! is turned into a distance field from its outline and copied into the
//! atlas. For a panel of a few hundred labels that is tens of milliseconds,
//! and it lands on the frame the panel opens in -- the one frame a person is
//! watching. Text that is built and laid out but hidden (a `Loader` holding
//! a preloaded item, say) is already shaped; this does the rest of the work
//! for it while the shell is idle, so that frame finds its glyphs waiting.

use std::collections::HashSet;

use morf_layout::Layout;
use morf_scene::{Element, NodeHandle, Scene};
use morf_text::RasterGlyph;

use super::backend_types::*;
use super::glyphs::{GlyphAtlas, GlyphKey, glyph_pixels, outside_byte, upload_glyph};

impl GlyphAtlas {
    /// Puts glyphs in the atlas without drawing them, while there is room.
    ///
    /// Unlike a frame's own glyphs these are not needed yet, so nothing that
    /// is already there is evicted or moved for them: once the atlas is full,
    /// the rest wait for the frame that draws them. Returns how many were
    /// added.
    pub(crate) fn warm(&mut self, queue: &wgpu::Queue, glyphs: &[RasterGlyph]) -> usize {
        let mut seen = HashSet::new();
        let mut added = 0;
        for glyph in glyphs {
            if !self.accepts(glyph.content) || glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let key = GlyphKey::from_glyph(glyph);
            if !seen.insert(key) || self.entries.contains_key(&key) {
                continue;
            }
            let Some((x, y)) = self.allocator.allocate(key.width, key.height) else {
                break;
            };
            let entry = super::glyphs::GlyphAtlasEntry {
                x,
                y,
                width: key.width,
                height: key.height,
                // As old as anything in the atlas: a glyph nobody has drawn
                // yet is the first to make room for one somebody has.
                last_used: 0,
                pixels: glyph_pixels(glyph),
                outside: outside_byte(glyph.content),
            };
            upload_glyph(queue, &self.texture, &entry, self.bytes_per_pixel);
            self.entries.insert(key, entry);
            added += 1;
        }
        added
    }
}

impl WgpuBackend {
    /// Prepares the glyphs of text under `root` that is laid out but hidden,
    /// at `scale_120`, so the frame that shows it does not have to.
    ///
    /// Hidden means an ancestor, or the node itself, has `visible` false;
    /// text that is shown is drawn, and so prepared, by the frame anyway.
    /// Returns how many glyphs went into the atlas. Cheap to repeat: a text
    /// whose glyphs are all there already costs a lookup a glyph.
    pub fn warm_hidden_text(
        &mut self,
        scene: &Scene,
        layout: &Layout,
        root: NodeHandle,
        scale_120: u32,
    ) -> usize {
        // It runs between frames, when nothing moves: a turn of it must not
        // become the reason the next frame is late. What does not fit in the
        // budget is done on the next quiet turn.
        const BUDGET: std::time::Duration = std::time::Duration::from_millis(3);
        let started = std::time::Instant::now();
        let scale = scale_120.max(1) as f32 / 120.0;
        let mut added = 0;
        let mut pending = vec![(root, false)];
        while let Some((node, hidden)) = pending.pop() {
            // Discovery and style lookups are part of the budget too. First
            // collecting every hidden label could exhaust it before warming
            // any glyph, and still scan the whole tree on each quiet frame.
            if started.elapsed() > BUDGET {
                break;
            }
            let hidden = hidden || !scene.bool_value(node, "visible").unwrap_or(true);
            if hidden && scene.element(node) == Ok(Element::Text) && layout.geometry(node).is_some()
            {
                // Warmed already, looking as it does now: nothing to do.
                let look = text_look(scene, node, scale_120);
                if self.warmed_text.get(&node) != Some(&look) {
                    let glyphs = self.text.rasterize(node, (0.0, 0.0), scale, true);
                    added += self.glyph_mask_atlas.warm(&self.queue, &glyphs)
                        + self.glyph_color_atlas.warm(&self.queue, &glyphs);
                    self.warmed_text.insert(node, look);
                }
            }
            if let Ok(children) = scene.children(node) {
                pending.extend(children.iter().map(|&child| (child, hidden)));
            }
        }
        // Nodes gone from the scene are forgotten.
        if self.warmed_text.len() > 4096 {
            self.warmed_text
                .retain(|node, _| scene.element(*node).is_ok());
        }
        added
    }
}

/// What a text node's glyphs depend on, as one number.
fn text_look(scene: &Scene, node: NodeHandle, scale_120: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scale_120.hash(&mut hasher);
    for property in ["text", "font_family", "font_style"] {
        scene
            .string_value(node, property)
            .unwrap_or("")
            .hash(&mut hasher);
    }
    for property in ["font_size", "font_weight", "letter_spacing"] {
        scene
            .number(node, property)
            .unwrap_or(0.0)
            .to_bits()
            .hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use morf_layout::{Layout, Size};
    use morf_scene::{Element, Scene};

    use crate::*;

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn hidden_text_is_ready_before_it_is_shown() {
        let mut scene = Scene::new();
        let root = scene.create(Element::Item);
        let panel = scene.create(Element::Item);
        scene.assign(panel, "visible", false).unwrap();
        scene.reparent(panel, Some(root)).unwrap();
        let label = scene.create(Element::Text);
        scene
            .assign(label, "text", "Wi-Fi Bluetooth 12:30")
            .unwrap();
        scene.assign(label, "font_size", 18.0).unwrap();
        scene.reparent(label, Some(panel)).unwrap();
        let size = Size {
            width: 200.0,
            height: 60.0,
        };
        let backend = pollster::block_on(WgpuBackend::new(200, 60)).unwrap();
        let mut engine = RenderEngine::new(backend);
        let layout = Layout::compute(&scene, root, size, engine.backend_mut()).unwrap();
        engine.render(&scene, &layout, 120, |_| {}).unwrap();
        let before = engine.backend_mut().glyph_mask_atlas.entries.len();

        let warmed = engine
            .backend_mut()
            .warm_hidden_text(&scene, &layout, root, 120);
        assert!(warmed > 0, "the hidden label's glyphs went in");
        assert_eq!(
            engine.backend_mut().glyph_mask_atlas.entries.len(),
            before + warmed
        );
        assert_eq!(
            engine
                .backend_mut()
                .warm_hidden_text(&scene, &layout, root, 120),
            0,
            "and only once"
        );

        // Shown, it draws from what is there: nothing new is made.
        scene.assign(panel, "visible", true).unwrap();
        let layout = Layout::compute(&scene, root, size, engine.backend_mut()).unwrap();
        engine.render(&scene, &layout, 120, |_| {}).unwrap();
        assert_eq!(
            engine.backend_mut().glyph_mask_atlas.entries.len(),
            before + warmed
        );
    }
}
