//! Buffers presented in rotation, on a real adapter: whatever buffer a frame
//! lands in, and however many frames it missed while the compositor held it,
//! it must show exactly the frame -- the persistent target, byte for byte,
//! and the same scene drawn whole from nothing.

use std::collections::VecDeque;

use morf_layout::{Layout, Size};
use morf_scene::{Element, NodeHandle, Scene, Value};

use super::partial_tests::{LOGICAL, desk, frame, largest_difference};
use crate::tests::NoText;
use crate::*;

/// Asserts that the buffer the last frame went to shows the frame.
fn assert_presented_shows_the_frame(
    engine: &mut RenderEngine<WgpuBackend>,
    what: &str,
) -> Option<usize> {
    let index = engine.backend_mut().ring_presented()?;
    let buffer = engine.backend_mut().ring_pixels(index);
    let target = engine.backend_mut().read_pixels();
    let width = engine.backend_mut().width;
    let (difference, x, y) = largest_difference(&buffer, &target, width);
    assert_eq!(
        difference, 0,
        "{what}: buffer {index} differs from the frame by {difference} at ({x}, {y})"
    );
    Some(index)
}

/// Releases the oldest held buffers until `keep` are held.
fn compositor_keeps(
    engine: &mut RenderEngine<WgpuBackend>,
    held: &mut VecDeque<usize>,
    keep: usize,
) {
    while held.len() > keep {
        let index = held.pop_front().expect("more than kept");
        engine.backend_mut().ring_release(index);
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn buffers_in_rotation_show_every_frame_exactly() {
    // How many buffers the compositor holds after each frame: one on screen
    // usually, sometimes one it has not let go of, and sometimes all three,
    // so the frame after has none to go to and is skipped.
    let keeps = [1, 2, 3, 1, 1, 2, 3, 3, 1, 2, 1, 1];
    for scale_120 in [120, 180] {
        let physical = (LOGICAL * f64::from(scale_120) / 120.0).round() as u32;
        let mut desk = desk();
        let mut engine =
            RenderEngine::new(pollster::block_on(WgpuBackend::new(physical, physical)).unwrap());
        engine.backend_mut().present_into_ring(3);
        frame(&mut engine, &desk.scene, desk.root, scale_120);
        let mut held = VecDeque::new();
        held.extend(assert_presented_shows_the_frame(&mut engine, "first frame"));
        let steps = std::mem::take(&mut desk.steps);
        let (mut used, mut skipped) = (std::collections::BTreeSet::new(), 0);
        // Twice through, so every buffer comes back more than once.
        for (step, (node, property, value)) in steps.iter().chain(&steps).enumerate() {
            desk.scene.assign(*node, property, value.clone()).unwrap();
            let damage = frame(&mut engine, &desk.scene, desk.root, scale_120);
            if damage.is_empty() {
                continue;
            }
            let what = format!("scale {scale_120}, step {step} ({property})");
            match assert_presented_shows_the_frame(&mut engine, &what) {
                Some(index) => {
                    used.insert(index);
                    held.push_back(index);
                }
                None => skipped += 1,
            }
            compositor_keeps(&mut engine, &mut held, keeps[step % keeps.len()]);

            // And the frame itself is the scene drawn whole.
            let mut fresh = RenderEngine::new(
                pollster::block_on(WgpuBackend::new(physical, physical)).unwrap(),
            );
            frame(&mut fresh, &desk.scene, desk.root, scale_120);
            let whole = fresh.backend_mut().read_pixels();
            if let Some(index) = engine.backend_mut().ring_presented() {
                let buffer = engine.backend_mut().ring_pixels(index);
                let (difference, x, y) = largest_difference(&buffer, &whole, physical);
                assert!(
                    difference <= 1,
                    "{what}: buffer {index} differs from the whole frame by {difference} \
                     at ({x}, {y})"
                );
            }
        }
        assert_eq!(used.len(), 3, "every buffer came into use");
        assert!(skipped > 0, "a frame found every buffer held");
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_frame_with_no_buffer_reaches_the_next_one() {
    let mut desk = desk();
    let physical = LOGICAL as u32;
    let mut engine =
        RenderEngine::new(pollster::block_on(WgpuBackend::new(physical, physical)).unwrap());
    engine.backend_mut().present_into_ring(1);
    frame(&mut engine, &desk.scene, desk.root, 120);
    assert_eq!(engine.backend_mut().ring_presented(), Some(0));
    // The one buffer is held: this frame goes nowhere.
    let (node, property, value) = desk.steps[2].clone();
    desk.scene.assign(node, property, value).unwrap();
    assert!(!frame(&mut engine, &desk.scene, desk.root, 120).is_empty());
    assert_eq!(engine.backend_mut().ring_presented(), None);
    // Released, it takes the next frame -- a change somewhere else -- and
    // must show the missed one's change as well.
    engine.backend_mut().ring_release(0);
    let (node, property, value) = desk.steps[6].clone();
    desk.scene.assign(node, property, value).unwrap();
    frame(&mut engine, &desk.scene, desk.root, 120);
    assert_eq!(
        assert_presented_shows_the_frame(&mut engine, "after a skipped frame"),
        Some(0)
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_resize_repaints_every_buffer_whole() {
    let desk = desk();
    let mut engine = RenderEngine::new(pollster::block_on(WgpuBackend::new(96, 96)).unwrap());
    engine.backend_mut().present_into_ring(2);
    frame(&mut engine, &desk.scene, desk.root, 120);
    engine.backend_mut().ring_release(0);
    engine.resize(144, 144);
    frame(&mut engine, &desk.scene, desk.root, 180);
    assert_presented_shows_the_frame(&mut engine, "after a resize").expect("a buffer");
    engine.backend_mut().ring_release(0);
    engine.backend_mut().ring_release(1);
    engine.resize(144, 144);
    frame(&mut engine, &desk.scene, desk.root, 180);
    assert_presented_shows_the_frame(&mut engine, "after a second resize").expect("a buffer");
}

/// A full-HD surface: a gradient, and a dot crossing it a little each frame.
fn full_hd() -> (Scene, NodeHandle, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 1920.0).unwrap();
    scene.assign(root, "height", 1080.0).unwrap();
    let ground = scene.create(Element::Rect);
    for (property, value) in [("width", 1920.0), ("height", 1080.0)] {
        scene.assign(ground, property, value).unwrap();
    }
    let gradient = Value::Map(
        [
            ("kind".to_owned(), Value::String("linear".to_owned())),
            ("angle".to_owned(), Value::Number(20.0)),
            (
                "stops".to_owned(),
                Value::List(vec![
                    Value::String("#0b132b".to_owned()),
                    Value::String("#5bc0be".to_owned()),
                ]),
            ),
        ]
        .into_iter()
        .collect(),
    );
    scene.assign(ground, "gradient", gradient).unwrap();
    scene.reparent(ground, Some(root)).unwrap();
    let dot = scene.create(Element::Rect);
    for (property, value) in [("x", 40.0), ("y", 500.0), ("width", 48.0), ("height", 48.0)] {
        scene.assign(dot, property, value).unwrap();
    }
    scene.assign(dot, "radius", 24.0).unwrap();
    scene.assign(dot, "color", "#ffbe0bdd").unwrap();
    scene.reparent(dot, Some(root)).unwrap();
    (scene, root, dot)
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_full_hd_surface_through_small_damage() {
    let (mut scene, root, dot) = full_hd();
    let mut engine = RenderEngine::new(pollster::block_on(WgpuBackend::new(1920, 1080)).unwrap());
    engine.backend_mut().present_into_ring(3);
    let draw = |engine: &mut RenderEngine<WgpuBackend>, scene: &Scene| {
        let size = Size {
            width: 1920.0,
            height: 1080.0,
        };
        let layout = Layout::compute(scene, root, size, &mut NoText).unwrap();
        engine.render(scene, &layout, 120, |_| {}).unwrap()
    };
    draw(&mut engine, &scene);
    let mut held = VecDeque::new();
    held.extend(assert_presented_shows_the_frame(&mut engine, "first frame"));
    for step in 0..9 {
        scene
            .assign(dot, "x", 40.0 + 170.0 * f64::from(step + 1))
            .unwrap();
        scene
            .assign(dot, "y", 500.0 + 30.0 * f64::from(step % 3))
            .unwrap();
        let damage = draw(&mut engine, &scene);
        let area: u64 = damage
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum();
        assert!(area < 1920 * 1080 / 50, "a dot's move damages a dot's area");
        let index = assert_presented_shows_the_frame(&mut engine, &format!("step {step}"))
            .expect("a buffer is free every frame");
        held.push_back(index);
        compositor_keeps(&mut engine, &mut held, 1 + step as usize % 2);
    }
}
