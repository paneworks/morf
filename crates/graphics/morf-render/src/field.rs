use crate::gradient::{GradientMaterial, gradient_material};
use crate::{commands::*, effects::*};
use glyph_layer::polygon_params;
use morf_region::Shape;

/// How many layers one field may compose.
///
/// A composition is resolved in a single fragment shader, so every layer costs
/// every pixel of the node — the cap is what keeps a runaway configuration from
/// turning one node into an unbounded loop per fragment.
pub const MAX_FIELD_LAYERS: usize = 16;

/// One distance-field layer as the shader storage buffer holds it.
#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, Debug, Default, PartialEq)]
pub struct SdfFieldLayer {
    /// `[shape, morph_to, morph, operation]`.
    pub kinds: [f32; 4],
    /// Centre then half-extents, in the field's own space.
    pub rect: [f32; 4],
    /// `[outline start, points, inner radius, thickness]`.
    ///
    /// The first slot was padding — corners come through `radii` — and now
    /// carries where a polygon layer's outline points begin. It is the one slot
    /// in here nothing else wanted.
    pub params: [f32; 4],
    /// `[angle, rotation, blend, outline loop count]`.
    pub extra: [f32; 4],
    /// Linear-light fill for this layer.
    pub color: [f32; 4],
    /// Corner radii, top-left clockwise.
    pub radii: [f32; 4],
    /// The inverse of the layer's linear map, column major: what takes a
    /// point from the field (once turned by `rotation`) into the shape's own
    /// frame. The identity for an ordinary layer.
    pub frame: [f32; 4],
    /// `[blend group, distance scale, blend profile, fade]`. The scale is
    /// the map's smallest stretch, which keeps a distance measured in the
    /// shape's frame from overstating the true one. The fade is one minus
    /// the layer's opacity.
    pub meta: [f32; 4],
}

/// Two masks over a field's layers, bit `i` for layer `i`: the ones with no
/// opacity, which the composition leaves out, and the ones partly there,
/// which it mixes in. A whole layer is in neither.
pub fn opacity_masks(layers: &[SdfLayer]) -> (u32, u32) {
    let mut absent = 0;
    let mut fading = 0;
    for (index, layer) in layers.iter().take(MAX_FIELD_LAYERS).enumerate() {
        if layer.opacity <= 0.0 {
            absent |= 1 << index;
        } else if layer.opacity < 1.0 {
            fading |= 1 << index;
        }
    }
    (absent, fading)
}

/// The inverse of a layer's linear map and how much it shrinks a distance.
///
/// A distance measured in the shape's own frame is multiplied by the map's
/// smallest singular value, which never overstates the distance on the
/// surface: the edge stays exactly where it is, and the soft ramp either
/// side of it stays a pixel wide rather than widening where the map
/// stretches. A map that flattens the plane has no inverse; the layer is left
/// as it was rather than drawn through infinities.
pub fn layer_frame(matrix: [f32; 4]) -> ([f32; 4], f32) {
    let [a, b, c, d] = matrix.map(f64::from);
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-9 || matrix == [1.0, 0.0, 0.0, 1.0] {
        return ([1.0, 0.0, 0.0, 1.0], 1.0);
    }
    let trace = a * a + b * b + c * c + d * d;
    let smallest = ((trace
        - (trace * trace - 4.0 * determinant * determinant)
            .max(0.0)
            .sqrt())
        / 2.0)
        .max(0.0)
        .sqrt();
    (
        [
            (d / determinant) as f32,
            (-b / determinant) as f32,
            (-c / determinant) as f32,
            (a / determinant) as f32,
        ],
        smallest as f32,
    )
}

/// How an outline sits against the shape's edge.
///
/// One outline serves both the inset border a rectangle has always drawn and
/// the centred stroke a field has always drawn — they were the same band of
/// pixels described by two shaders.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BorderAlignment {
    /// Entirely inside the shape, which is what a rectangle border means.
    #[default]
    Inside,
    /// Straddling the crossing, so widening it does not move the edge.
    Centred,
    /// Entirely outside the shape.
    Outside,
}

impl BorderAlignment {
    pub(crate) fn code(self) -> f32 {
        self as u32 as f32
    }
}

/// Everything about a field's surface that is not its shape.
///
/// One per instance, read by instance index. It is a storage buffer rather
/// than more vertex attributes because the quad pipeline this pass absorbed
/// already used sixteen of them, and sixteen is the limit.
#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, Debug, Default, PartialEq)]
pub struct SdfFieldMaterial {
    /// `[alignment, antialiased, unused, unused]`. The width itself rides in
    /// the instance's `style.x`, where the host already needs it to size the
    /// quad the field is drawn into.
    pub border: [f32; 4],
    pub border_color: [f32; 4],
    /// `[offset x, offset y, inner, unused]`.
    pub shadow: [f32; 4],
    pub shadow_color: [f32; 4],
    /// `[absent layers, shadow blur, shadow spread, fading layers]`: the
    /// two layer masks of [`opacity_masks`], bit `i` for the field's layer
    /// `i`, carried as floats (exact, sixteen bits).
    pub effects: [f32; 4],
    /// `[kind, centre x, centre y, radius]`; kind is 0 none, 1 linear, 2
    /// radial, 3 conic, and the centre and radius are fractions of the shape.
    pub gradient: [f32; 4],
    /// `[angle in radians, stop count, space, unused]`.
    pub gradient_extra: [f32; 4],
    /// Stop positions, four to a vector.
    pub gradient_positions: [[f32; 4]; 4],
    /// Linear-light stop colours, straight alpha.
    pub gradient_colors: [[f32; 4]; morf_scene::MAX_GRADIENT_STOPS],
    pub color_overlay: [f32; 4],
    /// The rectangle a gradient is measured across, in the field's own space.
    pub shape: [f32; 4],
}

impl SdfFieldMaterial {
    /// The material a plain composed field wants: an antialiased outline, no
    /// gradient, no shadow.
    pub fn plain(alignment: BorderAlignment, shape: [f32; 4]) -> Self {
        Self {
            border: [alignment.code(), 1.0, 0.0, 0.0],
            shape,
            ..Self::default()
        }
    }
}

/// One composed field as the shader instance buffer holds it.
#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, Debug, Default, PartialEq)]
pub struct SdfFieldInstance {
    /// Physical bounds: origin then size.
    pub bounds: [f32; 4],
    /// Fill colour.
    pub fill: [f32; 4],
    /// Outline colour.
    pub outline: [f32; 4],
    /// `[stroke width, softness, first layer, layer count]`.
    pub style: [f32; 4],
    /// Affine matrix, column major.
    pub transform: [f32; 4],
    /// Affine translation in `xy`; `z` is which material is this field's.
    pub transform_offset: [f32; 4],
    /// Everything the surface can reach, in the node's own space: left, top,
    /// right, bottom.
    pub area: [f32; 4],
}

impl SdfFieldInstance {
    /// Converts a field or quad command to physical instance, layer and
    /// material data.
    ///
    /// Both go through this pass. A rectangle is a field of one `Box` layer —
    /// it always was, once the shape it drew came from the same distance
    /// function — and giving it its own pipeline only meant that a gradient, a
    /// border and a shadow were things a rectangle could have and a composed
    /// shape could not.
    pub fn from_command(
        command: &DrawCommand,
        scale_120: u32,
        layers: &mut Vec<SdfFieldLayer>,
        materials: &mut Vec<SdfFieldMaterial>,
        outlines: &mut Vec<[f32; 2]>,
        text: &mut morf_text::TextSystem,
        drawings: &mut morf_svg::SvgOutlines,
    ) -> Option<Self> {
        match command {
            DrawCommand::Field { .. } => Self::from_field(
                command, scale_120, layers, materials, outlines, text, drawings,
            ),
            DrawCommand::Quad { .. } => Self::from_quad(command, scale_120, layers, materials),
            _ => None,
        }
    }

    /// The layers are written in the field's own space — origin at the node's
    /// top-left corner — because that is the space the fragment shader walks,
    /// and it keeps a layer's numbers independent of where the node sits.
    fn from_field(
        command: &DrawCommand,
        scale_120: u32,
        layers: &mut Vec<SdfFieldLayer>,
        materials: &mut Vec<SdfFieldMaterial>,
        outlines: &mut Vec<[f32; 2]>,
        text: &mut morf_text::TextSystem,
        drawings: &mut morf_svg::SvgOutlines,
    ) -> Option<Self> {
        let DrawCommand::Field {
            bounds,
            transform,
            fill_color,
            stroke_color,
            stroke_width,
            stroke_alignment,
            softness,
            gradient,
            color_overlay,
            shadow_color,
            shadow_blur,
            shadow_spread,
            shadow_offset_x,
            shadow_offset_y,
            shadow_inner,
            shader,
            layers: sources,
            ..
        } = command
        else {
            return None;
        };
        if sources.is_empty() {
            return None;
        }
        let scale = scale_120.max(1) as f64 / 120.0;
        let first = layers.len();
        // Which layers are not there at all, and which are partly: the
        // shader leaves out the first and mixes the field with and without
        // each of the second.
        let (absent, fading) = opacity_masks(sources);
        for layer in sources.iter().take(MAX_FIELD_LAYERS) {
            let outline = polygon_params(layer, scale, outlines, text, drawings);
            let (frame, distance_scale) = layer_frame(layer.matrix);
            layers.push(SdfFieldLayer {
                frame,
                meta: [
                    layer.blend_group as f32,
                    distance_scale,
                    layer.profile.code() as f32,
                    // Stored as how far it has faded, so a zeroed layer is a
                    // whole one.
                    1.0 - layer.opacity.clamp(0.0, 1.0),
                ],
                kinds: [
                    layer.shape.code() as f32,
                    layer.morph_to.code() as f32,
                    layer.morph.clamp(0.0, 1.0),
                    layer.operation.code() as f32,
                ],
                rect: [
                    ((layer.bounds.x - bounds.x + layer.bounds.width / 2.0) * scale) as f32,
                    ((layer.bounds.y - bounds.y + layer.bounds.height / 2.0) * scale) as f32,
                    ((layer.bounds.width / 2.0) * scale) as f32,
                    ((layer.bounds.height / 2.0) * scale) as f32,
                ],
                params: outline.0,
                extra: [
                    layer.angle,
                    layer.rotation,
                    (f64::from(layer.blend) * scale) as f32,
                    outline.1,
                ],
                color: color_array(layer.color),
                radii: layer.radii.map(|radius| (f64::from(radius) * scale) as f32),
            });
        }
        let GradientMaterial {
            gradient,
            gradient_extra,
            gradient_positions,
            gradient_colors,
        } = gradient_material(gradient.as_ref());
        // A field's outline straddles the crossing by default and a rectangle's
        // sits inside it, but both are the one outline the shader now has, and
        // either can say which it wants.
        materials.push(SdfFieldMaterial {
            border: [stroke_alignment.code(), 1.0, 0.0, 0.0],
            border_color: color_array(*stroke_color),
            shadow: [
                (shadow_offset_x * scale) as f32,
                (shadow_offset_y * scale) as f32,
                if *shadow_inner { 1.0 } else { 0.0 },
                0.0,
            ],
            shadow_color: color_array(*shadow_color),
            effects: [
                absent as f32,
                (shadow_blur * scale) as f32,
                (shadow_spread * scale) as f32,
                fading as f32,
            ],
            gradient,
            gradient_extra,
            gradient_positions,
            gradient_colors,
            color_overlay: color_array(*color_overlay),
            shape: [
                0.0,
                0.0,
                (bounds.width * scale) as f32,
                (bounds.height * scale) as f32,
            ],
        });
        Some(Self {
            bounds: [
                (bounds.x * scale) as f32,
                (bounds.y * scale) as f32,
                (bounds.width * scale) as f32,
                (bounds.height * scale) as f32,
            ],
            fill: color_array(*fill_color),
            outline: color_array(*stroke_color),
            style: [
                (stroke_width * scale) as f32,
                (softness * scale) as f32,
                first as f32,
                (layers.len() - first) as f32,
            ],
            transform: [
                transform.matrix[0] as f32,
                transform.matrix[1] as f32,
                transform.matrix[2] as f32,
                transform.matrix[3] as f32,
            ],
            transform_offset: [
                (transform.matrix[4] * scale) as f32,
                (transform.matrix[5] * scale) as f32,
                // Which material is this field's: its own instance index once, and
                // no longer since a field may be drawn as several tiles.
                (materials.len() - 1) as f32,
                0.0,
            ],
            // A shader that owns its coverage paints across the whole node,
            // so it is handed the whole node: the layers only say where the
            // shape it replaced would have reached.
            area: if shader.as_ref().is_some_and(|shader| shader.owns_coverage) {
                [
                    0.0,
                    0.0,
                    (bounds.width * scale) as f32,
                    (bounds.height * scale) as f32,
                ]
            } else {
                field_area(
                    *bounds,
                    *stroke_width,
                    *softness,
                    sources,
                    scale,
                    // An inner shadow falls inside the surface, so it needs no room.
                    (shadow_color.alpha > 0.0 && !*shadow_inner).then_some(ShadowReach {
                        offset_x: *shadow_offset_x,
                        offset_y: *shadow_offset_y,
                        blur: *shadow_blur,
                        spread: *shadow_spread,
                    }),
                )
            },
        })
    }

    /// A rectangle, as one `Box` layer of a field.
    ///
    /// Everything a quad could say that a field could not — the gradient, the
    /// inset border, the two shadow modes, the colour overlay — now says it
    /// through the material, which every field has.
    fn from_quad(
        command: &DrawCommand,
        scale_120: u32,
        layers: &mut Vec<SdfFieldLayer>,
        materials: &mut Vec<SdfFieldMaterial>,
    ) -> Option<Self> {
        let DrawCommand::Quad {
            bounds,
            transform,
            color,
            color_overlay,
            gradient,
            radii,
            border_width,
            antialiasing,
            border_pixel_aligned,
            border_color,
            blur,
            shadow_color,
            shadow_blur,
            shadow_spread,
            shadow_offset_x,
            shadow_offset_y,
            shadow_inner,
            shader,
            ..
        } = command
        else {
            return None;
        };
        let scale = scale_120.max(1) as f64 / 120.0;
        let width = (bounds.width * scale) as f32;
        let height = (bounds.height * scale) as f32;
        let first = layers.len();
        layers.push(SdfFieldLayer {
            kinds: [Shape::Box.code() as f32, Shape::Box.code() as f32, 0.0, 0.0],
            rect: [width / 2.0, height / 2.0, width / 2.0, height / 2.0],
            params: [0.0; 4],
            extra: [0.0; 4],
            color: color_array(*color),
            radii: radii.map(|radius| (radius.max(0.0) * scale) as f32),
            frame: [1.0, 0.0, 0.0, 1.0],
            meta: [0.0, 1.0, 0.0, 0.0],
        });
        let GradientMaterial {
            gradient,
            gradient_extra,
            gradient_positions,
            gradient_colors,
        } = gradient_material(gradient.as_ref());
        materials.push(SdfFieldMaterial {
            border: [
                BorderAlignment::Inside.code(),
                if *antialiasing { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ],
            border_color: color_array(*border_color),
            shadow: [
                (*shadow_offset_x * scale) as f32,
                (*shadow_offset_y * scale) as f32,
                if *shadow_inner { 1.0 } else { 0.0 },
                0.0,
            ],
            shadow_color: color_array(*shadow_color),
            effects: [
                0.0,
                (*shadow_blur * scale) as f32,
                (*shadow_spread * scale) as f32,
                0.0,
            ],
            gradient,
            gradient_extra,
            gradient_positions,
            gradient_colors,
            color_overlay: color_array(*color_overlay),
            shape: [0.0, 0.0, width, height],
        });
        // The quad the fragment shader walks has to reach everything the
        // effects do: the blurred edge, and an outer shadow's offset, blur and
        // spread. `effect_bounds` already knows that arithmetic; this only
        // restates its answer in the node's own frame, which is the frame a
        // field's `area` is expressed in.
        let expanded = effect_bounds(
            *bounds,
            *blur,
            if *shadow_inner { 0.0 } else { *shadow_blur },
            if *shadow_inner { 0.0 } else { *shadow_spread },
            if *shadow_inner { 0.0 } else { *shadow_offset_x },
            if *shadow_inner { 0.0 } else { *shadow_offset_y },
        );
        Some(Self {
            bounds: [
                (bounds.x * scale) as f32,
                (bounds.y * scale) as f32,
                width,
                height,
            ],
            fill: color_array(*color),
            outline: color_array(*border_color),
            style: [
                if *border_pixel_aligned {
                    (*border_width * scale).round() as f32
                } else {
                    (*border_width * scale) as f32
                },
                (*blur * scale) as f32,
                first as f32,
                1.0,
            ],
            transform: [
                transform.matrix[0] as f32,
                transform.matrix[1] as f32,
                transform.matrix[2] as f32,
                transform.matrix[3] as f32,
            ],
            transform_offset: [
                (transform.matrix[4] * scale) as f32,
                (transform.matrix[5] * scale) as f32,
                (materials.len() - 1) as f32,
                0.0,
            ],
            // A surface shader on a rectangle owns the whole node, exactly as
            // it does on a field: it is deciding coverage, so the effect
            // expansion is not what bounds it.
            area: if shader.as_ref().is_some_and(|shader| shader.owns_coverage) {
                [0.0, 0.0, width, height]
            } else {
                [
                    ((expanded.x - bounds.x) * scale) as f32,
                    ((expanded.y - bounds.y) * scale) as f32,
                    ((expanded.x + expanded.width - bounds.x) * scale) as f32,
                    ((expanded.y + expanded.height - bounds.y) * scale) as f32,
                ]
            },
        })
    }
}

mod cull;
pub(crate) mod glyph_layer;
mod reach;

pub use cull::{FIELD_TILE, FieldTile, composable, composed_distance};
pub(crate) use cull::{Spill, field_tiles};
pub use reach::*;
