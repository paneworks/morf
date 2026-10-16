//! A rectangle's glass: whether it draws one beneath its fill, and the
//! backdrop command that draws it.

use morf_scene::{Frost, Value};

use super::*;

/// What a `Rect` or `ClipRect` draws beneath its fill: a backdrop when it
/// gives `backdrop_blur` a radius, bends what is behind, lights its rim or
/// frosts by what is behind; nothing otherwise.
pub(super) fn backdrop_command(
    scene: &Scene,
    node: NodeHandle,
    (bounds, transform, clip): (Geometry, Transform2D, Option<Geometry>),
    radii: [f64; 4],
) -> Result<Option<DrawCommand>, RenderError> {
    let blur = match scene.current(node, "backdrop_blur")? {
        Value::Number(radius) if radius.is_finite() && *radius > 0.0 => {
            radius.min(MAX_BACKDROP_BLUR)
        }
        _ => 0.0,
    };
    let finite = |value: f64| if value.is_finite() { value } else { 0.0 };
    let refraction = finite(scene.number(node, "glass_refraction")?);
    let rim = finite(scene.number(node, "glass_rim")?).clamp(0.0, 1.0);
    let frost = Frost::of(scene.current(node, "glass_frost")?);
    if blur <= 0.0 && refraction == 0.0 && rim <= 0.0 && frost != Frost::Auto {
        return Ok(None);
    }
    // The frosted level is blurred at the node's own radius, or at the
    // glass's whenever some frost can show without one.
    let radius = match frost {
        _ if blur > 0.0 => blur,
        Frost::Amount(amount) if amount <= 0.0 => 0.0,
        _ => GLASS_BLUR,
    };
    Ok(Some(DrawCommand::Backdrop {
        node,
        bounds,
        transform,
        clip,
        radii,
        radius,
        saturation: scene.number(node, "backdrop_saturation")?.max(0.0),
        tint: scene.color_value(node, "backdrop_tint")?,
        refraction,
        frost,
        rim,
    }))
}
