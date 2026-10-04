//! Drawn paths as textures: each `Path` command drawn once at the pixels it
//! covers, and kept while it does not change.

use crate::{DistanceFieldStyle, DrawCommand, DrawList};
use morf_layout::Geometry;
use std::collections::{HashMap, HashSet};

use super::upload::upload_texture;
use super::{
    TextureBatch, TextureBatchContext, TextureImage, TexturePlacement, TextureStyle,
    push_texture_instance,
};

/// How many drawn paths the GPU keeps. A path whose numbers are moving mints
/// a picture per frame; the bound lets those fall out while the still ones,
/// drawn this frame, stay.
pub(crate) const MAX_PATH_TEXTURES: usize = 256;

/// Draws every `Path` command that has no picture yet and places them all.
///
/// Each is drawn at the device pixels it covers — its box and stroke margin,
/// times the output scale, times the scale its transform adds on each axis —
/// so it is sharp at any size, and kept under a key of everything that shaped
/// its pixels, so one that is not changing is drawn once.
pub(crate) fn push_path_textures(
    context: TextureBatchContext<'_>,
    outlines: &mut crate::path::PathOutlines,
    textures: &mut HashMap<u64, TextureImage>,
    list: &DrawList,
    scale_120: u32,
    batch: &mut TextureBatch,
) {
    let scale = scale_120.max(1) as f64 / 120.0;
    let mut used = HashSet::new();
    for (command_index, command) in list.commands.iter().enumerate() {
        let DrawCommand::Path {
            bounds,
            transform,
            color_overlay,
            paint,
            ..
        } = command
        else {
            continue;
        };
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            continue;
        }
        let margin = paint.margin(bounds.width, bounds.height);
        let covered = Geometry {
            x: bounds.x - margin,
            y: bounds.y - margin,
            width: bounds.width + margin * 2.0,
            height: bounds.height + margin * 2.0,
        };
        let [a, b, c, d, _, _] = transform.matrix;
        // Capped, so a node scaled up without bound cannot ask for an image
        // larger than a texture may be.
        const LARGEST: f64 = 4096.0;
        let pixels = (
            (covered.width * scale * a.hypot(b))
                .ceil()
                .clamp(1.0, LARGEST) as u32,
            (covered.height * scale * c.hypot(d))
                .ceil()
                .clamp(1.0, LARGEST) as u32,
        );
        let key = paint.key(pixels, (bounds.width, bounds.height));
        used.insert(key);
        let image = match textures.get(&key) {
            Some(image) => image.clone(),
            None => {
                let Some(drawn) = crate::path::rasterize(
                    outlines,
                    paint,
                    (bounds.width, bounds.height),
                    margin,
                    pixels,
                ) else {
                    continue;
                };
                let image = upload_texture(&context, drawn.width, drawn.height, &drawn.rgba, true);
                textures.insert(key, image.clone());
                image
            }
        };
        push_texture_instance(
            batch,
            command_index,
            image,
            TexturePlacement {
                bounds: covered,
                transform: *transform,
                logical_width: pixels.0,
                logical_height: pixels.1,
                uv: [0.0, 0.0, 1.0, 1.0],
            },
            TextureStyle {
                overlay: *color_overlay,
                distance_field: false,
                field: DistanceFieldStyle::default(),
                spread: 1.0,
            },
            context.target_size,
            scale,
        );
    }
    if textures.len() > MAX_PATH_TEXTURES {
        textures.retain(|key, _| used.contains(key));
    }
}
