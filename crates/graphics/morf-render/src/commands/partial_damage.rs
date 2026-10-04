//! Partial damage: which layers of a field, or rows of a terminal, changed
//! between two pictures of one command.

use super::*;

impl DrawCommand {
    /// What changed between two pictures of one field, layer by layer, when
    /// only some of its layers did. `None` when anything else about it
    /// changed, and the whole command is damaged as usual.
    ///
    /// A composition is pointwise: every layer's contribution is a function
    /// of the point, and a layer only changes the result where it is nearer
    /// than its seam — anywhere further, the operator is the plain one and the
    /// layer is not the answer. So a panel sliding in a fullscreen frame
    /// damages the panel's reach, before and after, and not the frame: the
    /// difference between repainting a few hundred thousand pixels and eight
    /// million on every frame of the slide.
    ///
    /// The reach is widened by twice the widest seam and the outline and
    /// softness, because a layer's colour is mixed through the seams of the
    /// layers after it; and repeated at the shadow's offset, grown by its blur
    /// and spread, because a shadow is the same composition sampled there.
    pub(crate) fn field_layers_changed(&self, old: &Self) -> Option<Vec<Geometry>> {
        let (
            Self::Field {
                transform,
                clip,
                stroke_width,
                softness,
                shadow_color,
                shadow_blur,
                shadow_spread,
                shadow_offset_x,
                shadow_offset_y,
                shader,
                layers,
                ..
            },
            Self::Field {
                layers: old_layers, ..
            },
        ) = (self, old)
        else {
            return None;
        };
        // A shader may read anything anywhere; and a different count shifts
        // every layer after the change into a different composition.
        if shader.is_some()
            || layers.len() != old_layers.len()
            || layers.len() > crate::field::MAX_FIELD_LAYERS
        {
            return None;
        }
        // Everything but the layers has to be the same for the difference to
        // be the layers alone.
        let mut same = old.clone();
        if let Self::Field {
            layers: same_layers,
            ..
        } = &mut same
        {
            same_layers.clone_from(layers);
        }
        if same != *self {
            return None;
        }
        let seam = layers
            .iter()
            .chain(old_layers)
            .map(|layer| f64::from(layer.blend))
            .fold(0.0, f64::max);
        // The layer's own seam is in its reach already. Its colour can still
        // show where a later seam mixes it in, which a quadratic seam allows up
        // to a quarter radius further than its own and the other's; half again
        // the widest seam covers both. Two pixels more for the derivative the
        // antialiased edge takes.
        let margin = stroke_width.max(0.0) + softness.max(0.0) + seam * 1.5 + 2.0;
        let shadow = shadow_color.alpha > 0.0;
        let mut areas = Vec::new();
        for (layer, old_layer) in layers.iter().zip(old_layers) {
            if layer == old_layer {
                continue;
            }
            for changed in [layer, old_layer] {
                let Some(reach) =
                    crate::field::field_reach(0.0, 0.0, std::slice::from_ref(changed))
                else {
                    continue;
                };
                let reach = crate::effects::expand_geometry(reach, margin);
                areas.push(transform.bounds(reach));
                if shadow {
                    let grown = shadow_blur.max(0.0) + shadow_spread.abs();
                    let moved = crate::effects::offset_geometry(
                        crate::effects::expand_geometry(reach, grown),
                        *shadow_offset_x,
                        *shadow_offset_y,
                    );
                    areas.push(transform.bounds(moved));
                }
            }
        }
        Some(
            areas
                .into_iter()
                .map(|area| clip.map_or(area, |clip| intersect_geometry(area, clip)))
                .collect(),
        )
    }

    /// What changed between two pictures of one terminal, row by row, when
    /// only what is on its screen did. `None` when anything else moved, and
    /// the whole command is damaged as usual.
    ///
    /// A shell printing a line changes two rows of fifty; repainting the
    /// terminal's whole rectangle for it would be the one cost a terminal has
    /// that the program did not ask for.
    pub(crate) fn terminal_rows_changed(&self, old: &Self) -> Option<Vec<Geometry>> {
        let (
            Self::Terminal {
                node,
                bounds,
                transform,
                clip,
                color_overlay,
                screen,
            },
            Self::Terminal {
                node: old_node,
                bounds: old_bounds,
                transform: old_transform,
                clip: old_clip,
                color_overlay: old_overlay,
                screen: old_screen,
            },
        ) = (self, old)
        else {
            return None;
        };
        let same_frame = node == old_node
            && bounds == old_bounds
            && transform == old_transform
            && clip == old_clip
            && color_overlay == old_overlay
            && screen.rows == old_screen.rows
            && screen.columns == old_screen.columns
            && screen.metrics == old_screen.metrics
            && screen.padding == old_screen.padding
            && screen.background == old_screen.background
            && screen.font_family == old_screen.font_family
            && screen.font_size == old_screen.font_size
            && screen.lines.len() == old_screen.lines.len();
        if !same_frame {
            return None;
        }
        let mut rows: Vec<usize> = (0..screen.lines.len())
            .filter(|&row| {
                !std::sync::Arc::ptr_eq(&screen.lines[row], &old_screen.lines[row])
                    && screen.lines[row] != old_screen.lines[row]
            })
            .collect();
        if screen.cursor != old_screen.cursor {
            rows.extend(screen.cursor.map(|cursor| cursor.row));
            rows.extend(old_screen.cursor.map(|cursor| cursor.row));
        }
        let cell = screen.metrics.cell_height;
        let top = bounds.y + screen.padding;
        Some(
            rows.into_iter()
                .map(|row| {
                    // A row and a pixel either side of it: the grid is
                    // snapped to device pixels, so a row may sit a fraction
                    // away from where its logical position says.
                    let area = transform.bounds(Geometry {
                        x: bounds.x,
                        y: top + row as f64 * cell - 1.0,
                        width: bounds.width,
                        height: cell + 2.0,
                    });
                    clip.map_or(area, |clip| intersect_geometry(area, clip))
                })
                .collect(),
        )
    }
}
