//! Run with `cargo run --release -p morf-render --example drawlist_bench`.
//! Measures the CPU side of a frame -- the draw list and its damage -- on a
//! dashboard of a few hundred nodes where one property changes per frame.

use std::time::{Duration, Instant};

use morf_layout::{Layout, Size, TextMeasurer, TextOptions};
use morf_render::{DamageRect, DamageTracker, DrawList, RenderBackend, RenderEngine};
use morf_scene::{Color, Element, NodeHandle, Scene, Value};

struct Fixed;

impl TextMeasurer for Fixed {
    fn measure(&mut self, _: NodeHandle, text: &str, _: &str, size: f64, _: TextOptions) -> Size {
        Size {
            width: text.len() as f64 * size / 2.0,
            height: size,
        }
    }
}

/// Draws nothing, so the engine's own work is all that is timed.
struct Null;

impl RenderBackend for Null {
    type Error = std::io::Error;

    fn render(&mut self, _: &DrawList, _: &[DamageRect], _: u32) -> Result<(), Self::Error> {
        Ok(())
    }

    fn resize(&mut self, _: u32, _: u32) {}
}

struct Dashboard {
    scene: Scene,
    root: NodeHandle,
    badges: Vec<NodeHandle>,
    labels: Vec<NodeHandle>,
}

fn dashboard(panels: usize) -> Dashboard {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 1920.0).unwrap();
    scene.assign(root, "height", 1080.0).unwrap();
    let (mut badges, mut labels) = (Vec::new(), Vec::new());
    for panel in 0..panels {
        let card = scene.create(Element::Rect);
        scene.assign(card, "x", (panel % 8) as f64 * 236.0).unwrap();
        scene.assign(card, "y", (panel / 8) as f64 * 200.0).unwrap();
        scene.assign(card, "width", 220.0).unwrap();
        scene.assign(card, "height", 180.0).unwrap();
        scene.assign(card, "radius", 12.0).unwrap();
        scene
            .assign(card, "color", Value::Color(Color::rgba8(30, 30, 40, 255)))
            .unwrap();
        if panel % 5 == 0 {
            scene.assign(card, "opacity", 0.9).unwrap();
        }
        scene.reparent(card, Some(root)).unwrap();
        let column = scene.create(Element::Column);
        scene.assign(column, "gap", 6.0).unwrap();
        scene.reparent(column, Some(card)).unwrap();
        let title = scene.create(Element::Text);
        scene
            .assign(title, "text", format!("Panel {panel}"))
            .unwrap();
        scene.reparent(title, Some(column)).unwrap();
        for item in 0..5 {
            let row = scene.create(Element::Row);
            scene.assign(row, "gap", 4.0).unwrap();
            scene.reparent(row, Some(column)).unwrap();
            let icon = scene.create(Element::Rect);
            scene.assign(icon, "width", 16.0).unwrap();
            scene.assign(icon, "height", 16.0).unwrap();
            scene.assign(icon, "radius", 4.0).unwrap();
            scene.reparent(icon, Some(row)).unwrap();
            let label = scene.create(Element::Text);
            scene.assign(label, "text", format!("item {item}")).unwrap();
            scene.reparent(label, Some(row)).unwrap();
            let badge = scene.create(Element::Rect);
            scene.assign(badge, "width", 24.0).unwrap();
            scene.assign(badge, "height", 12.0).unwrap();
            scene.reparent(badge, Some(row)).unwrap();
            badges.push(badge);
            labels.push(label);
        }
    }
    Dashboard {
        scene,
        root,
        badges,
        labels,
    }
}

fn main() {
    let frames = 400;
    let size = Size {
        width: 1920.0,
        height: 1080.0,
    };
    for panels in [10, 40] {
        let Dashboard {
            mut scene,
            root,
            badges,
            labels,
        } = dashboard(panels);
        let nodes = scene.node_count();
        let mut layout = Layout::default();
        layout.update(&scene, root, size, &mut Fixed).unwrap();
        let mut list = DrawList::default();
        let mut tracker = DamageTracker::default();
        list.rebuild(&scene, &layout).unwrap();
        tracker.retain(&mut list);
        let mut engine = RenderEngine::new(Null);
        engine.render(&scene, &layout, 120, |_| {}).unwrap();
        let (mut rebuild, mut diff, mut kept) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
        for frame in 0..frames {
            if frame % 2 == 0 {
                let badge = badges[frame * 7 % badges.len()];
                let shade = (frame % 200) as u8;
                scene
                    .assign(
                        badge,
                        "color",
                        Value::Color(Color::rgba8(shade, 100, 200, 255)),
                    )
                    .unwrap();
            } else {
                let label = labels[frame * 3 % labels.len()];
                scene
                    .assign(label, "text", format!("item {}", frame % 9))
                    .unwrap();
            }
            layout.update(&scene, root, size, &mut Fixed).unwrap();
            let started = Instant::now();
            list.rebuild(&scene, &layout).unwrap();
            rebuild += started.elapsed();
            let started = Instant::now();
            std::hint::black_box(tracker.diff(&list, 120));
            diff += started.elapsed();
            tracker.retain(&mut list);
            let started = Instant::now();
            engine.render(&scene, &layout, 120, |_| {}).unwrap();
            kept += started.elapsed();
            let fresh = DrawList::from_scene(&scene, &layout).unwrap();
            assert!(
                *engine.last_list() == fresh,
                "frame {frame}: the kept list differs from one built afresh"
            );
        }
        println!(
            "{nodes:5} nodes: whole rebuild {:?} + damage {:?}/frame; engine, copying unchanged subtrees, {:?}/frame",
            rebuild / frames as u32,
            diff / frames as u32,
            kept / frames as u32
        );
    }
}
