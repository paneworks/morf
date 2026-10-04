//! A headless surface drawn by the real renderer, into pixels.
//!
//! The same `RenderEngine` and `WgpuBackend` the shell paints with, on an
//! offscreen target instead of a Wayland surface. Every CPU gate can pass
//! while a shader is visibly wrong; this is how to look.

use std::path::Path;

use morf_layout::Size;
use morf_render::{BlendSpace, RenderEngine, WgpuBackend};

use crate::headless::Headless;

/// Layout passes a picture allows the bindings that read the layout.
const SETTLE_PASSES: usize = 8;

/// A picture: straight-alpha RGBA rows.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Picture {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        image::RgbaImage::from_raw(self.width, self.height, self.pixels.clone())
            .ok_or_else(|| "the picture is not the size it says".to_owned())?
            .save(path)
            .map_err(|error| format!("could not write {}: {error}", path.display()))
    }
}

/// Opens a renderer on the first adapter there is, or says plainly why not.
fn renderer(width: u32, height: u32) -> Result<RenderEngine<WgpuBackend>, String> {
    let backend = pollster::block_on(WgpuBackend::new(width, height)).map_err(|error| {
        format!(
            "no GPU to render with ({error}). morf renders through Vulkan: run it where a \
             Vulkan driver is visible -- under the nixVulkan or nixVulkanIntel wrapper from a \
             Nix shell, or on a machine with the driver installed"
        )
    })?;
    Ok(RenderEngine::new(backend))
}

/// Draws one surface at `scale`, premultiplied as the renderer leaves it.
fn draw(headless: &mut Headless, index: usize, scale: u32) -> Result<Picture, String> {
    let (root, size, blend, label) = {
        let surface = headless
            .surfaces
            .get(index)
            .ok_or_else(|| format!("there is no surface {index}"))?;
        (
            surface.root,
            surface.size,
            surface.blend.clone(),
            surface.label(),
        )
    };
    let (width, height) = (size.0 * scale, size.1 * scale);
    let mut engine = renderer(width, height)?;
    engine
        .backend_mut()
        .set_blend(BlendSpace::parse(&blend).unwrap_or_default());
    crate::surface_run::register_shaders(&headless.runtime, &mut engine)?;
    // Settled against the renderer's own faces, since that is what shapes
    // what is drawn.
    let available = Size {
        width: f64::from(size.0),
        height: f64::from(size.1),
    };
    let layout = headless
        .runtime
        .settle_layout(root, available, engine.backend_mut(), SETTLE_PASSES)
        .map_err(|error| format!("{label}: layout: {error}"))?
        .layout;
    headless
        .runtime
        .sync_text_inputs(&layout, engine.backend_mut().text_system());
    headless.runtime.observe_stretch(&layout);
    // Twice: the second frame is the incremental one, which reuses an effect
    // layer's target the way every frame after the first does.
    for _ in 0..2 {
        engine
            .render(&headless.runtime.scene(), &layout, scale * 120, |_| {})
            .map_err(|error| format!("{label}: render: {error}"))?;
    }
    Ok(Picture {
        width,
        height,
        pixels: engine.backend_mut().read_pixels(),
    })
}

/// Undoes premultiplied alpha, which is what a PNG does not hold.
fn straighten(mut picture: Picture) -> Picture {
    for pixel in picture.pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    picture
}

/// One surface, as a PNG would hold it.
pub fn render_surface(
    headless: &mut Headless,
    index: usize,
    scale: u32,
) -> Result<Picture, String> {
    draw(headless, index, scale).map(straighten)
}

/// The whole screen: the primary surface and every visible layer surface,
/// each where its anchors and margins put it, laid over one another the way
/// a compositor would.
pub fn render_screen(headless: &mut Headless, scale: u32) -> Result<Picture, String> {
    let (width, height) = (headless.screen.0 * scale, headless.screen.1 * scale);
    let mut canvas = vec![0u8; (width * height * 4) as usize];
    let mut order = (0..headless.surfaces.len())
        .filter(|index| {
            let surface = &headless.surfaces[*index];
            surface.visible && matches!(surface.kind, "primary" | "layer")
        })
        .collect::<Vec<_>>();
    // Bottom layer first, as a compositor stacks them: a background layer
    // declared after the shell's own surface is still under it. Within a
    // layer, the order they were declared in.
    order.sort_by_key(|index| headless.surfaces[*index].stack);
    for index in order {
        let (x, y) = headless.surfaces[index].position;
        let picture = draw(headless, index, scale)?;
        let (x, y) = (x.max(0) as u32 * scale, y.max(0) as u32 * scale);
        for row in 0..picture.height {
            let target_row = y + row;
            if target_row >= height {
                break;
            }
            for column in 0..picture.width {
                let target_column = x + column;
                if target_column >= width {
                    break;
                }
                let from = ((row * picture.width + column) * 4) as usize;
                let to = ((target_row * width + target_column) * 4) as usize;
                let alpha = u32::from(picture.pixels[from + 3]);
                for channel in 0..4 {
                    let under = u32::from(canvas[to + channel]);
                    canvas[to + channel] = (u32::from(picture.pixels[from + channel])
                        + (under * (255 - alpha) + 127) / 255)
                        .min(255) as u8;
                }
            }
        }
    }
    Ok(straighten(Picture {
        width,
        height,
        pixels: canvas,
    }))
}

/// Renders what `wanted` names -- a surface, or `screen` for all of them
/// composed -- and writes it.
pub fn render_to(
    headless: &mut Headless,
    wanted: Option<&str>,
    scale: u32,
    path: &Path,
) -> Result<(u32, u32, String), String> {
    let (picture, what) = if wanted == Some("screen") {
        (render_screen(headless, scale)?, "screen".to_owned())
    } else {
        let index = headless.surface_index(wanted)?;
        let label = headless.surfaces[index].label();
        (render_surface(headless, index, scale)?, label)
    };
    picture.save(path)?;
    Ok((picture.width, picture.height, what))
}
