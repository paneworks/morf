//! Field instances and glyph quads read from the last frame for the
//! commands a list copied, against the same list's built afresh: the same
//! buffers, every frame, on random scenes under random changes -- and, on a
//! GPU, the same pixels.

use morf_layout::{Layout, Size};
use morf_scene::{Element, NodeHandle, Scene, Value};
use morf_text::{RasterContent, TextSystem};
use morf_vector::svg::SvgOutlines;
use std::collections::HashMap;

use super::batches::{FieldBatch, KeptFields, collect_field_instances};
use super::glyph_batch::{AtlasView, KeptText, font_count, glyph_quads, lay_out_text};
use super::glyphs::{GLYPH_ATLAS_SIZE, GlyphAtlasEntry, GlyphBatch, GlyphKey, PreparedGlyph};
use crate::tests::RecordingBackend;
use crate::tests::reuse::{Rng, make, mutate};
use crate::{RenderEngine, WgpuBackend};

/// A random scene whose shapes are partly letters, so polygon layers carry
/// outlines that have to move with them.
fn scene_of(rng: &mut Rng) -> (Scene, NodeHandle) {
    let mut scene = Scene::new();
    let root = make(&mut scene, rng, 4);
    letters(&mut scene, rng, root);
    (scene, root)
}

fn letters(scene: &mut Scene, rng: &mut Rng, node: NodeHandle) {
    if scene.element(node).ok() == Some(Element::SdfShape) && rng.chance(60) {
        let _ = scene.assign(node, "glyph", ["a", "B", "%"][rng.below(3)]);
    }
    for child in scene.children(node).unwrap_or_default().to_vec() {
        letters(scene, rng, child);
    }
}

fn same_fields(left: &FieldBatch, right: &FieldBatch) -> bool {
    left.indices == right.indices
        && left.instances == right.instances
        && left.layers == right.layers
        && left.materials == right.materials
        && left.outlines == right.outlines
        && left.shaders == right.shaders
        && left.starts == right.starts
}

#[test]
fn fields_kept_from_the_last_frame_match_fields_gathered_afresh() {
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    let (mut text, mut drawings) = (TextSystem::new(), SvgOutlines::new());
    let (mut fresh_text, mut fresh_drawings) = (TextSystem::new(), SvgOutlines::new());
    let (mut read, mut outlines) = (0, 0);
    for seed in 1..=80u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (mut scene, root) = scene_of(&mut rng);
        let mut engine = RenderEngine::new(RecordingBackend::default());
        let mut kept = KeptFields::default();
        for step in 0..30 {
            if step > 0 {
                for _ in 0..1 + rng.below(3) {
                    mutate(&mut scene, &mut rng, root);
                }
                letters(&mut scene, &mut rng, root);
            }
            if rng.chance(3) {
                engine.forget();
            }
            let Ok(layout) = Layout::compute(&scene, root, size, &mut text) else {
                continue;
            };
            if engine.render(&scene, &layout, 120, |_| {}).is_err() {
                continue;
            }
            let (list, copies) = (engine.last_list(), engine.copies());
            let fonts = font_count(&mut text);
            let copier = kept.copier(copies, 120, &mut text);
            read += usize::from(copier.is_some() && !copies.runs.is_empty());
            let batch = collect_field_instances(list, 120, &mut text, &mut drawings, copier);
            let fresh =
                collect_field_instances(list, 120, &mut fresh_text, &mut fresh_drawings, None);
            assert!(
                same_fields(&batch, &fresh),
                "seed {seed}, step {step}: kept fields differ from fields gathered afresh"
            );
            outlines += batch.outlines.len();
            // A frame the backend was not asked to draw leaves nothing to
            // read from.
            if !rng.chance(10) {
                let fonts = (fonts, font_count(&mut text));
                kept.keep(batch, copies.frame, 120, fonts);
            }
        }
    }
    assert!(read > 500, "only {read} frames read kept fields");
    assert!(outlines > 0, "no polygon layer carried an outline");
}

/// An atlas without a texture: glyphs placed along shelves as they come,
/// which a rebuild shuffles and a growth makes taller, each a new
/// generation.
struct FakeAtlas {
    entries: HashMap<GlyphKey, GlyphAtlasEntry>,
    pages: u32,
    next: (u32, u32),
    generation: u64,
}

impl FakeAtlas {
    fn new(generation: u64) -> Self {
        Self {
            entries: HashMap::new(),
            pages: 1,
            next: (0, 0),
            generation,
        }
    }

    fn place(&mut self, key: GlyphKey) {
        if self.entries.contains_key(&key) {
            return;
        }
        if self.next.0 + key.width + 2 > GLYPH_ATLAS_SIZE {
            self.next = (0, self.next.1 + 64);
        }
        let entry = GlyphAtlasEntry {
            x: self.next.0 + 1,
            y: self.next.1 + 1,
            width: key.width,
            height: key.height,
            last_used: 0,
            pixels: Vec::new(),
            outside: 0,
        };
        self.next.0 += key.width + 2;
        self.entries.insert(key, entry);
    }

    fn rebuild(&mut self, generation: u64) {
        let mut keys: Vec<GlyphKey> = self.entries.keys().copied().collect();
        keys.sort_by_key(|key| std::cmp::Reverse((key.id, key.width, key.height)));
        self.entries.clear();
        self.next = (7, 0);
        for key in keys {
            self.place(key);
        }
        self.generation = generation;
    }

    fn view(&self) -> AtlasView<'_> {
        AtlasView {
            entries: &self.entries,
            height: GLYPH_ATLAS_SIZE * self.pages,
        }
    }
}

fn same_quads(left: &GlyphBatch, right: &GlyphBatch) -> bool {
    let spans = |batch: &GlyphBatch| -> Vec<Vec<(std::ops::Range<u32>, bool, bool)>> {
        let spans = batch.command_spans.iter();
        spans
            .map(|spans| {
                let spans = spans.iter();
                spans
                    .map(|span| (span.range.clone(), span.color, span.lcd))
                    .collect()
            })
            .collect()
    };
    bytemuck::cast_slice::<_, u8>(&left.instances)
        == bytemuck::cast_slice::<_, u8>(&right.instances)
        && left.plain == right.plain
        && spans(left) == spans(right)
}

#[test]
fn glyph_quads_kept_from_the_last_frame_match_quads_made_afresh() {
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    let mut text = TextSystem::new();
    let (mut generation, mut reused, mut moved) = (1, 0, 0);
    for seed in 1..=60u64 {
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let (mut scene, root) = scene_of(&mut rng);
        let mut engine = RenderEngine::new(RecordingBackend::default());
        let mut kept = KeptText::default();
        let mut atlases = [FakeAtlas::new(generation), FakeAtlas::new(generation + 1)];
        generation += 2;
        let mut target = (200, 160);
        for step in 0..30 {
            if step > 0 {
                for _ in 0..1 + rng.below(3) {
                    mutate(&mut scene, &mut rng, root);
                }
            }
            let Ok(layout) = Layout::compute(&scene, root, size, &mut text) else {
                continue;
            };
            if engine.render(&scene, &layout, 120, |_| {}).is_err() {
                continue;
            }
            let (list, copies) = (engine.last_list(), engine.copies());
            let fonts = font_count(&mut text);
            let (mut glyphs, mut under, mut over): (Vec<PreparedGlyph>, Vec<_>, Vec<_>) =
                Default::default();
            let mut copier = kept.copier(copies, 120, &mut text);
            lay_out_text(
                &mut text,
                list,
                1.0,
                copier.as_mut(),
                (&mut glyphs, &mut under, &mut over),
            );
            for prepared in &glyphs {
                for glyph in prepared.sources() {
                    let color = usize::from(glyph.content == RasterContent::Color);
                    atlases[color].place(GlyphKey::from_glyph(glyph));
                }
            }
            // The atlases move what they hold now and then, and the target
            // changes size.
            for atlas in &mut atlases {
                if rng.chance(8) {
                    atlas.rebuild(generation);
                    generation += 1;
                    moved += 1;
                } else if rng.chance(4) && atlas.pages == 1 {
                    atlas.pages = 2;
                    atlas.generation = generation;
                    generation += 1;
                    moved += 1;
                }
            }
            if rng.chance(4) {
                target = (target.0 + 8, target.1);
            }
            let views = (atlases[0].view(), atlases[1].view());
            let stamps = [atlases[0].generation, atlases[1].generation];
            let reuse = copier
                .as_ref()
                .and_then(|copier| copier.quads(stamps, target));
            reused += usize::from(reuse.is_some() && !copies.runs.is_empty());
            let items = (&glyphs[..], &under[..], &over[..]);
            let commands = list.commands.len();
            let Ok(mut batch) = glyph_quads(views, items, reuse, commands, 1.0, target) else {
                panic!("seed {seed}, step {step}: a glyph is missing from its atlas");
            };
            let fresh = glyph_quads(views, items, None, commands, 1.0, target).unwrap();
            assert!(
                same_quads(&batch, &fresh),
                "seed {seed}, step {step}: kept quads differ from quads made afresh"
            );
            batch.atlases = stamps;
            if !rng.chance(10) {
                let fonts = (fonts, font_count(&mut text));
                kept.keep(
                    (&mut glyphs, &mut under, &mut over),
                    copies.frame,
                    120,
                    fonts,
                );
                kept.keep_quads(batch, target);
            }
        }
    }
    assert!(reused > 300, "only {reused} frames read kept quads");
    assert!(moved > 50, "the atlases moved only {moved} times");
}

/// Rows of shadowed boxes, letters and labels over a ground: the root, the
/// ground and the nodes that change.
fn board(scene: &mut Scene) -> (NodeHandle, NodeHandle, Vec<NodeHandle>) {
    let root = scene.create(Element::Item);
    let ground = scene.create(Element::Rect);
    for node in [root, ground] {
        for (key, value) in [("width", 160.0), ("height", 120.0)] {
            scene.assign(node, key, value).unwrap();
        }
    }
    scene.assign(ground, "color", "#1d3557").unwrap();
    scene.reparent(ground, Some(root)).unwrap();
    let column = scene.create(Element::Column);
    scene.assign(column, "gap", 4.0).unwrap();
    scene.reparent(column, Some(root)).unwrap();
    let mut changing = Vec::new();
    for item in 0..5 {
        let row = scene.create(Element::Row);
        scene.assign(row, "gap", 6.0).unwrap();
        scene.reparent(row, Some(column)).unwrap();
        let card = scene.create(Element::Rect);
        for (key, value) in [("width", 18.0), ("height", 14.0), ("radius", 4.0)] {
            scene.assign(card, key, value).unwrap();
        }
        scene.assign(card, "color", "#e63946").unwrap();
        scene.assign(card, "shadow_color", "#00000080").unwrap();
        scene
            .assign(card, "shadow_blur", 1.0 + item as f64)
            .unwrap();
        scene.reparent(card, Some(row)).unwrap();
        let field = scene.create(Element::Sdf);
        for (key, value) in [("width", 14.0), ("height", 14.0)] {
            scene.assign(field, key, value).unwrap();
        }
        scene.assign(field, "fill_color", "#f1faee").unwrap();
        scene.reparent(field, Some(row)).unwrap();
        let letter = scene.create(Element::SdfShape);
        let fill = Value::Map([("fill".to_owned(), Value::Bool(true))].into());
        scene.assign(letter, "anchors", fill).unwrap();
        scene.assign(letter, "glyph", ["a", "B"][item % 2]).unwrap();
        scene.reparent(letter, Some(field)).unwrap();
        let label = scene.create(Element::Text);
        scene.assign(label, "text", format!("row {item}")).unwrap();
        scene.assign(label, "color", "#f1faee").unwrap();
        scene.reparent(label, Some(row)).unwrap();
        changing.extend([card, label]);
    }
    (root, ground, changing)
}

fn frame(engine: &mut RenderEngine<WgpuBackend>, scene: &Scene, root: NodeHandle) {
    let size = Size {
        width: 160.0,
        height: 120.0,
    };
    let layout = Layout::compute(scene, root, size, engine.backend_mut()).unwrap();
    engine.render(scene, &layout, 120, |_| {}).unwrap();
}

#[test]
#[ignore = "requires a GPU adapter"]
fn frames_drawn_with_kept_fields_and_quads_match_frames_drawn_whole() {
    require_gpu!();
    let mut scene = Scene::new();
    let (root, ground, changing) = board(&mut scene);
    let backend = pollster::block_on(WgpuBackend::new(160, 120)).unwrap();
    let mut engine = RenderEngine::new(backend);
    frame(&mut engine, &scene, root);
    for step in 0..10 {
        let node = changing[step * 3 % changing.len()];
        if scene.element(node).ok() == Some(Element::Text) {
            scene
                .assign(node, "text", format!("row {}", step * 7))
                .unwrap();
        } else if step % 4 == 2 {
            // A card gone: everything after it lies elsewhere in the buffers.
            let visible = !scene.bool_value(node, "visible").unwrap();
            scene.assign(node, "visible", visible).unwrap();
        } else {
            let color = ["#a8dadc", "#457b9d"][step % 2];
            scene.assign(node, "color", color).unwrap();
        }
        // The ground changes too, so every command copied is drawn again.
        let color = ["#14213d", "#1d3557"][step % 2];
        scene.assign(ground, "color", color).unwrap();
        frame(&mut engine, &scene, root);
        assert!(
            !engine.copies().runs.is_empty(),
            "step {step}: nothing was copied"
        );
        let drawn = engine.backend_mut().read_pixels();
        let backend = pollster::block_on(WgpuBackend::new(160, 120)).unwrap();
        let mut fresh = RenderEngine::new(backend);
        frame(&mut fresh, &scene, root);
        let whole = fresh.backend_mut().read_pixels();
        let (difference, x, y) = super::partial_tests::largest_difference(&drawn, &whole, 160);
        assert!(
            difference == 0,
            "step {step}: the frame drawn with kept buffers differs by {difference} at ({x}, {y})"
        );
    }
}
