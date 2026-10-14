//! Moving a backend whose device was lost onto a fresh one, in place.
//!
//! The window outlives the device: a swapchain's `wgpu::Surface` belongs to
//! the instance and is configured again on the new device, and a sink is the
//! window's own connection, which only needs new buffers. Everything made on
//! the old device -- targets, pipelines, atlases, registered shaders, cached
//! textures -- goes with it; what was worked out on the CPU (the fonts and
//! their shaping) is kept.

use super::super::backend_types::{GpuError, WgpuBackend};
use super::device::{ready, shared_instance};

impl WgpuBackend {
    /// Rebuilds this backend on a fresh device after its own was lost.
    ///
    /// The surface it presents to and its settings (blend space, depth,
    /// subpixel text, opacity, clock) carry over. Registered shaders do not: their
    /// pipelines were the old device's, so whoever registered them does so
    /// again, and the next frame has to be drawn in full.
    pub fn recover(&mut self) -> Result<(), GpuError> {
        let surface = self.surface.take().map(|state| state.surface);
        let sink = self.buffers.take().and_then(|ring| ring.into_sink());
        let mut fresh = ready(Self::initialize(
            shared_instance(),
            surface,
            self.width,
            self.height,
        ))?;
        if let Some(sink) = sink {
            fresh.attach_sink(sink).map_err(GpuError::Surface)?;
        }
        fresh.set_blend(self.space.blend);
        fresh.set_subpixel_text(self.subpixel);
        fresh.opaque_surface = self.opaque_surface;
        fresh.set_output_depth(self.depth);
        fresh.elapsed = self.elapsed;
        std::mem::swap(&mut fresh.text, &mut self.text);
        std::mem::swap(&mut fresh.drawings, &mut self.drawings);
        *self = fresh;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::device::open_device;
    use super::*;
    use crate::gpu::tests::test_quad;
    use crate::{DamageRect, DrawList, RenderEngine};
    use morf_scene::Color;

    fn whole(size: u32) -> [DamageRect; 1] {
        [DamageRect {
            x: 0,
            y: 0,
            width: size,
            height: size,
        }]
    }

    fn list(scene: &mut morf_scene::Scene, color: Color) -> DrawList {
        let node = scene.create(morf_scene::Element::Rect);
        DrawList {
            commands: vec![test_quad(node, color, Color::rgba8(0, 0, 0, 0), 0.0)],
            layers: Vec::new(),
        }
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn a_lost_device_is_replaced_and_the_next_frames_draw() {
        require_gpu!();
        let private = ready(open_device(&shared_instance(), None)).expect("an adapter");
        let device = private.device.clone();
        let backend = WgpuBackend::build(private, None, 4, 4).expect("a backend");
        let mut engine = RenderEngine::new(backend);
        let mut scene = morf_scene::Scene::new();
        let red = list(&mut scene, Color::rgba8(200, 30, 40, 255));
        let backend = engine.backend_mut();
        crate::RenderBackend::render(backend, &red, &whole(4), 120).expect("a first frame");
        let before = backend.read_pixels();
        assert!(near(&before[..4], [200, 30, 40, 255]), "{:?}", &before[..4]);

        device.destroy();
        let _ = device.poll(wgpu::PollType::Poll);
        assert!(engine.backend_mut().device_lost());
        let failed = crate::RenderBackend::render(engine.backend_mut(), &red, &whole(4), 120);
        assert!(failed.is_err_and(|error| error.is_device_lost()));

        engine.backend_mut().recover().expect("a fresh device");
        engine.forget();
        let backend = engine.backend_mut();
        assert!(!backend.device_lost());
        crate::RenderBackend::render(backend, &red, &whole(4), 120).expect("a frame after");
        assert_eq!(backend.read_pixels(), before, "the same frame, redrawn");

        let blue = list(&mut scene, Color::rgba8(20, 40, 220, 255));
        crate::RenderBackend::render(backend, &blue, &whole(4), 120).expect("a later frame");
        let after = backend.read_pixels();
        assert!(near(&after[..4], [20, 40, 220, 255]), "{:?}", &after[..4]);
    }

    fn near(pixel: &[u8], expected: [u8; 4]) -> bool {
        pixel
            .iter()
            .zip(expected)
            .all(|(got, want)| got.abs_diff(want) <= 2)
    }
}
