//! What a draw command covers: the node it was painted for, its bounds and
//! its clip, and whether it draws anything at all.

use super::*;

impl DrawCommand {
    /// The scene node this command was painted for.
    ///
    /// Public because telling a layer's own drawing apart from its subtree's is
    /// how a tool outside this crate can see that an effect shader wraps
    /// nothing — a mistake that otherwise renders as silence.
    pub fn node(&self) -> NodeHandle {
        match self {
            Self::Quad { node, .. }
            | Self::Text { node, .. }
            | Self::Texture { node, .. }
            | Self::Path { node, .. }
            | Self::Field { node, .. }
            | Self::Backdrop { node, .. }
            | Self::Terminal { node, .. } => *node,
        }
    }

    /// Whether the command leaves every pixel as it was: a rectangle with no
    /// fill, overlay, gradient, border, shadow or shader that shows -- a
    /// transparent catcher over the screen, a dimmer at rest. It stays in the
    /// list (a layer's reach may be measured by it) but is neither shaded
    /// nor damage.
    pub(crate) fn draws_nothing(&self) -> bool {
        let Self::Quad {
            color,
            color_overlay,
            gradient,
            border_width,
            border_color,
            shadow_color,
            shader,
            ..
        } = self
        else {
            return false;
        };
        color.alpha <= 0.0
            && color_overlay.alpha <= 0.0
            && gradient.is_none()
            && (*border_width <= 0.0 || border_color.alpha <= 0.0)
            && shadow_color.alpha <= 0.0
            && shader.is_none()
    }

    pub(crate) fn bounds(&self) -> Geometry {
        let bounds = match self {
            Self::Quad {
                bounds,
                transform,
                blur,
                shadow_blur,
                shadow_spread,
                shadow_offset_x,
                shadow_offset_y,
                shadow_inner,
                ..
            } => transform.bounds(effect_bounds(
                *bounds,
                *blur,
                if *shadow_inner { 0.0 } else { *shadow_blur },
                if *shadow_inner { 0.0 } else { *shadow_spread },
                if *shadow_inner { 0.0 } else { *shadow_offset_x },
                if *shadow_inner { 0.0 } else { *shadow_offset_y },
            )),
            Self::Text {
                bounds, transform, ..
            }
            | Self::Texture {
                bounds, transform, ..
            }
            | Self::Backdrop {
                bounds, transform, ..
            }
            | Self::Terminal {
                bounds, transform, ..
            } => transform.bounds(*bounds),
            // A stroke reaches past the box by half its width and more at a
            // mitred corner; the drawing's own margin says how far.
            Self::Path {
                bounds,
                transform,
                paint,
                ..
            } => {
                let margin = paint.margin(bounds.width, bounds.height);
                transform.bounds(Geometry {
                    x: bounds.x - margin,
                    y: bounds.y - margin,
                    width: bounds.width + margin * 2.0,
                    height: bounds.height + margin * 2.0,
                })
            }
            Self::Field {
                transform,
                stroke_width,
                softness,
                layers: sources,
                ..
            } => {
                // One computation, shared with the quad the shader is given.
                // Written out separately here, the two drifted: this copy took
                // the layer rectangles unrotated, so a rotated shape was drawn
                // whole and damaged as though it were not.
                let Some(reach) = field_reach(*stroke_width, *softness, sources) else {
                    return Geometry::default();
                };
                transform.bounds(reach)
            }
        };
        self.clip()
            .map_or(bounds, |clip| intersect_geometry(bounds, clip))
    }

    pub(crate) fn clip(&self) -> Option<Geometry> {
        match self {
            Self::Quad { clip, .. }
            | Self::Text { clip, .. }
            | Self::Texture { clip, .. }
            | Self::Path { clip, .. }
            | Self::Field { clip, .. }
            | Self::Backdrop { clip, .. }
            | Self::Terminal { clip, .. } => *clip,
        }
    }
}
