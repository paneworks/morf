//! Which glyphs of a frame are drawn in subpixels (see `lcd.rs` for why each
//! rule is there).

use crate::{DamageRect, DrawCommand, DrawList, Layer};
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
    let border = if border_color.alpha < 0.999 {
        border_width.max(0.0)
    } else {
        0.0
    };
    rounded_interior(*bounds, (tx, ty), radii, border, *clip, scale_120)
}

/// The whole pixels surely inside the rounded rectangle `bounds`, moved by
/// `offset` and `inset` further in, that `clip` keeps: past its rounded
/// corners and a pixel short of its antialiased edge.
pub(crate) fn rounded_interior(
    bounds: Geometry,
    (tx, ty): (f64, f64),
    radii: &[f64; 4],
    inset: f64,
    clip: Option<Geometry>,
    scale_120: u32,
) -> Option<DamageRect> {
    // The largest square in a rounded corner stops this far in: the corner's
    // arc crosses the diagonal at r(1 - 1/sqrt 2).
    let corner =
        radii.iter().copied().fold(0.0_f64, f64::max) * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
    let inset = corner + inset;
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

/// Per layer, where compositing it leaves the pixels of its target exactly
/// as they are -- so an opaque pixel drawn into it, fringes and all, reaches
/// the surface unchanged -- or `None` where it changes them everywhere.
///
/// That takes an opacity of one, no blur and no effect shader, on the layer
/// and on every layer it is composited into; a rounded mask, when there is
/// one, only moved, and then only what lies inside its corners counts.
/// `layers` lists a parent before its children, as a draw list does.
pub(crate) fn layers_keeping_pixels(
    layers: &[Layer],
    scale_120: u32,
    (width, height): (u32, u32),
) -> Vec<Option<DamageRect>> {
    let whole = DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut keeps: Vec<Option<DamageRect>> = Vec::with_capacity(layers.len());
    for layer in layers {
        let parent = match layer.parent {
            None => Some(whole),
            Some(parent) => keeps.get(parent).copied().flatten(),
        };
        // An alpha mask changes every pixel it is composited through, and a
        // mask's own pixels are read for their alpha alone.
        let own = if layer.opacity < 1.0
            || layer.blur > 0.0
            || layer.shader.is_some()
            || layer.alpha_mask.is_some()
            || layer.mask_for.is_some()
        {
            None
        } else {
            match &layer.mask {
                None => Some(whole),
                Some(mask) if only_moves(mask.transform) => {
                    let [_, _, _, _, tx, ty] = mask.transform.matrix;
                    rounded_interior(mask.bounds, (tx, ty), &mask.radii, 0.0, None, scale_120)
                }
                Some(_) => None,
            }
        };
        keeps.push(match (parent, own) {
            (Some(parent), Some(own)) => intersect(parent, own),
            _ => None,
        });
    }
    keeps
}

fn intersect(a: DamageRect, b: DamageRect) -> Option<DamageRect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    (right > left && bottom > top).then(|| DamageRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
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
/// (`GlyphBatch::plain`), each over an opaque rectangle drawn before it into
/// the same target. That target is the surface, or the innermost offscreen
/// layer holding the glyph (`layer_of`) where that layer's composite keeps
/// its pixels ([`layers_keeping_pixels`]). A surface declared opaque is
/// ground everywhere; a layer's target starts out transparent, so never.
pub(crate) fn mark_subpixel_glyphs(
    batch: &mut GlyphBatch,
    list: &DrawList,
    layer_of: impl Fn(usize) -> Option<usize>,
    scale_120: u32,
    target: (u32, u32),
    opaque_surface: bool,
) {
    let keeps = layers_keeping_pixels(&list.layers, scale_120, target);
    // Ground per target: the surface's, then each layer's own.
    let mut ground: Vec<Vec<DamageRect>> = vec![Vec::new(); list.layers.len() + 1];
    for (index, command) in list.commands.iter().enumerate() {
        let layer = layer_of(index);
        let slot = layer.map_or(0, |layer| layer + 1);
        // A command's glyphs are drawn after its own fill, but no command
        // has both.
        if matches!(command, DrawCommand::Text { .. }) {
            // Where the glyph must lie for its pixels to survive the
            // composite: anywhere on the surface itself.
            let kept = match layer {
                None => Some(None),
                Some(layer) => keeps[layer].map(Some),
            };
            let GlyphBatch {
                instances,
                command_spans,
                plain,
            } = batch;
            let beneath = &ground[slot];
            split_spans(&mut command_spans[index], |instance| {
                let at = instance as usize;
                plain[at]
                    && kept.is_some_and(|kept| {
                        let reach = quad_reach(&instances[at], target);
                        kept.is_none_or(|kept| contains(kept, reach))
                            && ((opaque_surface && layer.is_none())
                                || beneath.iter().any(|rect| contains(*rect, reach)))
                    })
            });
        }
        if let Some(rect) = opaque_interior(command, scale_120) {
            ground[slot].push(rect);
        }
    }
}
