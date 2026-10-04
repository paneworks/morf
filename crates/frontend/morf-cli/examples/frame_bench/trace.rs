//! `frame_bench config.lua trace [seconds]`: follows the moving parts
//! instead of timing them, so a configuration whose motion misbehaves can be
//! reproduced without a compositor in the way.

use std::time::{Duration, Instant};

use morf_lua::Runtime;

pub fn run(runtime: &mut Runtime) {
    let mut moving = Vec::new();
    let mut stack = vec![runtime.scene().roots()[0]];
    while let Some(node) = stack.pop() {
        let scene = runtime.scene();
        if format!("{:?}", scene.element(node).expect("live")) == "SdfShape" {
            moving.push(node);
        }
        stack.extend(scene.children(node).expect("live").iter().copied());
    }
    println!("tracing {} shapes", moving.len());
    // In real time, deliberately. Timers fire off the wall clock, so a
    // configuration that applies forces from one — anything with parts
    // that pull on each other — sees those forces only if the trace takes
    // as long to run as the motion it is tracing. Racing through the
    // frames traces the motion with every force switched off.
    // Long enough to show a slow drift, which is the failure a short trace
    // cannot tell apart from an orbit.
    let frames: u64 = std::env::args()
        .nth(3)
        .and_then(|arg| arg.parse().ok())
        .map_or(600, |seconds: u64| seconds * 1000 / 16);
    let started = Instant::now();
    for frame in 0..frames {
        let due = Duration::from_millis(frame * 16);
        if let Some(rest) = due.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
        runtime.poll_services();
        let advanced = runtime
            .tick_animations(Duration::from_millis(16))
            .expect("tick");
        if frame % (frames / 20).max(1) == 0 || frame + 1 == frames {
            let scene = runtime.scene();
            let placed: Vec<(f64, f64, f64)> = moving
                .iter()
                .map(|node| {
                    let size = scene.number(*node, "width").unwrap_or(0.0);
                    (
                        scene.number(*node, "x").unwrap_or(f64::NAN) + size / 2.0,
                        scene.number(*node, "y").unwrap_or(f64::NAN) + size / 2.0,
                        size / 2.0,
                    )
                })
                .collect();
            // Three numbers say more about a swarm than its coordinates
            // do: where it sits, how far it reaches, and whether its parts
            // are still distinct or have piled into one lump.
            let count = placed.len() as f64;
            let mid_x = placed.iter().map(|p| p.0).sum::<f64>() / count;
            let mid_y = placed.iter().map(|p| p.1).sum::<f64>() / count;
            let reach = placed
                .iter()
                .map(|p| (p.0 - mid_x).hypot(p.1 - mid_y))
                .fold(0.0, f64::max);
            let mut closest = f64::INFINITY;
            for (index, a) in placed.iter().enumerate() {
                for b in &placed[index + 1..] {
                    closest = closest.min((a.0 - b.0).hypot(a.1 - b.1) - a.2 - b.2);
                }
            }
            // How far the worst blob has pushed past the edge of the
            // surface, which is the one failure that shows on screen as a
            // shape sliced flat rather than as motion that looks wrong.
            let root = scene.roots()[0];
            let wide = scene.number(root, "width").unwrap_or(0.0);
            let tall = scene.number(root, "height").unwrap_or(0.0);
            let escaped = placed
                .iter()
                .map(|p| {
                    (p.2 - p.0)
                        .max(p.0 + p.2 - wide)
                        .max(p.2 - p.1)
                        .max(p.1 + p.2 - tall)
                })
                .fold(f64::NEG_INFINITY, f64::max);
            let spread = format!(
                "at ({mid_x:.0},{mid_y:.0}) reach {reach:.0} closest {closest:+.0} escaped {escaped:+.0}"
            );
            println!(
                "  t={:>5}ms active={} {}",
                started.elapsed().as_millis(),
                advanced.active,
                spread
            );
        }
    }
}
