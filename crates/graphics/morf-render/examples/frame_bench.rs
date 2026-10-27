//! Run with `nixVulkanIntel cargo run --release -p morf-render --example
//! frame_bench`. Times whole frames through the GPU backend on the dashboard
//! the draw-list bench paints; `MORF_PROFILE=frame_split,render_split=0`
//! prints where each frame's render went.

use std::time::{Duration, Instant};

use morf_layout::{Layout, Size};
use morf_render::{RenderEngine, WgpuBackend};

#[path = "common/dashboard.rs"]
mod dashboard;
use dashboard::{change, dashboard};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let frames = 300;
    let size = Size {
        width: 1920.0,
        height: 1080.0,
    };
    for panels in [10, 40] {
        let backend = pollster::block_on(WgpuBackend::new(1920, 1080))?;
        let mut engine = RenderEngine::new(backend);
        let mut board = dashboard(panels);
        let root = board.root;
        let nodes = board.scene.node_count();
        let mut layout = Layout::default();
        layout.update(&board.scene, root, size, engine.backend_mut())?;
        engine.render(&board.scene, &layout, 120, |_| {})?;
        let mut spent = Duration::ZERO;
        for frame in 0..frames {
            change(&mut board, frame);
            layout.update(&board.scene, root, size, engine.backend_mut())?;
            let started = Instant::now();
            engine.render(&board.scene, &layout, 120, |_| {})?;
            spent += started.elapsed();
        }
        println!(
            "{nodes:5} nodes: {:?}/frame through the GPU backend",
            spent / frames as u32
        );
    }
    Ok(())
}
