//! How a draw command styles what it draws: a node's shader, a distance
//! field's edge, an image's fill and a text's vertical place.

use morf_scene::Color;

/// A compiled shader attached to a node, and the values it was given.
///
/// The WGSL itself is not here: it was compiled and registered with the backend
/// once, at configuration load, and this only says which one and with what. A
/// draw command is compared every frame to decide damage, so it holds the
/// cheap half.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderBinding {
    /// Which registered program, by the hash of its generated WGSL.
    pub program: u64,
    /// Parameter values, flattened in declaration order. The backend places
    /// them using the layout the compiler computed, so the two cannot disagree.
    pub params: Vec<f32>,
    /// Values for the shader's data blocks, in binding order.
    pub data: Vec<Vec<f32>>,
    /// Whether the shader reads what is rendered underneath it.
    pub samples_behind: bool,
    /// Whether the shader decides coverage, and so needs the node's whole
    /// rectangle rather than the reach of the shape it replaced.
    pub owns_coverage: bool,
}

/// Edge shaping applied when sampling a cached distance field.
///
/// The field is a continuous distance, not a coverage mask, so where the edge
/// sits and how sharply it falls off are decisions taken at sampling time. That
/// makes every one of these an ordinary animatable scene property rather than a
/// property of the cached texture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceFieldStyle {
    /// How far to move the edge, in logical pixels, from where the shape says.
    ///
    /// Positive thickens and negative thins, the way a variable font gains or
    /// loses weight, and zero is the shape as drawn. Logical pixels rather than
    /// normalised field units because that is what a configuration can reason
    /// about: half a pixel more weight means the same thing at every size.
    ///
    /// This used to be an absolute threshold neutral at `0.5` for images and a
    /// signed offset neutral at `0.0` for text — one field, two unit systems,
    /// told apart only by which function had filled it in, with a `Default`
    /// that was right for one of them and wrong for the other.
    pub thickness: f32,
    /// Extra edge feathering in source pixels, on top of pixel-derived coverage.
    pub softness: f32,
    /// Outline band drawn outside the fill edge, in source pixels.
    pub outline_width: f32,
    /// Outline colour, composited beneath the fill.
    pub outline_color: Color,
}

impl Default for DistanceFieldStyle {
    fn default() -> Self {
        Self {
            thickness: 0.0,
            softness: 0.0,
            outline_width: 0.0,
            outline_color: Color::rgba8(0, 0, 0, 0),
        }
    }
}

/// Image placement policy inside resolved node bounds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImageFillMode {
    #[default]
    Stretch,
    PreserveAspectFit,
    PreserveAspectCrop,
}

/// Vertical positioning for shaped text inside its node bounds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VerticalAlignment {
    #[default]
    Top,
    Center,
    Bottom,
}
