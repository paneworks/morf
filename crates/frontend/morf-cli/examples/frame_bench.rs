//! Times the work a frame does on the CPU, without a compositor.
//!
//! Layout, the draw list and the input region are the three things every paint
//! performs before anything reaches the GPU, and all three are pure functions
//! of the scene — so they can be measured exactly, repeatably, and without a
//! display. Run it against any configuration:
//!
//! ```sh
//! cargo run --release -p morf-cli --example frame_bench -- examples/shells/caelestia/shell/init.lua
//! ```
//!
//! Numbers are the fastest of several batches. Background load only ever adds
//! time, so the minimum is the closest estimate of the work itself; an average
//! would mostly measure whatever else the machine was doing.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use morf_layout::{Layout, Size, TextMeasurer, TextOptions};
use morf_lua::{Limits, Runtime, Screen};
use morf_render::DrawList;
use morf_scene::{Element, NodeHandle};

#[path = "frame_bench/gpu.rs"]
mod gpu;
#[path = "frame_bench/trace.rs"]
mod trace;

/// Text measured by a rule rather than a font stack.
///
/// The point is to time layout, not shaping — and shaping is already cached per
/// node behind its own input key, so including it would measure a cache hit.
struct RuledText;

impl TextMeasurer for RuledText {
    fn measure(
        &mut self,
        _node: NodeHandle,
        text: &str,
        _family: &str,
        size: f64,
        _options: TextOptions,
    ) -> Size {
        Size {
            width: text.chars().count() as f64 * size * 0.6,
            height: size * 1.2,
        }
    }

    fn measure_image(
        &mut self,
        _node: NodeHandle,
        _element: Element,
        _source: &str,
        _theme: Option<&str>,
    ) -> Option<Size> {
        None
    }
}

/// The fastest of `batches` runs of `body`, per iteration.
fn best(batches: u32, runs: u32, mut body: impl FnMut()) -> Duration {
    let runs = std::env::var("FRAME_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|runs| *runs > 0)
        .map_or(runs, |limit| runs.min(limit));
    let mut best = Duration::MAX;
    for _ in 0..batches {
        let start = Instant::now();
        for _ in 0..runs {
            body();
        }
        best = best.min(start.elapsed() / runs);
    }
    best
}

/// How many layout passes a picture waits for the bindings that read the
/// layout. A handful is plenty for a real configuration; the bound is there
/// for one that feeds its own geometry back and never stops.
const SETTLE_PASSES: usize = 8;

/// Lays the scene out until the layout-reading bindings agree with it.
fn settled(
    runtime: &mut Runtime,
    root: NodeHandle,
    size: Size,
    text: &mut impl TextMeasurer,
    config: &str,
) -> Layout {
    let settled = runtime
        .settle_layout(root, size, text, SETTLE_PASSES)
        .unwrap_or_else(|error| panic!("{config}: layout: {error}"));
    if !settled.stable {
        eprintln!(
            "{config}: layout still moving after {} passes; drawing the last",
            settled.passes
        );
    }
    settled.layout
}

fn main() {
    let Some(config) = std::env::args().nth(1) else {
        eprintln!("usage: frame_bench <config.lua> [width] [height]");
        std::process::exit(2);
    };
    // `frame_bench config.lua gpu [WxH] [out.png]`: in GPU mode the size rides
    // in one argument, so the picture can be of a phone as easily as this screen.
    let args: Vec<String> = std::env::args().collect();
    let gpu_size = args
        .get(3)
        .filter(|_| args.get(2).map(String::as_str) == Some("gpu"))
        .and_then(|value| value.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?)));
    let argument = |index: usize, fallback: f64| {
        args.get(index)
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    };
    let width: f64 = gpu_size.map_or_else(|| argument(2, 3456.0), |size| size.0);
    let height: f64 = gpu_size.map_or_else(|| argument(3, 2160.0), |size| size.1);

    let mut runtime = Runtime::for_screen(
        Limits::default(),
        Screen {
            name: "frame-bench".into(),
            width: Some(width as i32),
            height: Some(height as i32),
            scale: 1,
            ..Screen::default()
        },
    );
    if let Some(parent) = PathBuf::from(&config).parent() {
        // The roots the shell itself gives a configuration: its folder, then
        // `MORF_RUNTIME_PATH` and the user's site folder, so a configuration
        // whose library lives on the runtime path loads here too.
        runtime.set_module_roots(morf_lua::runtimepath_roots(
            std::path::Path::new(&config),
            true,
        ));
        // The same root the shell gives a configuration, and for the same
        // reason: `core.shell_path` is how a configuration names a file beside
        // itself. Leaving it at the default made this bench resolve those paths
        // against the working directory instead, so a configuration could pass
        // every gate here and find nothing at all when the shell ran it.
        runtime.set_shell_root(parent.to_path_buf());
    }
    let source = std::fs::read(&config).expect("configuration is readable");
    if let Err(error) = runtime.execute(&config, &source) {
        eprintln!("{config}: {error}");
        std::process::exit(1);
    }
    // Let the services settle so the scene is the one a running shell has.
    // A service on another thread — the sound server, a bus — answers in
    // milliseconds rather than instantly; `FRAME_BENCH_SETTLE_MS` spreads the
    // polls over that long so the picture has what it said.
    let settle = std::env::var("FRAME_BENCH_SETTLE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(Duration::ZERO, |ms| Duration::from_millis(ms) / 30);
    for _ in 0..30 {
        std::thread::sleep(settle);
        runtime.poll_services();
        runtime
            .tick_animations(Duration::from_millis(16))
            .expect("animations tick");
    }

    // Reproduce idle service/timer work without changing a running shell.
    // Run on the wall clock: repeating timers are not driven by animation time.
    if args.get(2).map(String::as_str) == Some("idle") {
        let seconds = args.get(3).and_then(|v| v.parse::<u64>().ok()).unwrap_or(5);
        morf_lua::profile::clear();
        let started = Instant::now();
        let mut polls = 0;
        let mut repaints = 0;
        let mut busy = Duration::ZERO;
        while started.elapsed() < Duration::from_secs(seconds) {
            let turn = Instant::now();
            repaints += usize::from(runtime.poll_services());
            runtime
                .tick_animations(Duration::from_millis(16))
                .expect("tick");
            busy += turn.elapsed();
            polls += 1;
            std::thread::sleep(Duration::from_millis(16).saturating_sub(turn.elapsed()));
        }
        println!(
            "{polls} polls, {repaints} repaint requests, {:.2} ms work, motion={} shaders={}",
            busy.as_secs_f64() * 1000.0,
            runtime.has_motion(),
            runtime.shaders_animate()
        );
        println!(
            "  {} nodes, {} property slots",
            runtime.scene().node_count(),
            runtime.scene().property_signal_count()
        );
        for line in runtime.motion_report(15) {
            println!("  moving: {line}");
        }
        for line in morf_lua::profile::report(25) {
            println!("  {line}");
        }
        return;
    }

    // `trace` follows the moving parts instead of timing them, so a
    // configuration whose motion misbehaves can be reproduced without a
    // compositor in the way.
    if std::env::args().nth(2).as_deref() == Some("trace") {
        trace::run(&mut runtime);
        return;
    }

    // What a shell costs when nothing is animating: the run loop wakes ten
    // times a second and asks the services what happened, whether or not
    // anything is painted afterwards.
    let idle = best(8, 60, || {
        std::hint::black_box(runtime.poll_services());
    });
    let tick = best(8, 60, || {
        std::hint::black_box(
            runtime
                .tick_animations(Duration::from_millis(16))
                .expect("animations tick"),
        );
    });

    // A configuration with several surfaces has a root for each;
    // `FRAME_BENCH_ROOT` picks which one is measured and drawn.
    let root = {
        let roots = runtime.scene().roots().to_vec();
        let index = std::env::var("FRAME_BENCH_ROOT")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        roots[index.min(roots.len() - 1)]
    };
    let size = Size { width, height };
    let computed = settled(&mut runtime, root, size, &mut RuledText, &config);
    // `gpu` renders one frame on a real adapter instead of timing anything: a
    // shader the driver refuses looks fine from the CPU side, and the only way
    // to find out is to build the pipelines and draw, headless.
    if std::env::args().nth(2).as_deref() == Some("gpu") {
        let picture = args.get(if gpu_size.is_some() { 4 } else { 3 });
        gpu::run(&mut runtime, root, size, &config, picture);
        return;
    }
    let scene = runtime.scene();

    // Layout reads about a dozen properties per node, so this is the floor the
    // rest of the pass is built on.
    let mut all = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        all.push(node);
        stack.extend(scene.children(node).expect("live node").iter().copied());
    }
    const PROBED: [&str; 12] = [
        "x",
        "y",
        "width",
        "height",
        "visible",
        "opacity",
        "rotation",
        "scale",
        "implicit_width",
        "implicit_height",
        "z",
        "clip",
    ];
    let reads = best(12, 200, || {
        for &node in &all {
            for name in PROBED {
                std::hint::black_box(scene.number(node, name).ok());
            }
        }
    });

    let layout = best(12, 200, || {
        std::hint::black_box(Layout::compute(&scene, root, size, &mut RuledText).expect("layout"));
    });
    let reuse = best(12, 200, || {
        std::hint::black_box(computed.clone());
    });
    let mut reused = DrawList::default();
    let draw = best(12, 200, || {
        reused.rebuild(&scene, &computed).expect("draw list");
        std::hint::black_box(&reused);
    });
    let draw_fresh = best(12, 200, || {
        std::hint::black_box(DrawList::from_scene(&scene, &computed).expect("draw list"));
    });
    let region = best(12, 200, || {
        std::hint::black_box(computed.input_geometry(&scene).expect("input geometry"));
    });

    // Shaders are counted because the way they fail is silent. A material
    // shader that never reached a command, or an effect whose layer holds
    // nothing to sample, renders as a configuration that simply ignored it —
    // and the only place that is visible without a GPU is here.
    let shaded = DrawList::from_scene(&scene, &computed).expect("draw list");
    let material = shaded
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                morf_render::DrawCommand::Field {
                    shader: Some(_),
                    ..
                } | morf_render::DrawCommand::Quad {
                    shader: Some(_),
                    ..
                }
            )
        })
        .count();
    // What an effect layer holds *other than the node carrying it*. A rectangle
    // laid over its siblings still draws its own quad, so counting commands
    // would say 1 and mean nothing; the question is whether any content came
    // with it.
    let effects: Vec<usize> = shaded
        .layers
        .iter()
        .filter(|layer| layer.shader.is_some())
        .map(|layer| {
            shaded.commands[layer.commands.clone()]
                .iter()
                .filter(|command| command.node() != layer.node)
                .count()
        })
        .collect();

    // Nodes asking the compositor to blur behind them. Reported because the
    // failure is silent from up here: the region is derived on the CPU and
    // handed to the compositor, so a configuration that never sets the property
    // and one whose compositor ignores it look exactly alike on screen.
    let backdrop_shapes = computed.backdrop_geometry(&scene).unwrap_or_default();
    let backdrops = backdrop_shapes.len();
    // How many rectangles the region rasterises to, which is the only way to
    // see the shape from here: a square is one, and a circle is one span per
    // scanline. A blur region that came out as a box when a circle was asked
    // for looks identical from every other angle.
    let backdrop_rects = if backdrop_shapes.is_empty() {
        0
    } else {
        let shapes: Vec<morf_value::region::Region> = backdrop_shapes
            .iter()
            .map(|(geometry, radii)| morf_value::region::Region {
                rect: morf_value::region::Rect {
                    x: geometry.x.floor() as i32,
                    y: geometry.y.floor() as i32,
                    width: (geometry.width.ceil() as i32).max(0),
                    height: (geometry.height.ceil() as i32).max(0),
                },
                shape: morf_value::region::Shape::Box,
                params: morf_value::region::ShapeParams {
                    radii: *radii,
                    ..morf_value::region::ShapeParams::default()
                },
                ..morf_value::region::Region::default()
            })
            .collect();
        morf_value::region::build_scaled(
            width as u32,
            height as u32,
            &shapes,
            morf_value::region::COVERED_EDGE_GRID,
        )
        .map(|rects| rects.len())
        .unwrap_or(0)
    };
    // Rasterising a blur region is CPU work, done per surface per frame, and it
    // scales with the surface — so on a full-screen overlay it is one of the
    // largest single costs in the frame and none of the other numbers here
    // would show it.
    let backdrop_cost = if backdrop_shapes.is_empty() {
        Duration::ZERO
    } else {
        let shapes: Vec<morf_value::region::Region> = backdrop_shapes
            .iter()
            .map(|(geometry, radii)| morf_value::region::Region {
                rect: morf_value::region::Rect {
                    x: geometry.x.floor() as i32,
                    y: geometry.y.floor() as i32,
                    width: (geometry.width.ceil() as i32).max(0),
                    height: (geometry.height.ceil() as i32).max(0),
                },
                shape: morf_value::region::Shape::Box,
                params: morf_value::region::ShapeParams {
                    radii: *radii,
                    ..morf_value::region::ShapeParams::default()
                },
                ..morf_value::region::Region::default()
            })
            .collect();
        best(5, 20, || {
            std::hint::black_box(
                morf_value::region::build_scaled(
                    width as u32,
                    height as u32,
                    &shapes,
                    morf_value::region::COVERED_EDGE_GRID,
                )
                .ok(),
            );
        })
    };

    let frame = layout + draw + region;
    println!("{config}");
    let nodes = all.len();
    println!("  scene nodes        {nodes}");
    println!("  property slots     {}", scene.property_signal_count());
    if backdrops > 0 {
        println!(
            "  backdrop regions   {backdrops}  ({backdrop_rects} rectangles, {backdrop_cost:?} to rasterise)"
        );
    }
    if material > 0 || !effects.is_empty() {
        println!("  shaded commands    {material}");
        for covered in &effects {
            println!(
                "  effect layer       {covered} command(s) to sample{}",
                if *covered == 0 {
                    "  <-- wraps nothing, so it will sample an empty target"
                } else {
                    ""
                },
            );
        }
    }
    println!(
        "  DrawCommand size   {} bytes  ({} KiB for this scene)",
        std::mem::size_of::<morf_render::DrawCommand>(),
        std::mem::size_of::<morf_render::DrawCommand>() * nodes / 1024
    );
    println!("  poll_services      {idle:?}");
    println!(
        "  property read      {:.1}ns  ({} reads)",
        reads.as_secs_f64() * 1e9 / (all.len() * PROBED.len()) as f64,
        all.len() * PROBED.len()
    );
    println!("  tick_animations    {tick:?}");
    println!("  Layout::compute    {layout:?}");
    println!("  Layout reuse       {reuse:?}");
    println!("  DrawList (reused)  {draw:?}");
    println!("  DrawList (fresh)   {draw_fresh:?}");
    println!("  input_geometry     {region:?}");
    println!("  frame CPU          {frame:?}");
    println!(
        "  at 60fps           {:.1}% of one core per output",
        frame.as_secs_f64() * 60.0 * 100.0
    );
}
