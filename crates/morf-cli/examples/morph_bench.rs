//! Times layout through a morph, frame by frame, without a compositor.
//!
//! Loads a configuration, calls one of its IPC verbs (a panel opening, say),
//! then ticks the clock a refresh at a time and lays out every surface's tree
//! after every tick, the way the shell does. Each frame is laid out twice:
//! once from scratch and once incrementally from the frame before, so the two
//! can be timed side by side and checked against each other.
//!
//! ```sh
//! IMPASTO_DRY_RUN=1 cargo run --release -p morf-cli --example morph_bench -- \
//!     examples/impasto/init.lua controls close
//! ```
//!
//! Text is shaped by the real text system, since shaping is part of what a
//! morph frame's layout pays for. The whole pass gets a text system of its
//! own, as it would in a shell that lays out whole every frame: sharing one
//! would hand it the incremental pass's shaping as cache hits.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use morf_layout::{Layout, Size};
use morf_lua::{Limits, Runtime, Screen, WindowSurfaceKind};
use morf_scene::NodeHandle;
use morf_text::TextSystem;

struct Surface {
    name: String,
    root: NodeHandle,
    size: Size,
    layout: Layout,
    full_text: TextSystem,
    text: TextSystem,
    full: Vec<f64>,
    incremental: Vec<f64>,
    mismatches: usize,
}

fn surfaces(runtime: &Runtime, screen: Size) -> Vec<(String, NodeHandle, Size)> {
    let mut out = Vec::new();
    let windows: Vec<_> = runtime.window_surface_configs().to_vec();
    for surface in &windows {
        if let WindowSurfaceKind::Layer(config) = &surface.kind {
            let anchors = config.anchors;
            let extent = |near: bool, far: bool, size: u32, full: f64| {
                if size == 0 || (near && far) {
                    full
                } else {
                    f64::from(size)
                }
            };
            out.push((
                config.namespace.clone(),
                surface.root,
                Size {
                    width: extent(anchors.left, anchors.right, config.width, screen.width),
                    height: extent(anchors.top, anchors.bottom, config.height, screen.height),
                },
            ));
        }
    }
    let primary = runtime
        .scene()
        .roots()
        .into_iter()
        .find(|root| !windows.iter().any(|surface| surface.root == *root))
        .expect("a primary root");
    out.insert(0, ("primary".into(), primary, screen));
    out
}

fn subtree(runtime: &Runtime, root: NodeHandle) -> usize {
    let scene = runtime.scene();
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        count += 1;
        stack.extend(scene.children(node).map(<[_]>::to_vec).unwrap_or_default());
    }
    count
}

fn run_phase(runtime: &mut Runtime, surfaces: &mut [Surface], frames: usize) {
    for surface in surfaces.iter_mut() {
        surface.full.clear();
        surface.incremental.clear();
        surface.mismatches = 0;
    }
    for _ in 0..frames {
        runtime.poll_services();
        let _ = runtime
            .tick_frame_animations(Duration::from_millis(16))
            .expect("tick");
        for surface in surfaces.iter_mut() {
            if !runtime.scene().contains(surface.root) {
                continue;
            }
            let started = Instant::now();
            if let Err(error) = runtime.update_layout(
                &mut surface.layout,
                surface.root,
                surface.size,
                &mut surface.text,
            ) {
                eprintln!("  {}: {error}", surface.name);
                continue;
            }
            surface
                .incremental
                .push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            let full = runtime
                .compute_layout(surface.root, surface.size, &mut surface.full_text)
                .expect("layout");
            surface.full.push(started.elapsed().as_secs_f64() * 1000.0);
            if let Some(difference) = full.difference(&surface.layout) {
                if surface.mismatches == 0 {
                    eprintln!("  {}: incremental differs: {difference}", surface.name);
                }
                surface.mismatches += 1;
            }
            runtime.observe_layout(&surface.layout);
        }
    }
}

fn describe(values: &[f64]) -> String {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = values.iter().sum::<f64>() / values.len().max(1) as f64;
    format!(
        "mean {mean:.3} median {:.3} max {:.3}",
        sorted.get(sorted.len() / 2).copied().unwrap_or(0.0),
        sorted.last().copied().unwrap_or(0.0)
    )
}

fn report(name: &str, surfaces: &[Surface]) {
    println!("{name}:");
    let frames = surfaces.iter().map(|s| s.full.len()).max().unwrap_or(0);
    let total = |pick: fn(&Surface) -> &Vec<f64>| {
        (0..frames)
            .map(|frame| {
                surfaces
                    .iter()
                    .filter_map(|surface| pick(surface).get(frame))
                    .sum::<f64>()
            })
            .collect::<Vec<_>>()
    };
    println!("  all surfaces, per frame (ms):");
    println!("    full        {}", describe(&total(|s| &s.full)));
    println!("    incremental {}", describe(&total(|s| &s.incremental)));
    for surface in surfaces {
        let full = surface.full.iter().sum::<f64>();
        if full / (frames.max(1) as f64) < 0.05 {
            continue;
        }
        println!(
            "  {:<24} full {}  |  incremental {}  mismatches {}",
            surface.name,
            describe(&surface.full),
            describe(&surface.incremental),
            surface.mismatches
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(config) = args.get(1) else {
        eprintln!("usage: morph_bench <config.lua> [open-verb] [close-verb] [WxH]");
        std::process::exit(2);
    };
    let open = args.get(2).map_or("controls", String::as_str);
    let close = args.get(3).map_or("close", String::as_str);
    let (width, height) = args
        .get(4)
        .and_then(|value| value.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?)))
        .unwrap_or((1920.0, 1080.0));
    let mut runtime = Runtime::for_screen(
        Limits::default(),
        Screen {
            name: "morph-bench".into(),
            width: Some(width as i32),
            height: Some(height as i32),
            scale: 1,
            ..Screen::default()
        },
    );
    if let Some(parent) = PathBuf::from(config).parent() {
        runtime.set_module_roots(morf_lua::runtimepath_roots(
            std::path::Path::new(config),
            true,
        ));
        runtime.set_shell_root(parent.to_path_buf());
    }
    let source = std::fs::read(config).expect("configuration is readable");
    if let Err(error) = runtime.execute(config, &source) {
        eprintln!("{config}: {error}");
        std::process::exit(1);
    }
    for _ in 0..30 {
        runtime.poll_services();
        runtime
            .tick_animations(Duration::from_millis(16))
            .expect("animations tick");
    }
    let screen = Size { width, height };
    let mut all: Vec<Surface> = surfaces(&runtime, screen)
        .into_iter()
        .map(|(name, root, size)| Surface {
            name,
            root,
            size,
            layout: Layout::default(),
            full_text: TextSystem::new(),
            text: TextSystem::new(),
            full: Vec::new(),
            incremental: Vec::new(),
            mismatches: 0,
        })
        .collect();
    for surface in &all {
        println!(
            "  {:<24} {:>5} nodes  {}x{}",
            surface.name,
            subtree(&runtime, surface.root),
            surface.size.width,
            surface.size.height
        );
    }
    run_phase(&mut runtime, &mut all, 20);
    report("at rest", &all);
    runtime.call_ipc(open, &[]).expect("open verb");
    run_phase(&mut runtime, &mut all, 40);
    report(&format!("`{open}` (40 frames)"), &all);
    runtime.call_ipc(close, &[]).expect("close verb");
    run_phase(&mut runtime, &mut all, 40);
    report(&format!("`{close}` (40 frames)"), &all);
}
