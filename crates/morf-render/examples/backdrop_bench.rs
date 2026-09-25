//! What frosted glass costs on the GPU: a desk of ten blurred panels over a
//! wallpaper, timed still, with a clock ticking on top, and with the wallpaper
//! changing under every panel every frame.
//!
//! ```sh
//! nixVulkanIntel cargo run --release -p morf-render --example backdrop_bench -- \
//!     [WALLPAPER.jpg] [WIDTHxHEIGHT] [RADIUS]
//! ```
//!
//! Every frame is waited for, so the times are the GPU's. They are the median
//! of each run: background load only adds time, and one slow frame should not
//! move the answer.

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

struct Desk {
    scene: Scene,
    root: NodeHandle,
    ground: NodeHandle,
    panels: Vec<NodeHandle>,
    hands: Vec<NodeHandle>,
}

fn desk(width: f64, height: f64, wallpaper: Option<&str>, blur: Option<f64>) -> Desk {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", width).unwrap();
    scene.assign(root, "height", height).unwrap();
    let ground = match wallpaper {
        Some(path) => {
            let image = scene.create(Element::Image);
            scene.assign(image, "source", path).unwrap();
            scene
                .assign(image, "fill_mode", "preserve_aspect_crop")
                .unwrap();
            image
        }
        None => {
            let rect = scene.create(Element::Rect);
            let gradient = morf_scene::Value::Map(
                [
                    ("kind".to_owned(), Value::String("linear".to_owned())),
                    ("angle".to_owned(), Value::Number(35.0)),
                    (
                        "stops".to_owned(),
                        Value::List(vec![
                            Value::String("#1d3557".to_owned()),
                            Value::String("#e63946".to_owned()),
                            Value::String("#f1faee".to_owned()),
                        ]),
                    ),
                ]
                .into_iter()
                .collect(),
            );
            scene.assign(rect, "gradient", gradient).unwrap();
            rect
        }
    };
    scene.assign(ground, "width", width).unwrap();
    scene.assign(ground, "height", height).unwrap();
    scene.reparent(ground, Some(root)).unwrap();
    let mut panels = Vec::new();
    let mut hands = Vec::new();
    for index in 0..10 {
        let (column, row) = (index % 5, index / 5);
        let panel = scene.create(Element::ClipRect);
        for (property, value) in [
            ("x", 40.0 + f64::from(column) * 340.0),
            ("y", 60.0 + f64::from(row) * 260.0),
            ("width", 300.0),
            ("height", 220.0),
            ("radius", 22.0),
        ] {
            scene.assign(panel, property, value).unwrap();
        }
        scene.assign(panel, "color", "#1a1a1e8c").unwrap();
        if let Some(radius) = blur {
            scene.assign(panel, "backdrop_blur", radius).unwrap();
        }
        scene.reparent(panel, Some(root)).unwrap();
        // A clock hand inside each panel, for the ticking run.
        let hand = scene.create(Element::Rect);
        for (property, value) in [("x", 148.0), ("y", 30.0), ("width", 4.0), ("height", 80.0)] {
            scene.assign(hand, property, value).unwrap();
        }
        scene.assign(hand, "transform_origin_y", 1.0).unwrap();
        scene.reparent(hand, Some(panel)).unwrap();
        panels.push(panel);
        hands.push(hand);
    }
    Desk {
        scene,
        root,
        ground,
        panels,
        hands,
    }
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn run(
    engine: &mut RenderEngine<WgpuBackend>,
    desk: &mut Desk,
    size: Size,
    frames: u32,
    mut step: impl FnMut(&mut Desk, u32),
) -> (Duration, u64) {
    let blurs = engine.backend_mut().backdrop_blurs();
    let mut times = Vec::new();
    for frame in 0..frames {
        step(desk, frame);
        let layout = Layout::compute(&desk.scene, desk.root, size, &mut NoText).unwrap();
        let start = Instant::now();
        engine.render(&desk.scene, &layout, 120, |_| {}).unwrap();
        engine.backend_mut().wait_idle();
        times.push(start.elapsed());
    }
    (median(times), engine.backend_mut().backdrop_blurs() - blurs)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wallpaper = args
        .get(1)
        .filter(|path| !path.contains('x') || path.contains('/'));
    let (width, height) = args
        .iter()
        .skip(1)
        .find_map(|arg| {
            let (w, h) = arg.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        })
        .unwrap_or((1920, 1080));
    let radius: f64 = args
        .iter()
        .skip(1)
        .rev()
        .find_map(|arg| arg.parse().ok())
        .unwrap_or(20.0);
    let size = Size {
        width: f64::from(width),
        height: f64::from(height),
    };
    println!(
        "{width}x{height}, ten 300x220 panels, radius {radius}, wallpaper {}",
        wallpaper.map_or("a gradient", String::as_str)
    );
    for blur in [None, Some(radius)] {
        let backend = pollster::block_on(WgpuBackend::new(width, height)).expect("a GPU adapter");
        let mut engine = RenderEngine::new(backend);
        let mut desk = desk(size.width, size.height, wallpaper.map(String::as_str), blur);
        let label = if blur.is_some() { "frosted" } else { "flat   " };
        // Warm up: pipelines, the wallpaper's texture, the first blur.
        run(&mut engine, &mut desk, size, 3, |_, _| {});
        let (first, _) = run(&mut engine, &mut desk, size, 1, |desk, _| {
            // Everything repainted, as after a resize.
            desk.scene.assign(desk.root, "opacity", 0.999).unwrap();
        });
        let (still, still_blurs) = run(&mut engine, &mut desk, size, 60, |desk, frame| {
            // Nothing changes but a property nobody paints differently: the
            // frame is diffed and nothing is drawn.
            let _ = (desk, frame);
        });
        let (ticking, ticking_blurs) = run(&mut engine, &mut desk, size, 60, |desk, frame| {
            for hand in &desk.hands {
                desk.scene
                    .assign(*hand, "rotation", f64::from(frame) * 6.0)
                    .unwrap();
            }
        });
        let (changing, changing_blurs) = run(&mut engine, &mut desk, size, 60, |desk, frame| {
            // The wallpaper under every panel moves a pixel every frame.
            desk.scene
                .assign(desk.ground, "x", f64::from(frame % 2))
                .unwrap();
        });
        let _ = &desk.panels;
        println!(
            "{label}  full repaint {:>7.3} ms | still {:>6.3} ms ({still_blurs} blurs) | clock ticking in each {:>6.3} ms ({ticking_blurs} blurs) | wallpaper moving under all {:>6.3} ms ({changing_blurs} blurs)",
            first.as_secs_f64() * 1e3,
            still.as_secs_f64() * 1e3,
            ticking.as_secs_f64() * 1e3,
            changing.as_secs_f64() * 1e3,
        );
    }
}
