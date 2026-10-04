//! `frame_bench config.lua gpu [WxH] [out.png]`: renders one frame on a real
//! adapter instead of timing anything -- a shader the driver refuses looks
//! fine from the CPU side, and the only way to find out is to build the
//! pipelines and draw, headless.

use std::time::Duration;

use morf_layout::Size;
use morf_lua::{Runtime, WindowSurfaceKind};
use morf_render::{BlendSpace, RenderEngine, ShaderRegistration, WgpuBackend};
use morf_scene::NodeHandle;

use super::settled;

pub fn run(
    runtime: &mut Runtime,
    root: NodeHandle,
    size: Size,
    config: &str,
    picture: Option<&String>,
) {
    let (width, height) = (size.width, size.height);
    let backend =
        pollster::block_on(WgpuBackend::new(width as u32, height as u32)).expect("a GPU adapter");
    let mut engine = RenderEngine::new(backend);
    // The surface's own blend space, as the shell would paint it.
    let blend = BlendSpace::parse(&runtime.layer_surface_config().blend).unwrap_or_default();
    engine.backend_mut().set_blend(blend);
    // Settled again against the real faces: what the frame draws is
    // shaped by the renderer's text system, and a binding placing
    // something beside a label must read that label's real width, not
    // the ruled estimate the timings use.
    let mut computed = settled(runtime, root, size, engine.backend_mut(), config);
    let mut shaders = 0usize;
    for shader in runtime.shaders() {
        engine
            .backend_mut()
            .register_shader(ShaderRegistration {
                program: shader.program,
                wgsl: Some(&shader.wgsl),
                vertex: shader.vertex.as_deref(),
                offsets: &shader.offsets,
                uniform_size: shader.uniform_size,
                owns_coverage: shader.owns_coverage,
                effect: shader.samples_behind,
                textures: &shader.textures,
                data: &shader.data,
            })
            .unwrap_or_else(|error| panic!("{config}: shader pipeline: {error}"));
        shaders += 1;
    }
    // Twice: the second frame is incremental and reuses an effect layer's
    // target; `FRAME_BENCH_GPU_FRAMES` draws more, for `MORF_GPU_WAIT=1`.
    let frames: usize =
        std::env::var("FRAME_BENCH_GPU_FRAMES").map_or(2, |value| value.parse().unwrap_or(2));
    for _ in 0..frames.max(2) {
        if frames > 2 {
            let _ = runtime.tick_animations(Duration::from_millis(16));
            computed = settled(runtime, root, size, engine.backend_mut(), config);
        }
        runtime.sync_text_inputs(&computed, engine.backend_mut().text_system());
        runtime.observe_stretch(&computed);
        engine
            .render(&runtime.scene(), &computed, 120, |_| {})
            .unwrap_or_else(|error| panic!("{config}: render: {error}"));
    }
    println!("{config}");
    println!("  {shaders} shader(s) built and one frame drawn on the GPU");
    // A fourth argument names a PNG to write the frame to. Every gate in
    // this repository can pass while a shader is visibly wrong, and the
    // only way to find that out is to look at what it drew.
    //
    // Every visible `morf.window.layer` surface is drawn too, each by its
    // own renderer in its own blend space, and laid over the shell's
    // surface where its anchors and margins put it.
    if let Some(path) = picture {
        let (full_width, full_height) = (width as u32, height as u32);
        let mut picture = engine.backend_mut().read_pixels();
        for surface in runtime.window_surface_configs() {
            let WindowSurfaceKind::Layer(config) = &surface.kind else {
                continue;
            };
            if !surface.visible {
                continue;
            }
            let anchors = config.anchors;
            let stretched = |near: bool, far: bool, size: u32, full: u32| {
                if size == 0 || (near && far) {
                    full
                } else {
                    size.min(full)
                }
            };
            let surface_width = stretched(anchors.left, anchors.right, config.width, full_width);
            let surface_height = stretched(anchors.top, anchors.bottom, config.height, full_height);
            let place = |near: bool, far: bool, size: u32, full: u32, before: i32, after: i32| {
                let free = i64::from(full) - i64::from(size);
                let at = match (near, far) {
                    (true, false) => i64::from(before),
                    (false, true) => free - i64::from(after),
                    _ => free / 2,
                };
                at.clamp(0, free.max(0)) as u32
            };
            let x = place(
                anchors.left,
                anchors.right,
                surface_width,
                full_width,
                config.margin_left,
                config.margin_right,
            );
            let y = place(
                anchors.top,
                anchors.bottom,
                surface_height,
                full_height,
                config.margin_top,
                config.margin_bottom,
            );
            let backend = pollster::block_on(WgpuBackend::new(surface_width, surface_height))
                .expect("a GPU adapter");
            let mut surface_engine = RenderEngine::new(backend);
            surface_engine
                .backend_mut()
                .set_blend(BlendSpace::parse(&config.blend).unwrap_or_default());
            let surface_layout = settled(
                runtime,
                surface.root,
                Size {
                    width: f64::from(surface_width),
                    height: f64::from(surface_height),
                },
                surface_engine.backend_mut(),
                &config.namespace,
            );
            runtime.sync_text_inputs(&surface_layout, surface_engine.backend_mut().text_system());
            surface_engine
                .render(&runtime.scene(), &surface_layout, 120, |_| {})
                .unwrap_or_else(|error| panic!("{}: render: {error}", config.namespace));
            let pixels = surface_engine.backend_mut().read_pixels();
            // Premultiplied over, byte for byte, as the compositor lays
            // one surface on another.
            for row in 0..surface_height {
                for column in 0..surface_width {
                    let from = ((row * surface_width + column) * 4) as usize;
                    let to = (((y + row) * full_width + x + column) * 4) as usize;
                    let alpha = u32::from(pixels[from + 3]);
                    for channel in 0..4 {
                        let under = u32::from(picture[to + channel]);
                        picture[to + channel] = (u32::from(pixels[from + channel])
                            + (under * (255 - alpha) + 127) / 255)
                            .min(255) as u8;
                    }
                }
            }
        }
        image::RgbaImage::from_raw(full_width, full_height, picture)
            .expect("the readback is the size of the target")
            .save(path)
            .expect("the image is written");
        println!("  written to {path}");
    }
}
