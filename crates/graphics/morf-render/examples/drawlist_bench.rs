//! Run with `cargo run --release -p morf-render --example drawlist_bench`.
//! Measures the CPU side of a frame -- the draw list and its damage -- on a
//! dashboard of a few hundred nodes where one property changes per frame.

use std::time::{Duration, Instant};

use morf_layout::{Layout, Size, TextMeasurer, TextOptions};
use morf_render::{
    CopiedRun, Copies, DamageRect, DamageTracker, DrawList, RenderBackend, RenderEngine,
};
use morf_scene::NodeHandle;

#[path = "common/dashboard.rs"]
mod dashboard;
use dashboard::{change, dashboard};

struct Fixed;

impl TextMeasurer for Fixed {
    fn measure(&mut self, _: NodeHandle, text: &str, _: &str, size: f64, _: TextOptions) -> Size {
        Size {
            width: text.len() as f64 * size / 2.0,
            height: size,
        }
    }
}

/// Draws nothing, so the engine's own work is all that is timed; keeps what
/// the last list copied, to time the damage between those runs alone.
#[derive(Default)]
struct Null {
    runs: Vec<CopiedRun>,
}

impl RenderBackend for Null {
    type Error = std::io::Error;

    fn render(&mut self, _: &DrawList, _: &[DamageRect], _: u32) -> Result<(), Self::Error> {
        Ok(())
    }

    fn render_copied(
        &mut self,
        _: &DrawList,
        copies: Copies<'_>,
        _: &[DamageRect],
        _: u32,
    ) -> Result<(), Self::Error> {
        self.runs = copies.runs.to_vec();
        Ok(())
    }

    fn resize(&mut self, _: u32, _: u32) {}
}

fn main() {
    let frames = 400;
    let size = Size {
        width: 1920.0,
        height: 1080.0,
    };
    for panels in [10, 40] {
        let mut board = dashboard(panels);
        let root = board.root;
        let nodes = board.scene.node_count();
        let mut layout = Layout::default();
        layout.update(&board.scene, root, size, &mut Fixed).unwrap();
        let mut list = DrawList::default();
        let mut tracker = DamageTracker::default();
        list.rebuild(&board.scene, &layout).unwrap();
        tracker.retain(&mut list);
        let mut engine = RenderEngine::new(Null::default());
        engine.render(&board.scene, &layout, 120, |_| {}).unwrap();
        let mut between = DamageTracker::default();
        between.diff(engine.last_list(), 120);
        between.retain(&mut engine.last_list().clone());
        let (mut rebuild, mut diff, mut kept) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
        let mut runs_diff = Duration::ZERO;
        for frame in 0..frames {
            change(&mut board, frame);
            let scene = &board.scene;
            layout.update(scene, root, size, &mut Fixed).unwrap();
            let started = Instant::now();
            list.rebuild(scene, &layout).unwrap();
            rebuild += started.elapsed();
            let started = Instant::now();
            std::hint::black_box(tracker.diff(&list, 120));
            diff += started.elapsed();
            tracker.retain(&mut list);
            let started = Instant::now();
            engine.render(scene, &layout, 120, |_| {}).unwrap();
            kept += started.elapsed();
            let fresh = DrawList::from_scene(scene, &layout).unwrap();
            assert!(
                *engine.last_list() == fresh,
                "frame {frame}: the kept list differs from one built afresh"
            );
            let mut made = engine.last_list().clone();
            let started = Instant::now();
            std::hint::black_box(between.diff_copied(&made, &engine.backend_mut().runs, 120));
            runs_diff += started.elapsed();
            between.retain(&mut made);
        }
        println!(
            "{nodes:5} nodes: whole rebuild {:?} + damage {:?}/frame; engine, copying unchanged subtrees, {:?}/frame, of which damage between copied runs {:?}",
            rebuild / frames as u32,
            diff / frames as u32,
            kept / frames as u32,
            runs_diff / frames as u32
        );
    }
}
