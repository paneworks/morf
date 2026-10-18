//! The frame on the target, kept as a picture: what a surface showed just
//! before it turned or was rebuilt, for whatever follows to start from.

use super::backend_types::{ExternalTexture, WgpuBackend};

impl WgpuBackend {
    /// The target's size in pixels: the size the last frame was drawn at.
    pub fn target_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Copies the frame the target holds into a texture published under
    /// `name` (`gpu:<name>` to a `ui.Image`), replacing what the name held,
    /// and returns its size in pixels. Nothing while nothing was drawn.
    ///
    /// The copy keeps the target's format and is read as a layer's target
    /// is: premultiplied, in the space this renderer blends in.
    pub fn snapshot(&mut self, name: &str) -> Option<(u32, u32)> {
        let (width, height) = (self.texture.width(), self.texture.height());
        if self.width == 0 || self.height == 0 || self.device_lost() {
            return None;
        }
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("morf snapshot"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture.format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("morf snapshot copy"),
            });
        encoder.copy_texture_to_texture(
            self.texture.as_image_copy(),
            texture.as_image_copy(),
            extent,
        );
        super::present::submit(&self.queue, Some(encoder.finish()), || {});
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("morf snapshot bind group"),
            layout: &self.glyph_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                },
            ],
        });
        self.external_textures.insert(
            name.to_owned(),
            ExternalTexture {
                texture,
                size: (width, height),
                bind_group,
                drawn: true,
                _export: None,
            },
        );
        Some((width, height))
    }
}
