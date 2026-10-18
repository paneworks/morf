//! What the field shader's commonest shapes cost on the GPU: a rounded box
//! alone in its field, the same box casting an offset shadow, and a
//! rectangle with the same shadow for comparison, each drawn as a stack of
//! large overlapping cards repainted whole every frame.
//!
//! ```sh
//! nixVulkanIntel cargo run --release -p morf-render --example box_bench
//! ```
//!
//! Every frame is waited for; each case prints the median of its frames and,
//! after the slash, the fastest -- the GPU is shared with the compositor, so
//! the fastest is the closest reading of the frame's own cost.

use std::time::{Duration, Instant};

use morf_layout::{Layout, Size, TextMeasurer, TextOptions};
use morf_render::{RenderEngine, WgpuBackend};
use morf_scene::{Element, NodeHandle, Scene, Value};

struct NoText;

impl TextMeasurer for NoText {
    fn measure(
        &mut self,
        _node: NodeHandle,
        _text: &str,
        _family: &str,
        _size: f64,
        _options: TextOptions,
    ) -> Size {
        Size::default()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Card {
    Field,
    FieldShadow,
    RectShadow,
}

const CARDS: usize = 24;

fn cards(size: Size, kind: Card) -> (Scene, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", size.width).unwrap();
    scene.assign(root, "height", size.height).unwrap();
    for index in 0..CARDS {
        let (column, row) = ((index % 6) as f64, (index / 6) as f64);
        let card = match kind {
            Card::RectShadow => {
                let card = scene.create(Element::Rect);
                scene.assign(card, "color", "#3a5a8cf0").unwrap();
                scene.assign(card, "radius", 24.0).unwrap();
                card
            }
            Card::Field | Card::FieldShadow => {
                let card = scene.create(Element::Sdf);
                scene.assign(card, "fill_color", "#3a5a8cf0").unwrap();
                let shape = scene.create(Element::SdfShape);
                scene.assign(shape, "shape", "box").unwrap();
                scene.assign(shape, "radius", 24.0).unwrap();
                let fill = Value::Map([("fill".to_owned(), Value::Bool(true))].into());
                scene.assign(shape, "anchors", fill).unwrap();
                scene.reparent(shape, Some(card)).unwrap();
                card
            }
        };
        scene.assign(card, "x", 40.0 + column * 260.0).unwrap();
        scene.assign(card, "y", 40.0 + row * 230.0).unwrap();
        scene.assign(card, "width", 420.0).unwrap();
        scene.assign(card, "height", 300.0).unwrap();
        if kind != Card::Field {
            scene.assign(card, "shadow_color", "#00000090").unwrap();
            scene.assign(card, "shadow_blur", 28.0).unwrap();
            scene.assign(card, "shadow_offset_y", 10.0).unwrap();
        }
        scene.reparent(card, Some(root)).unwrap();
    }
    (scene, root)
}

fn full(engine: &mut RenderEngine<WgpuBackend>, size: Size, kind: Card) -> (Duration, Duration) {
    let (scene, root) = cards(size, kind);
    let layout = Layout::compute(&scene, root, size, &mut NoText).unwrap();
    let mut times = Vec::new();
    for _ in 0..120 {
        engine.forget();
        let start = Instant::now();
        engine.render(&scene, &layout, 120, |_| {}).unwrap();
        engine.backend_mut().wait_idle();
        times.push(start.elapsed());
    }
    times.sort();
    (times[times.len() / 2], times[0])
}

fn main() {
    let size = Size {
        width: 1920.0,
        height: 1080.0,
    };
    let backend = pollster::block_on(WgpuBackend::new(1920, 1080)).expect("a GPU adapter");
    let mut engine = RenderEngine::new(backend);
    full(&mut engine, size, Card::FieldShadow);
    println!("{CARDS} cards of 420x300 at 1920x1080, median/fastest");
    for (name, kind) in [
        ("rounded box field          ", Card::Field),
        ("rounded box field + shadow ", Card::FieldShadow),
        ("rounded rect + shadow      ", Card::RectShadow),
    ] {
        let (median, fastest) = full(&mut engine, size, kind);
        println!(
            "  {name} {:>6.3}/{:>6.3} ms",
            median.as_secs_f64() * 1e3,
            fastest.as_secs_f64() * 1e3
        );
    }
}
