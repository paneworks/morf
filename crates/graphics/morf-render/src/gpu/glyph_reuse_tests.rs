//! Text read from the last frame for the commands a list copied, against the
//! same list's text laid out afresh by a text system of its own: the same
//! glyphs and bands, every frame, on random scenes under random changes.

use morf_layout::{Layout, Size};
use morf_scene::{Element, NodeHandle, Scene};
use morf_text::TextSystem;

use super::glyph_batch::{KeptText, font_count, lay_out_text};
use crate::tests::RecordingBackend;
use crate::tests::reuse::{Rng, make, mutate, underline};
use crate::{RenderEngine, WgpuBackend};

#[test]
fn text_kept_from_the_last_frame_matches_text_laid_out_afresh() {
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    let mut text = TextSystem::new();
    let mut fresh_text = TextSystem::new();
    let (mut read, mut drawn) = (0, 0);
    for seed in 1..=80u64 {
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let mut scene = Scene::new();
        let root = make(&mut scene, &mut rng, 4);
        let mut engine = RenderEngine::new(RecordingBackend::default());
        let mut kept = KeptText::default();
        for step in 0..30 {
            if step > 0 {
                for _ in 0..1 + rng.below(3) {
                    mutate(&mut scene, &mut rng, root);
                }
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
            let list = engine.last_list();
            let copies = engine.copies();
            let fonts = font_count(&mut text);
            let (mut glyphs, mut under, mut over) = Default::default();
            let copier = kept.copier(copies, 120, &mut text);
            read += usize::from(copier.is_some() && !copies.runs.is_empty());
            lay_out_text(
                &mut text,
                list,
                1.0,
                copier,
                (&mut glyphs, &mut under, &mut over),
            );
            let (mut fresh_glyphs, mut fresh_under, mut fresh_over) = Default::default();
            lay_out_text(
                &mut fresh_text,
                list,
                1.0,
                None,
                (&mut fresh_glyphs, &mut fresh_under, &mut fresh_over),
            );
            assert!(
                glyphs == fresh_glyphs && under == fresh_under && over == fresh_over,
                "seed {seed}, step {step}: kept text differs from text laid out afresh"
            );
            drawn += glyphs.len();
            // A frame the backend was not asked to draw leaves nothing to
            // read from.
            if !rng.chance(10) {
                let fonts = (fonts, font_count(&mut text));
                kept.keep(
                    (&mut glyphs, &mut under, &mut over),
                    copies.frame,
                    120,
                    fonts,
                );
            }
        }
    }
    assert!(read > 500, "only {read} frames read kept text");
    assert!(drawn > 1000, "only {drawn} glyphs laid out");
}

/// Rows of an icon, an underlined label and a badge; the badges and labels.
fn rows(scene: &mut Scene, root: NodeHandle) -> (Vec<NodeHandle>, Vec<NodeHandle>) {
    let column = scene.create(Element::Column);
    scene.assign(column, "gap", 2.0).unwrap();
    scene.reparent(column, Some(root)).unwrap();
    let (mut badges, mut labels) = (Vec::new(), Vec::new());
    for item in 0..6 {
        let row = scene.create(Element::Row);
        scene.assign(row, "gap", 4.0).unwrap();
        scene.reparent(row, Some(column)).unwrap();
        let label = scene.create(Element::Text);
        scene.assign(label, "text", format!("item {item}")).unwrap();
        scene.assign(label, "color", "#f1faee").unwrap();
        scene.assign(label, "decoration", underline()).unwrap();
        scene.reparent(label, Some(row)).unwrap();
        let badge = scene.create(Element::Rect);
        scene.assign(badge, "width", 14.0).unwrap();
        scene.assign(badge, "height", 10.0).unwrap();
        scene.assign(badge, "color", "#e63946").unwrap();
        scene.reparent(badge, Some(row)).unwrap();
        badges.push(badge);
        labels.push(label);
    }
    (badges, labels)
}

fn frame(engine: &mut RenderEngine<WgpuBackend>, scene: &Scene, root: NodeHandle) {
    let size = Size {
        width: 120.0,
        height: 120.0,
    };
    let layout = Layout::compute(scene, root, size, engine.backend_mut()).unwrap();
    engine.render(scene, &layout, 120, |_| {}).unwrap();
}

#[test]
#[ignore = "requires a GPU adapter"]
fn frames_drawing_kept_text_match_frames_drawn_whole() {
    require_gpu!();
    let mut scene = Scene::new();
    let root = scene.create(Element::Rect);
    scene.assign(root, "width", 120.0).unwrap();
    scene.assign(root, "height", 120.0).unwrap();
    scene.assign(root, "color", "#1d3557").unwrap();
    let (badges, labels) = rows(&mut scene, root);
    let mut engine = RenderEngine::new(pollster::block_on(WgpuBackend::new(120, 120)).unwrap());
    frame(&mut engine, &scene, root);
    for step in 0..8 {
        if step % 2 == 0 {
            let color = ["#a8dadc", "#457b9d"][step / 2 % 2];
            scene.assign(badges[step % 6], "color", color).unwrap();
        } else {
            let text = format!("item {}", step * 7);
            scene.assign(labels[step % 6], "text", text).unwrap();
        }
        frame(&mut engine, &scene, root);
        assert!(
            !engine.copies().runs.is_empty(),
            "step {step}: nothing was copied"
        );
        let drawn = engine.backend_mut().read_pixels();
        let mut fresh = RenderEngine::new(pollster::block_on(WgpuBackend::new(120, 120)).unwrap());
        frame(&mut fresh, &scene, root);
        let whole = fresh.backend_mut().read_pixels();
        let (difference, x, y) = super::partial_tests::largest_difference(&drawn, &whole, 120);
        assert!(
            difference == 0,
            "step {step}: the frame drawn with kept text differs by {difference} at ({x}, {y})"
        );
    }
}
