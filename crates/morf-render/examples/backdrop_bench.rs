//! What frosted glass costs on the GPU: a desk of ten blurred panels over a
//! wallpaper, timed still, with a clock ticking on top, and with the wallpaper
//! changing under every panel every frame.
//!
//! ```sh
//! nixVulkanIntel cargo run --release -p morf-render --example backdrop_bench -- \
//!     [WALLPAPER.jpg] [WIDTHxHEIGHT] [RADIUS]
//! ```
//!
//! Every frame is waited for, so the times are the GPU's. Each is printed as
//! the median of its run and, after the slash, the fastest frame: the GPU is
//! shared with the compositor, so background load only ever adds time and the
//! minimum is the closest reading of what the frame itself costs.
//!
//! `BENCH_WAIT=1` also prints, before each line, the part of every run spent
//! waiting for the GPU alone, without the CPU's share.
//!
//! A second desk follows: thirty small rounded widgets, timed with the content
//! of one of them changing and with the content of all of them changing. And
//! a third: a page of forty lines of text on an opaque ground with a clock
//! ticking in its corner, drawn in greyscale and then in subpixels.

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

/// Median and minimum of a run.
#[derive(Clone, Copy)]
struct Timing(Duration, Duration);

impl std::fmt::Display for Timing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:>6.3}/{:>6.3} ms",
            self.0.as_secs_f64() * 1e3,
            self.1.as_secs_f64() * 1e3
        )
    }
}

fn timing(mut times: Vec<Duration>) -> Timing {
    times.sort();
    Timing(times[times.len() / 2], times[0])
}

/// Thirty small rounded widgets over a gradient, each with a bar inside that
/// grows when it ticks.
fn widgets(width: f64, height: f64) -> Desk {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", width).unwrap();
    scene.assign(root, "height", height).unwrap();
    let ground = scene.create(Element::Rect);
    scene.assign(ground, "width", width).unwrap();
    scene.assign(ground, "height", height).unwrap();
    scene.assign(ground, "color", "#1d3557").unwrap();
    scene.reparent(ground, Some(root)).unwrap();
    let mut panels = Vec::new();
    let mut hands = Vec::new();
    for index in 0..30 {
        let (column, row) = (index % 6, index / 6);
        let widget = scene.create(Element::ClipRect);
        for (property, value) in [
            ("x", 40.0 + f64::from(column) * 180.0),
            ("y", 40.0 + f64::from(row) * 120.0),
            ("width", 160.0),
            ("height", 96.0),
            ("radius", 14.0),
        ] {
            scene.assign(widget, property, value).unwrap();
        }
        scene.assign(widget, "color", "#1a1a1ecc").unwrap();
        scene.reparent(widget, Some(root)).unwrap();
        let bar = scene.create(Element::Rect);
        for (property, value) in [("x", 12.0), ("y", 40.0), ("width", 40.0), ("height", 16.0)] {
            scene.assign(bar, property, value).unwrap();
        }
        scene.assign(bar, "color", "#8ab4f8").unwrap();
        scene.reparent(bar, Some(widget)).unwrap();
        panels.push(widget);
        hands.push(bar);
    }
    Desk {
        scene,
        root,
        ground,
        panels,
        hands,
    }
}

/// A page of text on an opaque ground, a clock in its corner.
fn page(width: f64, height: f64) -> Desk {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", width).unwrap();
    scene.assign(root, "height", height).unwrap();
    let ground = scene.create(Element::Rect);
    scene.assign(ground, "width", width).unwrap();
    scene.assign(ground, "height", height).unwrap();
    scene.assign(ground, "color", "#fbfbf8").unwrap();
    scene.reparent(ground, Some(root)).unwrap();
    let mut panels = Vec::new();
    for line in 0..40 {
        let text = scene.create(Element::Text);
        scene
            .assign(
                text,
                "text",
                "The quick brown fox jumps over the lazy dog, 0123456789 times over.",
            )
            .unwrap();
        for (property, value) in [
            ("x", 40.0),
            ("y", 20.0 + f64::from(line) * 24.0),
            ("width", 900.0),
            ("height", 22.0),
            ("font_size", 15.0),
        ] {
            scene.assign(text, property, value).unwrap();
        }
        scene.assign(text, "color", "#202124").unwrap();
        scene.reparent(text, Some(root)).unwrap();
        panels.push(text);
    }
    let clock = scene.create(Element::Text);
    for (property, value) in [
        ("x", width - 160.0),
        ("y", 20.0),
        ("width", 120.0),
        ("height", 22.0),
        ("font_size", 15.0),
    ] {
        scene.assign(clock, property, value).unwrap();
    }
    scene.assign(clock, "text", "12:00:00").unwrap();
    scene.assign(clock, "color", "#202124").unwrap();
    scene.reparent(clock, Some(root)).unwrap();
    Desk {
        scene,
        root,
        ground,
        panels,
        hands: vec![clock],
    }
}

fn run(
    engine: &mut RenderEngine<WgpuBackend>,
    desk: &mut Desk,
    size: Size,
    frames: u32,
    step: impl FnMut(&mut Desk, u32),
) -> (Timing, u64) {
    run_measured(engine, desk, size, frames, false, step)
}

fn run_measured(
    engine: &mut RenderEngine<WgpuBackend>,
    desk: &mut Desk,
    size: Size,
    frames: u32,
    text: bool,
    mut step: impl FnMut(&mut Desk, u32),
) -> (Timing, u64) {
    let blurs = engine.backend_mut().backdrop_blurs();
    let mut times = Vec::new();
    let mut waits = Vec::new();
    for frame in 0..frames {
        step(desk, frame);
        let layout = if text {
            Layout::compute(&desk.scene, desk.root, size, engine.backend_mut()).unwrap()
        } else {
            Layout::compute(&desk.scene, desk.root, size, &mut NoText).unwrap()
        };
        let start = Instant::now();
        engine.render(&desk.scene, &layout, 120, |_| {}).unwrap();
        let submitted = Instant::now();
        engine.backend_mut().wait_idle();
        times.push(start.elapsed());
        waits.push(submitted.elapsed());
    }
    if std::env::var_os("BENCH_WAIT").is_some() && frames > 1 {
        // Only the part spent waiting for the GPU, without the CPU's.
        eprintln!("  (gpu wait {})", timing(waits));
    }
    (timing(times), engine.backend_mut().backdrop_blurs() - blurs)
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
                    .assign(*hand, "rotation", f64::from(frame + 1) * 6.0)
                    .unwrap();
            }
        });
        let (changing, changing_blurs) = run(&mut engine, &mut desk, size, 60, |desk, frame| {
            // The wallpaper under every panel moves a pixel every frame.
            desk.scene
                .assign(desk.ground, "x", f64::from((frame + 1) % 2))
                .unwrap();
        });
        let _ = &desk.panels;
        println!(
            "{label}  full repaint {first} | still {still} ({still_blurs} blurs) | clock ticking in each {ticking} ({ticking_blurs} blurs) | wallpaper moving under all {changing} ({changing_blurs} blurs)",
        );
    }
    let backend = pollster::block_on(WgpuBackend::new(width, height)).expect("a GPU adapter");
    let mut engine = RenderEngine::new(backend);
    let mut desk = widgets(size.width, size.height);
    run(&mut engine, &mut desk, size, 3, |_, _| {});
    let (first, _) = run(&mut engine, &mut desk, size, 1, |desk, _| {
        desk.scene.assign(desk.root, "opacity", 0.999).unwrap();
    });
    let (still, _) = run(&mut engine, &mut desk, size, 60, |_, _| {});
    let (one, _) = run(&mut engine, &mut desk, size, 60, |desk, frame| {
        desk.scene
            .assign(desk.hands[7], "width", 40.0 + f64::from(frame % 60 + 1))
            .unwrap();
    });
    let (all, _) = run(&mut engine, &mut desk, size, 60, |desk, frame| {
        for bar in &desk.hands {
            desk.scene
                .assign(*bar, "width", 40.0 + f64::from(frame % 60 + 1))
                .unwrap();
        }
    });
    let _ = (&desk.panels, desk.ground);
    println!(
        "widgets  thirty 160x96 rounded: full repaint {first} | still {still} | one ticking {one} | all ticking {all}",
    );
    for subpixel in [
        None,
        Some(morf_render::SubpixelText {
            bgr: false,
            filter: morf_render::LcdFilter::Default,
        }),
    ] {
        let mut backend =
            pollster::block_on(WgpuBackend::new(width, height)).expect("a GPU adapter");
        if subpixel.is_some() && !backend.supports_subpixel_text() {
            println!("text     subpixel: this adapter has no dual-source blending");
            continue;
        }
        backend.set_subpixel_text(subpixel);
        let mut engine = RenderEngine::new(backend);
        let mut desk = page(size.width, size.height);
        run_measured(&mut engine, &mut desk, size, 3, true, |_, _| {});
        let (first, _) = run_measured(&mut engine, &mut desk, size, 1, true, |desk, _| {
            desk.scene.assign(desk.root, "opacity", 0.999).unwrap();
        });
        let (ticking, _) = run_measured(&mut engine, &mut desk, size, 60, true, |desk, frame| {
            let text = format!("12:00:{:02}", (frame + 1) % 60);
            desk.scene.assign(desk.hands[0], "text", text).unwrap();
        });
        let _ = (&desk.panels, desk.ground);
        let label = if subpixel.is_some() {
            "subpixel"
        } else {
            "greyscale"
        };
        println!("text     forty lines, {label}: full repaint {first} | clock ticking {ticking}",);
    }
}
