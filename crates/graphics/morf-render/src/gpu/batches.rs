use morf_text::TextSystem;
use morf_vector::svg::SvgOutlines;

use crate::{DrawList, SdfFieldInstance, SdfFieldLayer, SdfFieldMaterial, ShaderBinding};

/// Everything one frame's fields need, gathered in one walk of the list.
pub(crate) struct FieldBatch {
    /// The instances each draw command is drawn as — one, or a large field's
    /// tiles — or `None` if the command is not a field.
    pub(crate) indices: Vec<Option<std::ops::Range<u32>>>,
    pub(crate) instances: Vec<SdfFieldInstance>,
    pub(crate) layers: Vec<SdfFieldLayer>,
    pub(crate) materials: Vec<SdfFieldMaterial>,
    /// Outline points for every polygon layer in the frame, end to end. A
    /// layer records where its own run begins and how long it is.
    pub(crate) outlines: Vec<[f32; 2]>,
    /// Parallel to `instances`: which pipeline draws each, which is not
    /// instance data because it selects the pipeline rather than riding in it.
    pub(crate) shaders: Vec<Option<ShaderBinding>>,
}

/// Groups every field and quad command, and the layers and materials they
/// carry, into one set of buffers.
///
/// A rectangle is a field of one layer, so there is one collector rather than
/// two: each instance records where its own run of layers begins, and its
/// material is found by its own instance index.
pub(crate) fn collect_field_instances(
    list: &DrawList,
    scale_120: u32,
    text: &mut TextSystem,
    drawings: &mut SvgOutlines,
) -> FieldBatch {
    let mut indices = vec![None; list.commands.len()];
    let mut instances = Vec::new();
    let mut layers = Vec::new();
    let mut materials = Vec::new();
    let mut outlines = Vec::new();
    // Parallel to `instances`, because which pipeline draws an instance is not
    // instance data: it selects the pipeline itself.
    let mut shaders = Vec::new();
    for (command_index, command) in list.commands.iter().enumerate() {
        // Shading a rectangle that shows nothing would cost its whole area.
        if command.draws_nothing() {
            continue;
        }
        if let Some(instance) = SdfFieldInstance::from_command(
            command,
            scale_120,
            &mut layers,
            &mut materials,
            &mut outlines,
            text,
            drawings,
        ) {
            // Both a field and a rectangle can carry one: a rectangle is a
            // field of one layer, and that is the shape most configurations
            // reach for first.
            let shader = match command {
                crate::DrawCommand::Field { shader, .. }
                | crate::DrawCommand::Quad { shader, .. } => shader.clone(),
                _ => None,
            };
            // A large field is drawn as the tiles its surface can reach; the
            // rest of its quad would only have decided pixel by pixel that it
            // is empty. A shader may paint anywhere, so a shaded one is whole.
            let tiles = match command {
                crate::DrawCommand::Field {
                    bounds,
                    layers: sources,
                    stroke_width,
                    softness,
                    shadow_color,
                    shadow_inner,
                    shadow_blur,
                    shadow_spread,
                    shadow_offset_x,
                    shadow_offset_y,
                    shader: None,
                    ..
                } => crate::field_tiles(
                    sources,
                    *bounds,
                    instance.area,
                    scale_120.max(1) as f64 / 120.0,
                    crate::Spill {
                        edge: stroke_width.max(0.0) + softness.max(0.0) + 2.0,
                        shadow: (shadow_color.alpha > 0.0 && !*shadow_inner).then_some(
                            crate::ShadowReach {
                                offset_x: *shadow_offset_x,
                                offset_y: *shadow_offset_y,
                                blur: *shadow_blur,
                                spread: *shadow_spread,
                            },
                        ),
                        // An inner shadow darkens the inside, and layers of
                        // several colours mix across it: then every pixel
                        // has to walk them.
                        solid: !*shadow_inner
                            && sources.iter().all(|layer| layer.color == sources[0].color),
                    },
                ),
                _ => None,
            };
            let start = instances.len() as u32;
            match tiles {
                Some(tiles) => {
                    for tile in tiles {
                        let mut drawn = SdfFieldInstance {
                            area: tile.area,
                            ..instance
                        };
                        // The shader's cue to fill the tile without walking
                        // the layers.
                        drawn.transform_offset[3] = if tile.solid { 1.0 } else { 0.0 };
                        instances.push(drawn);
                        shaders.push(shader.clone());
                    }
                }
                None => {
                    instances.push(instance);
                    shaders.push(shader);
                }
            }
            indices[command_index] = Some(start..instances.len() as u32);
        }
    }
    FieldBatch {
        indices,
        instances,
        layers,
        materials,
        outlines,
        shaders,
    }
}
