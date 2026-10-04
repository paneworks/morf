//! The properties of the shape elements: `ui.Sdf`, `ui.SdfShape` and `ui.Path`.

use super::*;

/// What `ui.Sdf` adds to every element's properties.
pub(super) fn sdf() -> Vec<PropertySpec> {
    vec![
        color("fill_color", Color::rgba8(255, 255, 255, 255)),
        color("stroke_color", Color::rgba8(0, 0, 0, 0)),
        number("stroke_width", 0.0),
        // Extra edge softness in logical pixels, on top of the
        // derivative-based antialiasing the shader always applies. A
        // field is resolution independent, so this is the one knob that
        // turns a crisp edge into a glow.
        number("softness", 0.0),
        // The seam radius every absorbed layer uses unless it names its
        // own. A field with a blend fuses what it contains; a field
        // without one composes the same shapes with hard edges.
        number("blend", 0.0),
        // One position along the morph for the whole composition. A
        // compound shape — a disc with a ring and a notch, say — is
        // several layers that have to move together, and keeping that
        // many numbers in step by hand is how a configuration acquires
        // a frame runtime. Driving them from here makes the compound
        // one animatable property.
        number("morph_progress", 0.0),
        // Everything below belonged to a rectangle alone, because a
        // rectangle had its own pipeline and a composed shape did not.
        // One pipeline draws both now, so a star can carry a gradient
        // and a shadow like anything else.
        any("gradient", Value::Map(BTreeMap::new())),
        color("shadow_color", Color::rgba8(0, 0, 0, 0)),
        number("shadow_blur", 0.0),
        number("shadow_spread", 0.0),
        number("shadow_offset_x", 0.0),
        number("shadow_offset_y", 0.0),
        boolean("shadow_inner", false),
        // Where the stroke sits against the edge: inside, centred or
        // outside. A rectangle border has always been inside and a
        // field stroke centred; they are one outline now, so both are
        // sayable on either.
        string("stroke_alignment", "centre"),
        // The shape of a smooth seam. `quadratic` is the polynomial
        // blend a field has always had: soft, and it swells where two
        // shapes merely come close. `circular` rounds the joint with
        // an arc of the blend radius and leaves both shapes exact
        // outside it — the concave fillet where a panel meets a frame.
        string("blend_profile", "quadratic"),
    ]
}

/// What `ui.SdfShape` adds to every element's properties.
pub(super) fn sdf_shape() -> Vec<PropertySpec> {
    vec![
        string("shape", "circle"),
        // A letter, as a shape in the composition rather than as text
        // drawn beside it. Naming one makes this layer that letter's
        // outline, which then unions, subtracts and morphs with a
        // circle by the same arithmetic a circle does — so a numeral
        // cut out of a disc is a subtraction, and the disc becoming a
        // square while the numeral becomes another is one animation.
        //
        // `glyph_morph_to` names the letter it turns into, walked at
        // `morph_progress` alongside whatever the shapes are doing.
        string("glyph", ""),
        string("glyph_morph_to", ""),
        // A drawing, on exactly the same terms. An SVG is a set of
        // closed curves and so is a letter, so naming a file here makes
        // this layer that drawing's outline — which then unions,
        // subtracts and morphs like every other shape, including into a
        // letter or a circle. Nothing is rasterised on the way: a
        // picture of a shape has pixels rather than points, and there is
        // nothing in a picture to walk onto anything else.
        //
        // `source_morph_to` names the drawing it turns into, walked at
        // `morph_progress` beside whatever the shapes are doing.
        string("source", ""),
        string("source_morph_to", ""),
        // Which face the letter is cut from, and which the letter it
        // turns into is cut from. Empty means the same face, which is
        // the ordinary case; naming a second one morphs across faces,
        // since matching two outlines is geometry and does not care
        // which font either of them came out of.
        string("font_family", "sans-serif"),
        string("font_family_morph_to", ""),
        // The weight the letter is cut at (100-900): the face's own
        // bold, or a variable face's `wght` there, for both ends.
        number("font_weight", 400.0),
        // The layer's own fill. Fully transparent means "take the
        // field's", which is what keeps a single-colour composition
        // from having to repeat itself on every layer.
        color("fill_color", Color::rgba8(0, 0, 0, 0)),
        // The field this layer becomes at `morph_progress` of one.
        // Interpolating two distance fields passes through shapes that
        // neither end describes, and survives a change of topology —
        // one blob splitting into two — which interpolating outlines
        // cannot do at all.
        string("morph_to", ""),
        // Negative means "follow the field's", so a layer joins the
        // compound morph by saying nothing and leaves it by naming its
        // own position.
        number("morph_progress", -1.0),
        string("operation", "union"),
        // How far either side of the seam a smooth operation blends.
        // Zero is the hard boolean; animating it is what makes two
        // shapes merge and part like liquid.
        number("blend", 0.0),
        number("radius", 0.0),
        // Per-corner overrides, as a Rect carries them: negative means
        // "use the uniform radius". A field box keeps all four, so a
        // rect absorbed into a composition keeps its own shape.
        number("top_left_radius", -1.0),
        number("top_right_radius", -1.0),
        number("bottom_right_radius", -1.0),
        number("bottom_left_radius", -1.0),
        number("points", 5.0),
        number("inner_radius", 0.5),
        // A ring's or a cross's wall; for a letter or a drawing, how
        // much heavier it is drawn -- its outline grown by this much,
        // a bold that needs no bold face.
        number("thickness", 0.0),
        number("angle", 90.0),
        // A linear map `{ a, b, c, d }` the shape is drawn through
        // about its centre, after `rotation`: a shear, a squash, a
        // mirror. The distance is scaled back so the edge stays one
        // pixel soft however the map stretches it.
        any("matrix", Value::Nil),
        // Which layers this one blends with. Two layers in different
        // non-zero groups meet with a hard edge even when the
        // operation is smooth; group 0 blends with everything. Four
        // panels coming out of one frame blend into the frame and
        // not into one another.
        number("blend_group", 0.0),
    ]
}

/// What `ui.Path` adds to every element's properties.
pub(super) fn path() -> Vec<PropertySpec> {
    vec![
        // SVG path data: `M 0 0 L 10 10 A 5 5 0 0 1 20 20 Z`, every
        // command, absolute or relative.
        string("d", ""),
        // The outline this one turns into, and how far along it is.
        // The two are walked point by point when they have the same
        // run of segments (lines and curves count alike); otherwise
        // the outline changes over at the halfway mark.
        string("morph_to", ""),
        number("morph_progress", 0.0),
        // What SVG does with a path that says nothing: filled black,
        // not stroked, a unit-wide stroke once it has a colour.
        color("fill_color", Color::rgba8(0, 0, 0, 255)),
        string("fill_rule", "nonzero"),
        color("stroke_color", Color::rgba8(0, 0, 0, 0)),
        number("stroke_width", 1.0),
        string("stroke_cap", "butt"),
        string("stroke_join", "miter"),
        number("miter_limit", 4.0),
        // Dash and gap lengths, in path units, repeated; an odd list
        // is read twice over, as SVG reads it.
        any("dash", Value::List(Vec::new())),
        number("dash_offset", 0.0),
        // The part of the outline that is stroked, as fractions of
        // its length: a progress ring is `trim_end`, a line drawing
        // itself on is `trim_end` going from zero to one.
        number("trim_start", 0.0),
        number("trim_end", 1.0),
        // `{ x, y, w, h }` (or `{ x, y, width, height }`, or four
        // numbers): the part of path space that fills the node. Empty
        // means path units are the node's own pixels.
        any("view_box", Value::Map(BTreeMap::new())),
        // How a view box that is not the node's shape fits it:
        // `stretch`, `preserve_aspect_fit` or `preserve_aspect_crop`,
        // the words an Image uses.
        string("fill_mode", "stretch"),
        // A data channel to draw (`channel.rs`), its id: the outline
        // is made from its numbers where the path is painted, and
        // `d` is not read. `plot` says how: `{ kind = "line" |
        // "area" | "steps" | "steps_area" | "hatch_steps" | "bars",
        // width, height, samples, bottom, top, headroom, floor,
        // pad_top, pad_bottom, smooth, gap, radius, min_bar, mirror,
        // hatch, with }` (`morf_vector::series`).
        any("series", Value::Nil),
        any("plot", Value::Nil),
    ]
}
