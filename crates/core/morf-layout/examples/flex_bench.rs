//! Run with `cargo run --release -p morf-layout --example flex_bench`.
//! Measures an incremental layout of a few hundred flex nodes when one
//! leaf changes size per frame, against a whole pass.

use std::time::Instant;

use morf_layout::{Layout, Size, TextMeasurer, TextOptions};
use morf_scene::{Element, NodeHandle, Scene};

struct Fixed;

impl TextMeasurer for Fixed {
    fn measure(
        &mut self,
        _: NodeHandle,
        text: &str,
        _: &str,
        size: f64,
        options: TextOptions,
    ) -> Size {
        let full = text.len() as f64 * size / 2.0;
        match options.width {
            Some(width) if width > 0.0 && options.wrap => Size {
                width: full.min(width),
                height: (full / width).ceil().max(1.0) * size,
            },
            _ => Size {
                width: full,
                height: size,
            },
        }
    }
}

fn build(rows: usize, columns: usize) -> (Scene, NodeHandle, Vec<NodeHandle>) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Flex);
    scene.assign(root, "direction", "column").unwrap();
    scene.assign(root, "gap", 4.0).unwrap();
    let mut leaves = Vec::new();
    for row in 0..rows {
        let line = scene.create(Element::Flex);
        scene.assign(line, "direction", "row").unwrap();
        scene.assign(line, "gap", 2.0).unwrap();
        scene.assign(line, "wrap", row % 3 == 0).unwrap();
        scene.reparent(line, Some(root)).unwrap();
        for column in 0..columns {
            let leaf = if column % 2 == 0 {
                let label = scene.create(Element::Text);
                scene.assign(label, "text", "label text").unwrap();
                scene.assign(label, "font_size", 12.0).unwrap();
                label
            } else {
                let rect = scene.create(Element::Rect);
                scene.assign(rect, "width", 20.0).unwrap();
                scene.assign(rect, "height", 16.0).unwrap();
                rect
            };
            scene.reparent(leaf, Some(line)).unwrap();
            leaves.push(leaf);
        }
    }
    (scene, root, leaves)
}

fn main() {
    let frames = 300;
    let available = Size {
        width: 1600.0,
        height: 900.0,
    };
    for (rows, columns) in [(10, 20), (20, 20), (40, 25)] {
        let (mut scene, root, leaves) = build(rows, columns);
        let nodes = 1 + rows * (columns + 1);
        let mut layout = Layout::default();
        layout.update(&scene, root, available, &mut Fixed).unwrap();
        let rects: Vec<NodeHandle> = leaves
            .iter()
            .copied()
            .filter(|leaf| scene.element(*leaf).unwrap() == Element::Rect)
            .collect();
        let started = Instant::now();
        for frame in 0..frames {
            let leaf = rects[frame * 7 % rects.len()];
            scene
                .assign(leaf, "width", 20.0 + (frame % 13) as f64)
                .unwrap();
            layout.update(&scene, root, available, &mut Fixed).unwrap();
        }
        let incremental = started.elapsed() / frames as u32;
        let started = Instant::now();
        for _ in 0..frames / 10 {
            std::hint::black_box(Layout::compute(&scene, root, available, &mut Fixed).unwrap());
        }
        let whole = started.elapsed() / (frames / 10) as u32;
        let fresh = Layout::compute(&scene, root, available, &mut Fixed).unwrap();
        assert_eq!(fresh.difference(&layout), None);
        println!("{nodes:5} nodes: update {incremental:?}/frame, whole pass {whole:?}");
    }
}
