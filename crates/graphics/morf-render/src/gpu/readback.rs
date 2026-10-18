//! Reading the offscreen target back to ordinary memory.
//!
//! Only useful when there is no compositor to hand the frame to: tests
//! asserting on pixels, and tools that want to look at what a configuration
//! actually draws. A shader can pass every gate there is and still be wrong in
//! a way only looking at it will reveal, and until this existed the only way to
//! look was to run the whole shell.

use super::backend_types::WgpuBackend;

impl WgpuBackend {
    /// The offscreen target's pixels, tightly packed as RGBA8.
    ///
    /// Rows come back with no padding, so a caller can index by `(y * width +
    /// x) * 4`. The target is sRGB, which is what an image file wants anyway;
    /// a deep one is brought down to eight bits.
    pub fn read_pixels(&mut self) -> Vec<u8> {
        let texture = self.texture.clone();
        let bytes = self.read_texture(&texture);
        if texture.format() == wgpu::TextureFormat::Rgba16Float {
            return half_to_rgba8(&bytes, self.space.blend == crate::BlendSpace::Linear);
        }
        bytes
    }

    /// A texture's bytes as it stores them, tightly packed.
    pub(crate) fn read_texture(&self, texture: &wgpu::Texture) -> Vec<u8> {
        let width = texture.width();
        let height = texture.height();
        let texel = texture.format().block_copy_size(None).unwrap_or(4);
        // Copies out of a texture want rows aligned to 256 bytes, so the
        // buffer is wider than the image and the rows are repacked
        // afterwards.
        let bytes_per_row = (width * texel).next_multiple_of(256);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("morf readback"),
            size: u64::from(bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("morf readback copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        super::present::submit(&self.queue, [encoder.finish()], || {});
        let slice = buffer.slice(..);
        let (send, receive) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the queue drains");
        receive.recv().expect("the map completes").expect("mapped");
        let mapped = slice.get_mapped_range().expect("the range is readable");
        let mut out = Vec::with_capacity((width * height * texel) as usize);
        for row in 0..height {
            let start = (row * bytes_per_row) as usize;
            out.extend_from_slice(&mapped[start..start + (width * texel) as usize]);
        }
        out
    }
}

/// A half-float target's pixels as sRGB-encoded RGBA8: encoded on the way
/// when the target holds linear light.
fn half_to_rgba8(bytes: &[u8], linear: bool) -> Vec<u8> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .enumerate()
        .map(|(index, half)| {
            let value = half_to_f32(u16::from_le_bytes(*half)).clamp(0.0, 1.0);
            let value = if linear && index % 4 != 3 {
                encode_srgb(value)
            } else {
                value
            };
            (value * 255.0).round() as u8
        })
        .collect()
}

pub(crate) fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f32::from(bits & 0x3ff);
    sign * match exponent {
        0 => mantissa * 2f32.powi(-24),
        31 => f32::INFINITY,
        _ => (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15),
    }
}

pub(crate) fn encode_srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

impl WgpuBackend {
    /// Publishes pixels under a name `ui.Image` can resolve as `memory:<name>`.
    ///
    /// The way a screen capture reaches a configuration's own scene. Without it
    /// the capture protocols hand over pixels that nothing can display, which
    /// is half a feature: an overview needs the thumbnail on screen, not in a
    /// buffer.
    ///
    /// Replaces whatever the name held, so a thumbnail that refreshes does not
    /// leak an image per refresh.
    pub fn publish_image(
        &mut self,
        name: impl Into<String>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) {
        self.images.insert_memory(
            name,
            morf_image::ImageData {
                width,
                height,
                rgba,
            },
        );
    }

    /// Drops a published image, and says whether there was one.
    pub fn forget_image(&mut self, name: &str) -> bool {
        self.images.forget_memory(name)
    }

    /// How many published images are held, and how many bytes they occupy.
    pub fn published_images(&self) -> (usize, usize) {
        self.images.memory_usage()
    }
}
