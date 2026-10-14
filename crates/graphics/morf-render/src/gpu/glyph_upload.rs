//! The glyph atlas's texture: making it, and writing glyphs into it.

use morf_text::{RasterContent, RasterGlyph};

use super::glyphs::{GLYPH_ATLAS_SIZE, GlyphAtlasEntry};

/// The texel format, bytes per texel and label an atlas of `content` uses.
pub(crate) fn atlas_format(content: RasterContent) -> (wgpu::TextureFormat, u32, &'static str) {
    match content {
        RasterContent::Mask | RasterContent::Field => {
            (wgpu::TextureFormat::R8Unorm, 1, "morf glyph mask atlas")
        }
        RasterContent::Color => (
            wgpu::TextureFormat::Rgba8UnormSrgb,
            4,
            "morf color glyph atlas",
        ),
    }
}

/// An atlas texture `pages` pages tall, and the group that binds it.
pub(crate) fn atlas_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    content: RasterContent,
    pages: u32,
) -> (wgpu::Texture, wgpu::BindGroup) {
    let (format, _, label) = atlas_format(content);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: GLYPH_ATLAS_SIZE,
            height: GLYPH_ATLAS_SIZE * pages,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (texture, bind_group)
}

pub(crate) fn glyph_pixels(glyph: &RasterGlyph) -> Vec<u8> {
    let pixel_count = glyph.width as usize * glyph.height as usize;
    match glyph.content {
        RasterContent::Mask if glyph.data.len() >= pixel_count * 3 => {
            let mut pixels = Vec::with_capacity(pixel_count);
            for index in 0..pixel_count {
                let at = index * 3;
                pixels.push(glyph.data[at..at + 3].iter().copied().max().unwrap_or(0));
            }
            pixels
        }
        // One byte a pixel either way: coverage for a mask, distance from the
        // glyph edge for a field.
        RasterContent::Mask | RasterContent::Field => glyph.data[..pixel_count].to_vec(),
        RasterContent::Color => glyph.data[..pixel_count * 4].to_vec(),
    }
}

pub(crate) fn upload_glyph(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    entry: &GlyphAtlasEntry,
    bytes_per_pixel: u32,
) {
    let padded_width = entry.width + 2;
    let padded_height = entry.height + 2;
    let bytes_per_pixel = bytes_per_pixel as usize;
    let mut padded =
        vec![entry.outside; padded_width as usize * padded_height as usize * bytes_per_pixel];
    for row in 0..entry.height as usize {
        let source = row * entry.width as usize * bytes_per_pixel;
        let destination = ((row + 1) * padded_width as usize + 1) * bytes_per_pixel;
        let row_bytes = entry.width as usize * bytes_per_pixel;
        padded[destination..destination + row_bytes]
            .copy_from_slice(&entry.pixels[source..source + row_bytes]);
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: entry.x - 1,
                y: entry.y - 1,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &padded,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(padded_width * bytes_per_pixel as u32),
            rows_per_image: Some(padded_height),
        },
        wgpu::Extent3d {
            width: padded_width,
            height: padded_height,
            depth_or_array_layers: 1,
        },
    );
}
