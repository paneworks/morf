//! Flex and grid containers, laid out by Taffy.
//!
//! A `Flex`, or a `Grid` given track lists, is a root that Taffy lays out
//! whole: every descendant that is itself such a container is a Taffy node
//! too, and everything else under it is a Taffy leaf whose size is what the
//! engine already measured -- text through the text measurer, at the width
//! Taffy offers, since wrapping is the whole reason to ask. The leaf's own
//! children are then laid out by the engine's ordinary rules, so a card
//! placed by a grid still anchors its label in its corner.
//!
//! The tree is built afresh each layout pass. A pass already happens only
//! when something that affects layout changed, and a shadow tree kept in
//! step with the scene is a second thing to keep right for a saving that
//! has not been measured.

use morf_scene::{Element, FastMap, NodeHandle, Scene};
use taffy::prelude::*;
use taffy::tree::{LayoutInput, LayoutOutput};

use crate::flex_style::{is_flex_root, item_style};
use crate::geometry::{Geometry, Size as EngineSize, TextMeasurer, TextOptions};
use crate::helpers::{LayoutError, anchors, positive, text_alignment, text_elide};

/// One node Taffy placed: its geometry, whether the engine still places its
/// children, and where it sits from its Taffy parent's origin.
pub(crate) type Flexed = (NodeHandle, Geometry, bool, (f64, f64));

/// The Taffy tree for one flex root, and which engine node each id is.
pub(crate) struct FlexTree {
    tree: TaffyTree<NodeHandle>,
    root: NodeId,
    /// Which engine node each Taffy node is.
    ids: FastMap<NodeId, NodeHandle>,
}

impl FlexTree {
    /// Builds the tree under `root`, a node for which `is_flex_root` holds.
    pub(crate) fn build(scene: &Scene, root: NodeHandle) -> Result<Self, LayoutError> {
        let mut tree = TaffyTree::new();
        let mut ids = FastMap::default();
        let id = Self::add(scene, root, &mut tree, &mut ids)?;
        Ok(Self {
            tree,
            root: id,
            ids,
        })
    }

    fn add(
        scene: &Scene,
        node: NodeHandle,
        tree: &mut TaffyTree<NodeHandle>,
        ids: &mut FastMap<NodeId, NodeHandle>,
    ) -> Result<NodeId, LayoutError> {
        let style = item_style(scene, node)?;
        let id = if is_flex_root(scene, node)? {
            let mut children = Vec::new();
            for &child in scene.children(node)? {
                // A mask is placed in the root's box afterwards, not by Taffy.
                if scene.is_mask(child) {
                    continue;
                }
                // A child the scene no longer has, still listed here, is a
                // bookkeeping fault worth naming: which container, which slot.
                if scene.element(child).is_err() {
                    return Err(LayoutError::Scene(format!(
                        "stale scene node handle: child {child:?} of {:?} {node:?}",
                        scene.element(node)
                    )));
                }
                // One kind owns a child's placement. Anchors speak to a plain
                // parent; here the container places the child, and an
                // anchor would be silently ignored, which is worse than an
                // error that says so.
                if anchors(scene.current(child, "anchors")?)?
                    .values()
                    .any(|value| matches!(value, morf_scene::Value::Bool(true)))
                {
                    return Err(LayoutError::AxisConflict { axis: "flex" });
                }
                if scene.bool_value(child, "visible")? {
                    children.push(Self::add(scene, child, tree, ids)?);
                }
            }
            tree.new_with_children(style, &children)
        } else {
            tree.new_leaf_with_context(style, node)
        }
        .map_err(|error| LayoutError::Scene(error.to_string()))?;
        ids.insert(id, node);
        Ok(id)
    }

    /// Gives the root the box the engine resolved for it, so a Flex with no
    /// size of its own fills what its parent gave it rather than shrinking
    /// to its content.
    pub(crate) fn set_root_size(&mut self, width: f64, height: f64) -> Result<(), LayoutError> {
        let mut style = self
            .tree
            .style(self.root)
            .map_err(|error| LayoutError::Scene(error.to_string()))?
            .clone();
        style.size = Size {
            width: Dimension::length(width as f32),
            height: Dimension::length(height as f32),
        };
        self.tree
            .set_style(self.root, style)
            .map_err(|error| LayoutError::Scene(error.to_string()))
    }

    /// Lays the tree out in `available`, measuring leaves as the engine does.
    pub(crate) fn compute(
        &mut self,
        scene: &Scene,
        available: Size<AvailableSpace>,
        requested: &FastMap<NodeHandle, EngineSize>,
        text: &mut impl TextMeasurer,
    ) -> Result<(), LayoutError> {
        let mut failure = None;
        self.tree
            .compute_layout_with_measure(
                self.root,
                available,
                |input: LayoutInput, _id, context: Option<&mut NodeHandle>, _style| {
                    let Some(&mut node) = context else {
                        return LayoutOutput::HIDDEN;
                    };
                    match measure_leaf(scene, node, input, requested, text) {
                        Ok(size) => LayoutOutput::from_outer_size(size),
                        Err(error) => {
                            failure.get_or_insert(error);
                            LayoutOutput::HIDDEN
                        }
                    }
                },
            )
            .map_err(|error| LayoutError::Scene(error.to_string()))?;
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// The root's laid-out size.
    pub(crate) fn size(&self) -> EngineSize {
        let layout = self.tree.unrounded_layout(self.root);
        EngineSize {
            width: f64::from(layout.size.width),
            height: f64::from(layout.size.height),
        }
    }

    /// Every node's geometry, absolute within the root at `origin`, in
    /// tree order, with whether it is a leaf whose own children the engine
    /// still has to place.
    pub(crate) fn geometries(
        &self,
        scene: &Scene,
        origin: Geometry,
    ) -> Result<Vec<Flexed>, LayoutError> {
        let mut out = Vec::with_capacity(self.ids.len());
        self.collect(scene, self.root, origin.x, origin.y, &mut out)?;
        Ok(out)
    }

    fn collect(
        &self,
        scene: &Scene,
        id: NodeId,
        x: f64,
        y: f64,
        out: &mut Vec<Flexed>,
    ) -> Result<(), LayoutError> {
        let children = self
            .tree
            .children(id)
            .map_err(|error| LayoutError::Scene(error.to_string()))?;
        for child in children {
            let layout = self.tree.unrounded_layout(child);
            let node = *self
                .ids
                .get(&child)
                .ok_or_else(|| LayoutError::Scene("flex node without a scene node".into()))?;
            let location = (f64::from(layout.location.x), f64::from(layout.location.y));
            let geometry = Geometry {
                x: x + location.0,
                y: y + location.1,
                width: f64::from(layout.size.width),
                height: f64::from(layout.size.height),
            };
            let leaf = !is_flex_root(scene, node)?;
            out.push((node, geometry, leaf, location));
            if !leaf {
                self.collect(scene, child, geometry.x, geometry.y, out)?;
            }
        }
        Ok(())
    }
}

/// What a leaf answers when Taffy asks how big it is.
///
/// Text is shaped at the width on offer, so it wraps where the container
/// says; anything else is the size it asked for or was measured at.
fn measure_leaf(
    scene: &Scene,
    node: NodeHandle,
    input: LayoutInput,
    requested: &FastMap<NodeHandle, EngineSize>,
    text: &mut impl TextMeasurer,
) -> Result<Size<f32>, LayoutError> {
    let known = input.known_dimensions;
    if scene.element(node)? == Element::Text {
        let offered = known.width.or(match input.available_space.width {
            AvailableSpace::Definite(width) => Some(width),
            _ => None,
        });
        let measured = text.measure(
            node,
            scene.string_value(node, "text")?,
            scene.string_value(node, "font_family")?,
            scene.number(node, "font_size")?,
            TextOptions {
                width: offered
                    .map(f64::from)
                    .or(positive(scene.number(node, "width")?)),
                wrap: scene.bool_value(node, "wrap")?,
                alignment: text_alignment(scene.string_value(node, "horizontal_alignment")?)?,
                elide: text_elide(scene.string_value(node, "elide")?)?,
                font_weight: scene.number(node, "font_weight")?,
                font_source: match scene.string_value(node, "font_source")? {
                    "" => None,
                    source => Some(source.to_owned()),
                },
                max_lines: scene.number(node, "max_lines")?.max(0.0) as usize,
                style: crate::text_style::TextStyle::from_scene(scene, node)?,
            },
        );
        return Ok(Size {
            width: known.width.unwrap_or(measured.width as f32),
            height: known.height.unwrap_or(measured.height as f32),
        });
    }
    // A field that wraps is as tall as its lines at the width it is offered,
    // like text; one that does not keeps the size it measured.
    if scene.element(node)? == Element::TextInput
        && scene.bool_value(node, "multiline")?
        && scene.bool_value(node, "wrap")?
        && known.height.is_none()
    {
        let offered = known.width.or(match input.available_space.width {
            AvailableSpace::Definite(width) => Some(width),
            _ => None,
        });
        let shape = crate::text_input::InputShape::read(scene, node, offered.map(f64::from))?;
        let line = shape.line_height();
        let measured = text.measure(
            node,
            &shape.display.text,
            &shape.family,
            shape.size,
            shape.options,
        );
        let size = requested.get(&node).copied().unwrap_or_default();
        return Ok(Size {
            width: known.width.unwrap_or(size.width as f32),
            height: measured.height.max(line) as f32,
        });
    }
    let size = requested.get(&node).copied().unwrap_or_default();
    Ok(Size {
        width: known.width.unwrap_or(size.width as f32),
        height: known.height.unwrap_or(size.height as f32),
    })
}
