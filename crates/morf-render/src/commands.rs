use morf_layout::{Geometry, Layout, TextAlignment, TextElide, TextStyle, Transform2D};
use morf_scene::{Color, Gradient, NodeHandle, Scene, TextDecoration};
use std::ops::Range;

use crate::{effects::*, field::*, paint::*, sdf::*};

mod sdf_types;

pub use sdf_types::*;

/// What a text input adds to its text: where it has scrolled to, what is
/// selected, and where the caret is.
///
/// Offsets are into the shaped string — the dots of a password, not its
/// letters — because it is the shaped string the glyphs came from.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    /// How far the content has scrolled under the box.
    pub scroll: (f64, f64),
    /// The selected range, empty when nothing is.
    pub selection: Range<usize>,
    /// Behind the selected text.
    pub selection_color: Color,
    /// The selected text's own colour; fully transparent keeps the text's.
    pub selected_text_color: Color,
    /// The caret's offset, or nothing while it is hidden.
    pub caret: Option<usize>,
    /// The caret's colour, already resolved from the text's when unset.
    pub caret_color: Color,
    /// The caret's width in logical pixels.
    pub caret_width: f64,
    /// Whether the text drawn is the placeholder; its caret then stands at the
    /// start of the line, wherever the alignment would put nothing.
    pub placeholder: bool,
}

/// One ordered paint operation emitted from the scene graph.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    /// SDF rounded rectangle and border.
    Quad {
        /// Source scene node.
        node: NodeHandle,
        /// Logical surface bounds.
        bounds: Geometry,
        /// Composed node and ancestor transform.
        transform: Transform2D,
        /// Intersected ancestor clip in logical surface coordinates.
        clip: Option<Geometry>,
        /// Fill colour after node opacity.
        color: Color,
        /// Inherited colour overlay.
        color_overlay: Color,
        /// A gradient across the rectangle, if it has one.
        gradient: Option<Gradient>,
        /// Corner radii in top-left clockwise order.
        radii: [f64; 4],
        /// Border width.
        border_width: f64,
        /// If rectangle edges use smooth coverage.
        antialiasing: bool,
        /// If the border width is rounded in physical pixels.
        border_pixel_aligned: bool,
        /// Border colour after node opacity.
        border_color: Color,
        /// Fill-edge blur radius.
        blur: f64,
        /// Outer shadow colour.
        shadow_color: Color,
        /// Shadow blur radius.
        shadow_blur: f64,
        /// Shadow expansion around the rectangle.
        shadow_spread: f64,
        /// Shadow horizontal displacement.
        shadow_offset_x: f64,
        /// Shadow vertical displacement.
        shadow_offset_y: f64,
        /// Draw the shadow inside the rectangle edge.
        shadow_inner: bool,
        /// A configuration's own shader, if this node carries one.
        ///
        /// A rectangle is a field of one layer, so it wears a shader the same
        /// way — and `ui.Rect { shader = ... }` is how anybody first reaches
        /// for one, which is why leaving it off here made the feature look
        /// broken rather than absent.
        shader: Option<ShaderBinding>,
    },
    /// Shaped glyph run owned by the text subsystem.
    Text {
        /// Source scene node.
        node: NodeHandle,
        /// Logical surface bounds.
        bounds: Geometry,
        /// Composed node and ancestor transform.
        transform: Transform2D,
        /// Intersected ancestor clip in logical surface coordinates.
        clip: Option<Geometry>,
        /// UTF-8 text used to locate its shaped buffer.
        text: String,
        /// Font family used by the shaping cache.
        family: String,
        /// Optional local font file or directory.
        font_source: String,
        /// Logical font size.
        size: f64,
        /// Numeric OpenType font weight.
        font_weight: f64,
        /// Glyph colour after node opacity.
        color: Color,
        /// Inherited colour overlay.
        color_overlay: Color,
        /// Whether lines wrap at the resolved width.
        wrap: bool,
        /// Lines kept when wrapping, the last elided; zero keeps all.
        max_lines: usize,
        /// Ellipsis placement for an overflowing unwrapped line.
        elide: TextElide,
        /// Horizontal line alignment.
        horizontal_alignment: TextAlignment,
        /// Vertical placement inside the resolved height.
        vertical_alignment: VerticalAlignment,
        /// How the glyph field is thresholded: edge, softness and outline.
        field_style: DistanceFieldStyle,
        /// Text this run is interpolating towards, empty when it is not.
        morph_to: String,
        /// How far between the two, zero at the run's own text.
        morph_progress: f32,
        /// Line height, spacing, slant and width.
        style: TextStyle,
        /// A line under, over or through the text, if it has one.
        decoration: Option<TextDecoration>,
        /// The caret, selection and scroll of a text input; nothing for text
        /// that is only read.
        edit: Option<Box<TextEdit>>,
    },
    /// Rasterized image or theme icon.
    Texture {
        /// Source scene node.
        node: NodeHandle,
        /// Logical surface bounds.
        bounds: Geometry,
        /// Composed node and ancestor transform.
        transform: Transform2D,
        /// Intersected ancestor clip in logical surface coordinates.
        clip: Option<Geometry>,
        /// Image path or icon name.
        source: String,
        /// Theme name for an icon command.
        icon_theme: Option<String>,
        /// Inherited colour overlay.
        color_overlay: Color,
        /// Aspect-ratio policy inside the resolved bounds.
        fill_mode: ImageFillMode,
        /// Filtered sampling; false takes the nearest texel.
        smooth: bool,
        /// Interpret source alpha as a cached signed distance field mask.
        distance_field: bool,
        /// Pixel distance represented on either side of the mask edge.
        distance_field_spread: f32,
        /// Edge shaping applied to the sampled field.
        distance_field_style: DistanceFieldStyle,
        /// Which frame of a moving picture to show; still ones ignore it.
        frame: u32,
    },
    /// A vector outline, rasterised at the pixels it covers.
    Path {
        /// Source scene node.
        node: NodeHandle,
        /// Logical surface bounds.
        bounds: Geometry,
        /// Composed node and ancestor transform.
        transform: Transform2D,
        /// Intersected ancestor clip in logical surface coordinates.
        clip: Option<Geometry>,
        /// Inherited colour overlay.
        color_overlay: Color,
        /// The outline and how it is filled and stroked.
        paint: Box<crate::path::PathPaint>,
    },
    /// Composed signed-distance field resolved in one fragment shader.
    Field {
        /// Source scene node.
        node: NodeHandle,
        /// Logical surface bounds.
        bounds: Geometry,
        /// Composed node and ancestor transform.
        transform: Transform2D,
        /// Intersected ancestor clip in logical surface coordinates.
        clip: Option<Geometry>,
        /// Fill colour after node opacity.
        fill_color: Color,
        /// Outline colour after node opacity.
        stroke_color: Color,
        /// Logical outline width.
        stroke_width: f64,
        /// Where that outline sits against the edge.
        stroke_alignment: BorderAlignment,
        /// Extra edge softness in logical pixels.
        softness: f64,
        /// Gradient across the node's own rectangle, if any.
        gradient: Option<Gradient>,
        /// Multiplied over the finished surface.
        color_overlay: Color,
        /// Drop shadow colour; fully transparent means no shadow.
        shadow_color: Color,
        /// Shadow edge softness in logical pixels.
        shadow_blur: f64,
        /// How far the shadow is dilated past the shape.
        shadow_spread: f64,
        shadow_offset_x: f64,
        shadow_offset_y: f64,
        /// Whether the shadow falls inside the shape rather than behind it.
        shadow_inner: bool,
        /// A configuration's own shader, if this node carries one.
        shader: Option<ShaderBinding>,
        /// Layers in composition order; the first establishes the field.
        layers: Vec<SdfLayer>,
    },
    /// Frosted glass: what this surface has already drawn beneath a rounded
    /// rectangle, blurred and drawn back inside it.
    ///
    /// Emitted just before the rectangle's own fill, which then tints it. The
    /// backend blurs at reduced resolution and keeps the result until what is
    /// beneath changes, so a still desk costs one textured quad per panel.
    Backdrop {
        /// Source scene node.
        node: NodeHandle,
        /// Logical surface bounds of the shape.
        bounds: Geometry,
        /// Composed node and ancestor transform.
        transform: Transform2D,
        /// Intersected ancestor clip in logical surface coordinates.
        clip: Option<Geometry>,
        /// Corner radii in top-left clockwise order.
        radii: [f64; 4],
        /// Logical blur radius, already capped at [`MAX_BACKDROP_BLUR`].
        radius: f64,
        /// Saturation of the blurred backdrop, 1 unchanged and 0 grey.
        saturation: f64,
    },
    /// A terminal's screen: a grid of cells, each drawn at its own column
    /// and row whatever the font would have advanced it by.
    Terminal {
        /// Source scene node.
        node: NodeHandle,
        /// Logical surface bounds.
        bounds: Geometry,
        /// Composed node and ancestor transform.
        transform: Transform2D,
        /// Intersected ancestor clip in logical surface coordinates.
        clip: Option<Geometry>,
        /// Inherited colour overlay.
        color_overlay: Color,
        /// What the screen shows. Shared with the runtime, which made it;
        /// comparing two is a pointer comparison when nothing changed.
        screen: std::sync::Arc<morf_scene::TerminalScreen>,
    },
}

/// The largest backdrop blur radius, in logical pixels.
///
/// Past this a frosted panel is a flat wash of the average colour anyway, and
/// every doubling of the radius is another pass over the region.
pub const MAX_BACKDROP_BLUR: f64 = 96.0;

impl DrawCommand {
    /// For a backdrop, the area it reads beneath itself: its shape widened by
    /// how far the blur reaches, so the edge of the glass pulls in what lies
    /// just outside it rather than darkening towards nothing.
    pub fn backdrop_reach(&self) -> Option<Geometry> {
        let Self::Backdrop {
            bounds,
            transform,
            radius,
            ..
        } = self
        else {
            return None;
        };
        Some(crate::effects::expand_geometry(
            transform.bounds(*bounds),
            radius * 2.0,
        ))
    }
}

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

/// Ordered commands for one surface frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    /// Back-to-front paint operations.
    pub commands: Vec<DrawCommand>,
    /// Nested offscreen subtree layers.
    pub layers: Vec<Layer>,
}

/// One subtree rendered into an offscreen target before composition.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// Scene node that owns the layer.
    pub node: NodeHandle,
    /// Contiguous command range contained by the subtree.
    pub commands: Range<usize>,
    /// Containing layer, if nested.
    pub parent: Option<usize>,
    /// Opacity applied once while compositing the complete subtree.
    pub opacity: f32,
    /// Logical dual-kawase blur radius.
    pub blur: f32,
    /// Colour applied to the blurred subtree alpha behind the layer.
    pub shadow_color: Color,
    /// Logical dual-kawase shadow radius.
    pub shadow_blur: f32,
    /// Logical shadow displacement.
    pub shadow_offset: [f32; 2],
    /// Rounded owner geometry used to mask the composited subtree.
    pub mask: Option<LayerMask>,
    /// An effect shader applied while compositing the subtree.
    ///
    /// It lives on the layer rather than on a command because there is nothing
    /// to sample until the subtree has been rendered into its own target —
    /// which is exactly what a layer is for.
    pub shader: Option<ShaderBinding>,
    /// Logical bounds affected by this layer.
    pub bounds: Geometry,
    /// Another layer whose alpha multiplies this one's as it is composited:
    /// the node's `mask`.
    pub alpha_mask: Option<AlphaMask>,
    /// For a layer that is another's alpha mask, that layer. It is rendered
    /// over exactly the region its owner is, and never composited itself.
    pub mask_for: Option<usize>,
}

/// Where a layer's alpha mask was rendered, and how it is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlphaMask {
    /// The layer holding the mask, listed right after its owner's commands.
    pub layer: usize,
    /// Keeps what the mask does not cover rather than what it does.
    pub invert: bool,
}

/// Rounded geometry applied while compositing an offscreen layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerMask {
    /// Owner geometry before its transform.
    pub bounds: Geometry,
    /// Composed owner transform.
    pub transform: Transform2D,
    /// Corner radii in top-left clockwise order.
    pub radii: [f64; 4],
}

impl DrawList {
    /// Builds a draw list from resolved scene geometry.
    pub fn from_scene(scene: &Scene, layout: &Layout) -> Result<Self, RenderError> {
        let mut list = Self::default();
        list.rebuild(scene, layout)?;
        Ok(list)
    }

    /// Refills this list from the scene, keeping the memory it already holds.
    ///
    /// A command is 350-odd bytes and a busy surface has thousands of them, so
    /// a list built afresh every frame is hundreds of kilobytes allocated, filled
    /// and returned to the allocator sixty times a second. Reusing the buffer
    /// keeps the capacity and the pages, which is most of the cost once a scene
    /// is large enough to leave the cache.
    pub fn rebuild(&mut self, scene: &Scene, layout: &Layout) -> Result<(), RenderError> {
        self.commands.clear();
        self.layers.clear();
        let list = self;
        for root in scene.roots() {
            append_node(
                scene,
                layout,
                root,
                PaintContext {
                    transform: Transform2D::IDENTITY,
                    clip: None,
                    overlay: Color::rgba8(0, 0, 0, 0),
                    layer: None,
                    in_field: false,
                    color: None,
                },
                list,
            )?;
        }
        Ok(())
    }
}
