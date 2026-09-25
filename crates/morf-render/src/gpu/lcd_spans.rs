//! Which glyphs of a frame are drawn in subpixels (see `lcd.rs` for why each
//! rule is there).

use crate::{DamageRect, DrawCommand, DrawList};
use morf_layout::{Geometry, Transform2D};

use super::glyphs::{GlyphBatch, GlyphSpan};
use super::targets::quad_reach;

/// Whether a transform only moves what it is applied to.
pub(crate) fn only_moves(transform: Transform2D) -> bool {
    let [a, b, c, d, _, _] = transform.matrix;
    a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0
}

/// The pixels a command surely paints opaque, if any: a rectangle filled
/// with an opaque colour, without the corners its radii round off, the
/// border when that is translucent, or what its clip cuts away.
pub(crate) fn opaque_interior(command: &DrawCommand, scale_120: u32) -> Option<DamageRect> {
    let DrawCommand::Quad {
        bounds,
        transform,
        clip,
        color,
        gradient,
        radii,
        border_width,
        border_color,
        blur,
        shadow_color,
        shadow_inner,
        shader,
        ..
    } = command
    else {
        return None;
    };
    if color.alpha < 0.999
        || gradient.is_some()
        || shader.is_some()
        || *blur > 0.0
        || (*shadow_inner && shadow_color.alpha > 0.0)
        || !only_moves(*transform)
    {
        return None;
    }
    let [_, _, _, _, tx, ty] = transform.matrix;
    // The largest square in a rounded corner stops this far in: the corner's
    // arc crosses the diagonal at r(1 - 1/sqrt 2).
    let corner =
        radii.iter().copied().fold(0.0_f64, f64::max) * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
    let border = if border_color.alpha < 0.999 {
        border_width.max(0.0)
    } else {
        0.0
    };
    let inset = corner + border;
    let mut area = Geometry {
        x: bounds.x + tx + inset,
        y: bounds.y + ty + inset,
        width: bounds.width - 2.0 * inset,
        height: bounds.height - 2.0 * inset,
    };
    if let Some(clip) = clip {
        let left = area.x.max(clip.x);
        let top = area.y.max(clip.y);
        let right = (area.x + area.width).min(clip.x + clip.width);
        let bottom = (area.y + area.height).min(clip.y + clip.height);
        area = Geometry {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        };
    }
    // Whole pixels inside it, and a pixel less for the antialiased edge.
    let scale = f64::from(scale_120.max(1)) / 120.0;
    let left = (area.x * scale).ceil() + 1.0;
    let top = (area.y * scale).ceil() + 1.0;
    let right = ((area.x + area.width) * scale).floor() - 1.0;
    let bottom = ((area.y + area.height) * scale).floor() - 1.0;
    if right <= left || bottom <= top || left < 0.0 || top < 0.0 {
        return None;
    }
    Some(DamageRect {
        x: left as u32,
        y: top as u32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

fn contains(outer: DamageRect, inner: DamageRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// Splits a command's spans where the glyphs change between greyscale and
/// subpixel, keeping their order.
pub(crate) fn split_spans(spans: &mut Vec<GlyphSpan>, lcd: impl Fn(u32) -> bool) {
    let mut split = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        if span.color {
            split.push(span);
            continue;
        }
        let mut start = span.range.start;
        while start < span.range.end {
            let this = lcd(start);
            let mut end = start + 1;
            while end < span.range.end && lcd(end) == this {
                end += 1;
            }
            split.push(GlyphSpan {
                range: start..end,
                color: false,
                lcd: this,
            });
            start = end;
        }
    }
    *spans = split;
}

/// Marks the glyphs of this frame that are drawn in subpixels: plain ones
/// (`GlyphBatch::plain`), straight into the surface (`in_layer` says which
/// commands are not), each over an opaque rectangle drawn before it in the
/// surface -- or anywhere, on a surface declared opaque.
pub(crate) fn mark_subpixel_glyphs(
    batch: &mut GlyphBatch,
    list: &DrawList,
    in_layer: impl Fn(usize) -> bool,
    scale_120: u32,
    target: (u32, u32),
    opaque_surface: bool,
) {
    let mut ground: Vec<DamageRect> = Vec::new();
    for (index, command) in list.commands.iter().enumerate() {
        if in_layer(index) {
            continue;
        }
        // A command's glyphs are drawn after its own fill, but no command
        // has both.
        if matches!(command, DrawCommand::Text { .. }) {
            let GlyphBatch {
                instances,
                command_spans,
                plain,
            } = batch;
            split_spans(&mut command_spans[index], |instance| {
                let at = instance as usize;
                plain[at] && {
                    let reach = quad_reach(&instances[at], target);
                    opaque_surface || ground.iter().any(|rect| contains(*rect, reach))
                }
            });
        }
        if let Some(rect) = opaque_interior(command, scale_120) {
            ground.push(rect);
        }
    }
}
