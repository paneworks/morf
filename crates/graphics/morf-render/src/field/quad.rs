//! A rectangle, as one `Box` layer of a field.

use super::*;

impl SdfFieldInstance {
    /// A rectangle, as one `Box` layer of a field.
    ///
    /// Everything a quad could say that a field could not — the gradient, the
    /// inset border, the two shadow modes, the colour overlay — now says it
    /// through the material, which every field has.
    pub(super) fn from_quad(
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
