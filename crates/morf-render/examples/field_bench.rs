//! What a screen-edge frame costs on the GPU: a fullscreen field — the
//! screen minus a rounded inner box — with a drawer on every edge merged into
//! it through circular seams, each drawer a layer tracking a sliding node.
//!
//! ```sh
//! nixVulkanIntel cargo run --release -p morf-render --example field_bench -- [WIDTHxHEIGHT]
//! ```
//!
//! Every frame is waited for, so the times are the GPU's (and the CPU's share
//! of building and submitting it). Each is printed as the median of its run
//! and, after the slash, the fastest frame: the GPU is shared with the
//! compositor, so background load only ever adds time and the minimum is the
//! closest reading of what the frame itself costs.

use std::time::{Duration, Instant};

use morf_layout::{Layout, Size, TextMeasurer, TextOptions};
use morf_render::{RenderEngine, WgpuBackend};
use morf_scene::{Behavior, Easing, Element, NodeHandle, Scene, Stretch, Value};

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

const THICK: f64 = 10.0;
const SEAM: f64 = 18.0;

struct Frame {
    scene: Scene,
    root: NodeHandle,
    drawers: Vec<(NodeHandle, &'static str, f64)>,
}

/// The frame, and four drawers tucked away (or out, `open`).
fn frame(width: f64, height: f64, open: bool, with_field: bool) -> Frame {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", width).unwrap();
    scene.assign(root, "height", height).unwrap();
    let field = scene.create(Element::Sdf);
    let fill = || Value::Map([("fill".to_owned(), Value::Bool(true))].into());
    scene.assign(field, "anchors", fill()).unwrap();
    scene.assign(field, "fill_color", "#1c1b22").unwrap();
    scene.assign(field, "blend", SEAM).unwrap();
    scene.assign(field, "blend_profile", "circular").unwrap();
    let outer = scene.create(Element::SdfShape);
    scene.assign(outer, "shape", "box").unwrap();
    scene.assign(outer, "anchors", fill()).unwrap();
    let inner = scene.create(Element::SdfShape);
    scene.assign(inner, "shape", "box").unwrap();
    scene
        .assign(
            inner,
            "anchors",
            Value::Map(
                [
                    ("fill".to_owned(), Value::Bool(true)),
                    ("margins".to_owned(), Value::Number(THICK)),
                ]
                .into(),
            ),
        )
        .unwrap();
    scene.assign(inner, "radius", 22.0).unwrap();
    scene.assign(inner, "operation", "subtract").unwrap();
    scene.reparent(outer, Some(field)).unwrap();
    scene.reparent(inner, Some(field)).unwrap();
    if with_field {
        scene.reparent(field, Some(root)).unwrap();
    }
    let specs = [
        (
            "translate_y",
            width / 2.0 - 210.0,
            THICK,
            420.0,
            150.0,
            -1.0,
        ),
        (
            "translate_y",
            width / 2.0 - 260.0,
            height - THICK - 120.0,
            520.0,
            120.0,
            1.0,
        ),
        (
            "translate_x",
            THICK,
            height / 2.0 - 180.0,
            260.0,
            360.0,
            -1.0,
        ),
        (
            "translate_x",
            width - THICK - 300.0,
            height / 2.0 - 210.0,
            300.0,
            420.0,
            1.0,
        ),
    ];
    let mut drawers = Vec::new();
    for (group, (axis, x, y, w, h, side)) in specs.into_iter().enumerate() {
        let panel = scene.create(Element::Item);
        for (property, value) in [("x", x), ("y", y), ("width", w), ("height", h)] {
            scene.assign(panel, property, value).unwrap();
        }
        let size = if axis == "translate_y" { h } else { w };
        let hidden = side * (size + THICK + SEAM + 2.0);
        scene
            .assign(panel, axis, if open { 0.0 } else { hidden })
            .unwrap();
        scene
            .set_behavior(
                panel,
                axis,
                Some(Behavior::timed(
                    Duration::from_millis(460),
                    Easing::OutCubic,
                )),
            )
            .unwrap();
        scene
            .set_stretch(
                panel,
                Some(Stretch {
                    scale: 0.16,
                    ..Stretch::default()
                }),
            )
            .unwrap();
        scene.reparent(panel, Some(root)).unwrap();
        let shape = scene.create(Element::SdfShape);
        scene.assign(shape, "shape", "box").unwrap();
        scene.assign(shape, "radius", 18.0).unwrap();
        scene.assign(shape, "operation", "smooth_union").unwrap();
        scene
            .assign(shape, "blend_group", (group + 1) as f64)
            .unwrap();
        scene.set_track(shape, Some(panel)).unwrap();
        scene.reparent(shape, Some(field)).unwrap();
        drawers.push((panel, axis, hidden));
    }
    Frame {
        scene,
        root,
        drawers,
    }
}

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

fn lay_out(frame: &mut Frame, size: Size) -> Layout {
    let layout = Layout::compute(&frame.scene, frame.root, size, &mut NoText).unwrap();
    morf_layout::observe_stretch(&mut frame.scene, &layout).unwrap();
    layout
}

/// The same picture drawn from nothing, over and over.
fn full(engine: &mut RenderEngine<WgpuBackend>, frame: &mut Frame, size: Size) -> Timing {
    let layout = lay_out(frame, size);
    let mut times = Vec::new();
    for _ in 0..60 {
        engine.forget();
        let start = Instant::now();
        engine.render(&frame.scene, &layout, 120, |_| {}).unwrap();
        engine.backend_mut().wait_idle();
        times.push(start.elapsed());
    }
    timing(times)
}

/// Every drawer opening at once, frame by frame: time and damaged area.
fn slide(
    engine: &mut RenderEngine<WgpuBackend>,
    frame: &mut Frame,
    size: Size,
) -> (Timing, u64, u64) {
    let layout = lay_out(frame, size);
    engine.forget();
    engine.render(&frame.scene, &layout, 120, |_| {}).unwrap();
    engine.backend_mut().wait_idle();
    for (panel, axis, _) in frame.drawers.clone() {
        frame.scene.assign(panel, axis, 0.0).unwrap();
    }
    let mut times = Vec::new();
    let (mut total, mut most) = (0u64, 0u64);
    for _ in 0..40 {
        frame
            .scene
            .tick_animations(Duration::from_millis(16))
            .unwrap();
        let layout = lay_out(frame, size);
        let start = Instant::now();
        let damage = engine.render(&frame.scene, &layout, 120, |_| {}).unwrap();
        engine.backend_mut().wait_idle();
        let area: u64 = damage
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum();
        if area > 0 {
            times.push(start.elapsed());
        }
        total += area;
        most = most.max(area);
    }
    (timing(times), total / 40, most)
}

fn main() {
    let (width, height) = std::env::args()
        .nth(1)
        .and_then(|size| {
            let (w, h) = size.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        })
        .unwrap_or((1920, 1080));
    let size = Size {
        width: f64::from(width),
        height: f64::from(height),
    };
    let backend = pollster::block_on(WgpuBackend::new(width, height)).expect("a GPU adapter");
    let mut engine = RenderEngine::new(backend);
    // Warm the pipelines.
    let mut warm = frame(size.width, size.height, true, true);
    full(&mut engine, &mut warm, size);
    println!("{width}x{height}, median/fastest");
    // The floor: one flat rectangle over the whole screen, which every full
    // repaint pays for the target at least.
    let mut flat = frame(size.width, size.height, true, false);
    let ground = flat.scene.create(Element::Rect);
    flat.scene.assign(ground, "width", size.width).unwrap();
    flat.scene.assign(ground, "height", size.height).unwrap();
    flat.scene.assign(ground, "color", "#1c1b22").unwrap();
    flat.scene.reparent(ground, Some(flat.root)).unwrap();
    println!(
        "  one flat fullscreen rect, full       {}",
        full(&mut engine, &mut flat, size)
    );
    let mut closed = frame(size.width, size.height, false, true);
    println!(
        "  frame, drawers tucked away, full    {}",
        full(&mut engine, &mut closed, size)
    );
    let mut open = frame(size.width, size.height, true, true);
    println!(
        "  frame, four drawers out, full       {}",
        full(&mut engine, &mut open, size)
    );
    let mut sliding = frame(size.width, size.height, false, true);
    let (time, mean, most) = slide(&mut engine, &mut sliding, size);
    let screen = u64::from(width) * u64::from(height);
    println!(
        "  four drawers sliding out, per frame {time}  damaged {mean} px on average, {most} at most ({:.1}% of the screen)",
        most as f64 * 100.0 / screen as f64
    );
}
