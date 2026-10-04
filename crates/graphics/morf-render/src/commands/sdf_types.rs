use morf_layout::Geometry;
// The one shape vocabulary, shared with the input-region rasteriser so a
// star-shaped node is clickable as a star. Re-exported, so naming a shape does
// not oblige a caller to depend on `morf-region` directly.
use morf_scene::Color;
pub use morf_value::region::{BlendProfile, Operation, Shape, ShapeParams};

/// One analytic distance field, and how it joins the composition.
///
/// Not `Copy`: a letter names the face it is cut from, and a face is a name.
/// Only a layer that *is* a letter carries one, so a composition of plain
/// shapes still allocates nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct SdfLayer {
    /// Layer rectangle in logical surface coordinates.
    pub bounds: Geometry,
    /// Resolved fill for this layer, already fallen back to the field's own.
    pub color: Color,
    /// Shape at `morph` of zero.
    pub shape: Shape,
    /// Shape at `morph` of one.
    pub morph_to: Shape,
    /// Position between the two fields, clamped to zero through one.
    pub morph: f32,
    /// How this layer joins the ones before it.
    pub operation: Operation,
    /// Seam radius for a smooth operation, in logical pixels.
    pub blend: f32,
    /// How much of the layer is there, from 0 (as if it were absent) to 1:
    /// the `SdfShape`'s (or `Rect`'s) own `opacity` times that of every node
    /// between it and the field. The field's coverage and colour are mixed
    /// between the composition without the layer and with it, so the seam
    /// fades with the layer and the rest of the field does not change.
    pub opacity: f32,
    /// Rotation about the layer centre, in degrees.
    pub rotation: f32,
    /// A linear map `[a, b, c, d]` (column major) the shape is drawn through
    /// about its centre, after `rotation`: the identity for an ordinary
    /// layer. A layer tracking a sheared or stretched node carries the node's
    /// linear transform here.
    pub matrix: [f32; 4],
    /// Which layers this one blends with: two in different non-zero groups
    /// meet with a hard edge whatever the operation says.
    pub blend_group: u32,
    /// The shape of this layer's smooth seam.
    pub profile: BlendProfile,
    /// Corner radii — top-left, top-right, bottom-right, bottom-left — for the
    /// shapes that have corners. A rect absorbed into a field keeps all four.
    pub radii: [f32; 4],
    /// Point count, for `Star`.
    pub points: f32,
    /// Waist as a fraction of the outer radius, for `Star`.
    pub inner_radius: f32,
    /// Arm or wall thickness, for `Ring` and `Cross`.
    pub thickness: f32,
    /// Sector sweep in degrees, for `Pie`.
    pub angle: f32,
    /// The letter this layer is, for `Polygon`.
    ///
    /// A glyph is not a family of shape with parameters — it is one particular
    /// outline — so it reaches the composition as a character and is resolved
    /// to points when the frame is gathered, where the fonts are. One character
    /// rather than a string: a layer is one shape, and a word is a row of them.
    pub glyph: Option<char>,
    /// The letter it is turning into, interpolated at `morph`.
    ///
    /// The points are walked to their opposite numbers on the CPU and the
    /// result is one outline, so a morphing letter costs the composition
    /// exactly what a still one does.
    pub glyph_morph_to: Option<char>,
    /// The drawing this layer is, for `Polygon` — the same slot a glyph fills,
    /// because by the time a field sees either one they are both an outline.
    pub svg_source: Option<Box<str>>,
    /// The drawing it turns into.
    pub svg_source_morph_to: Option<Box<str>>,
    /// The face the letter is cut from, or `None` for the default one.
    ///
    /// Which outline a glyph is depends on the face as much as on the
    /// character: an `8` in a grotesque and an `8` in a script are two
    /// different shapes.
    pub font_family: Option<Box<str>>,
    /// The face the letter it turns into is cut from, if not this one.
    ///
    /// The correspondence between two outlines is geometric — contours matched
    /// by position, resampled, rotated onto each other — and knows nothing
    /// about where either came from. So a grotesque `8` walks to a blackletter
    /// `W` exactly as it walks to its own `W`, and changing the face mid-morph
    /// is a morph rather than a swap.
    pub font_family_morph_to: Option<Box<str>>,
}
