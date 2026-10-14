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
//! The tree is kept between passes, one for measuring a root and one for
//! placing it, and brought into step with the scene before each use: the
//! children of every container are read again, a node's style is read
//! again only when the scene stamped it as changed, and a leaf is marked
//! dirty when it was stamped or asks for another size. Taffy then reuses
//! what it worked out last time for every subtree nothing touched.

use morf_scene::{Element, FastMap, FastSet, NodeHandle, Scene};
use taffy::prelude::*;
use taffy::tree::{LayoutInput, LayoutOutput, RunMode};

use crate::flex_style::{is_flex_root, item_style};
use crate::geometry::{Geometry, Size as EngineSize, TextMeasurer, TextOptions};
use crate::helpers::{LayoutError, anchors, positive, text_alignment, text_elide};

/// One node Taffy placed: its geometry, whether the engine still places its
/// children, and where it sits from its Taffy parent's origin.
pub(crate) type Flexed = (NodeHandle, Geometry, bool, (f64, f64));

/// What Taffy asked a leaf: the sizes it knew, and the room on offer.
type Asked = (Size<Option<f32>>, Size<AvailableSpace>);

/// What a flex root's tree is used for: its implicit size, measured
/// unconstrained, or its children's places at its resolved size. Each keeps
/// a tree of its own, so neither overwrites what Taffy kept for the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Purpose {
    Measure,
    Place,
}

/// The Taffy trees a layout keeps, by flex root and purpose.
#[derive(Clone, Default)]
pub(crate) struct FlexTrees(FastMap<(NodeHandle, Purpose), FlexTree>);

impl std::fmt::Debug for FlexTrees {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FlexTrees({})", self.0.len())
    }
}

impl FlexTrees {
    /// The tree for `root`, in step with the scene and with `requested`.
    pub(crate) fn get(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        purpose: Purpose,
        requested: &FastMap<NodeHandle, EngineSize>,
    ) -> Result<&mut FlexTree, LayoutError> {
        let tree = self
            .0
            .entry((root, purpose))
            .or_insert_with(|| FlexTree::new(root));
        tree.sync(scene, requested)?;
        Ok(tree)
    }

    /// Drops the trees of roots no longer in the scene.
    pub(crate) fn retain(&mut self, scene: &Scene) {
        self.0.retain(|(root, _), _| scene.contains(*root));
    }
}

/// One engine node in a tree.
#[derive(Clone, Debug)]
struct Entry {
    id: NodeId,
    container: bool,
    /// The size a leaf asked for when it was last measured.
    requested: Option<EngineSize>,
    /// The sync that last found it in the tree.
    seen: u64,
}

/// The Taffy tree for one flex root, and which engine node each id is.
#[derive(Clone)]
pub(crate) struct FlexTree {
    tree: TaffyTree<NodeHandle>,
    root: NodeHandle,
    root_id: Option<NodeId>,
    /// The root's own style, before `set_root_size` gives it a size.
    root_style: Style,
    nodes: FastMap<NodeHandle, Entry>,
    /// Which engine node each Taffy node is.
    ids: FastMap<NodeId, NodeHandle>,
    /// What each leaf was last asked while Taffy laid it out for good.
    performed: FastMap<NodeHandle, Asked>,
    /// The layout revision of the last sync; `None` before the first.
    synced: Option<u64>,
    generation: u64,
}

impl FlexTree {
    fn new(root: NodeHandle) -> Self {
        Self {
            tree: TaffyTree::new(),
            root,
            root_id: None,
            root_style: Style::default(),
            nodes: FastMap::default(),
            ids: FastMap::default(),
            performed: FastMap::default(),
            synced: None,
            generation: 0,
        }
    }

    /// Builds the tree under `root`, a node for which `is_flex_root` holds.
    #[cfg(test)]
    pub(crate) fn build(
        scene: &Scene,
        root: NodeHandle,
        requested: &FastMap<NodeHandle, EngineSize>,
    ) -> Result<Self, LayoutError> {
        let mut tree = Self::new(root);
        tree.sync(scene, requested)?;
        Ok(tree)
    }

    /// Brings the tree into step with the scene under the root.
    pub(crate) fn sync(
        &mut self,
        scene: &Scene,
        requested: &FastMap<NodeHandle, EngineSize>,
    ) -> Result<(), LayoutError> {
        self.generation += 1;
        let mut moved = false;
        let id = self.visit(scene, self.root, requested, &mut moved)?;
        self.root_id = Some(id);
        if moved {
            let Self {
                tree,
                nodes,
                ids,
                performed,
                generation,
                ..
            } = self;
            nodes.retain(|node, entry| {
                let kept = entry.seen == *generation;
                if !kept {
                    let _ = tree.remove(entry.id);
                    ids.remove(&entry.id);
                    performed.remove(node);
                }
                kept
            });
        }
        self.synced = Some(scene.layout_revision());
        Ok(())
    }

    /// Brings `node` and what is under it into step, and says which Taffy
    /// node it is. `moved` is set when the tree's shape changed.
    fn visit(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        requested: &FastMap<NodeHandle, EngineSize>,
        moved: &mut bool,
    ) -> Result<NodeId, LayoutError> {
        let container = is_flex_root(scene, node)?;
        let own = scene.layout_stamps(node)?.own;
        let stamped = self.synced.is_none_or(|synced| own > synced);
        let known = self
            .nodes
            .get(&node)
            .filter(|entry| entry.container == container)
            .map(|entry| entry.id);
        let id = match known {
            Some(id) => {
                if stamped {
                    let style = item_style(scene, node)?;
                    if node == self.root {
                        self.root_style = style;
                    } else if self.tree.style(id).map_err(taffy_error)? != &style {
                        self.tree.set_style(id, style).map_err(taffy_error)?;
                    }
                    if !container {
                        self.tree.mark_dirty(id).map_err(taffy_error)?;
                    }
                }
                id
            }
            None => {
                if let Some(old) = self.nodes.remove(&node) {
                    let _ = self.tree.remove(old.id);
                    self.ids.remove(&old.id);
                }
                let style = item_style(scene, node)?;
                if node == self.root {
                    self.root_style = style.clone();
                }
                let id = if container {
                    self.tree.new_with_children(style, &[])
                } else {
                    self.tree.new_leaf_with_context(style, node)
                }
                .map_err(taffy_error)?;
                self.ids.insert(id, node);
                *moved = true;
                id
            }
        };
        let mut wanted = None;
        if container {
            let children = self.children(scene, node, requested, moved)?;
            if self.tree.children(id).map_err(taffy_error)? != children {
                self.tree.set_children(id, &children).map_err(taffy_error)?;
                *moved = true;
            }
        } else {
            wanted = requested.get(&node).copied();
            if known.is_some() && self.nodes.get(&node).and_then(|entry| entry.requested) != wanted
            {
                self.tree.mark_dirty(id).map_err(taffy_error)?;
            }
        }
        self.nodes.insert(
            node,
            Entry {
                id,
                container,
                requested: wanted,
                seen: self.generation,
            },
        );
        Ok(id)
    }

    /// The Taffy nodes of a container's children that Taffy places.
    fn children(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        requested: &FastMap<NodeHandle, EngineSize>,
        moved: &mut bool,
    ) -> Result<Vec<NodeId>, LayoutError> {
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
            // A child on its way out is placed apart, where it was.
            if scene.bool_value(child, "visible")? && !scene.is_exiting(child) {
                children.push(self.visit(scene, child, requested, moved)?);
            }
        }
        Ok(children)
    }

    fn root_id(&self) -> Result<NodeId, LayoutError> {
        self.root_id
            .ok_or_else(|| LayoutError::Scene("flex tree used before it was built".into()))
    }

    /// Gives the root the box the engine resolved for it, so a Flex with no
    /// size of its own fills what its parent gave it rather than shrinking
    /// to its content; `None` leaves it its own style.
    pub(crate) fn set_root_size(&mut self, size: Option<(f64, f64)>) -> Result<(), LayoutError> {
        let root = self.root_id()?;
        let mut style = self.root_style.clone();
        if let Some((width, height)) = size {
            style.size = Size {
                width: Dimension::length(width as f32),
                height: Dimension::length(height as f32),
            };
        }
        if self.tree.style(root).map_err(taffy_error)? != &style {
            self.tree.set_style(root, style).map_err(taffy_error)?;
        }
        Ok(())
    }

    /// Lays the tree out in `available`, measuring leaves as the engine does.
    pub(crate) fn compute(
        &mut self,
        scene: &Scene,
        available: Size<AvailableSpace>,
        requested: &FastMap<NodeHandle, EngineSize>,
        text: &mut impl TextMeasurer,
    ) -> Result<(), LayoutError> {
        let root = self.root_id()?;
        let mut failure = None;
        let mut fresh = FastSet::default();
        let performed = &mut self.performed;
        self.tree
            .compute_layout_with_measure(
                root,
                available,
                |input: LayoutInput, _id, context: Option<&mut NodeHandle>, _style| {
                    let Some(&mut node) = context else {
                        return LayoutOutput::HIDDEN;
                    };
                    let asked = (input.known_dimensions, input.available_space);
                    if input.run_mode == RunMode::PerformLayout {
                        performed.insert(node, asked);
                        fresh.insert(node);
                    }
                    match measure_leaf(scene, node, asked, requested, text) {
                        Ok(size) => LayoutOutput::from_outer_size(size),
                        Err(error) => {
                            failure.get_or_insert(error);
                            LayoutOutput::HIDDEN
                        }
                    }
                },
            )
            .map_err(taffy_error)?;
        if let Some(error) = failure {
            return Err(error);
        }
        self.replay(scene, &fresh, requested, text)
    }

    /// Asks the text measurer again, for every text leaf Taffy took from
    /// its cache, what the last layout that placed it asked: the text
    /// system draws what it shaped last, which must be what a tree built
    /// afresh would have left it with.
    fn replay(
        &self,
        scene: &Scene,
        fresh: &FastSet<NodeHandle>,
        requested: &FastMap<NodeHandle, EngineSize>,
        text: &mut impl TextMeasurer,
    ) -> Result<(), LayoutError> {
        for (node, asked) in &self.performed {
            let live = self
                .nodes
                .get(node)
                .is_some_and(|entry| entry.seen == self.generation && !entry.container);
            if fresh.contains(node)
                || !live
                || !matches!(scene.element(*node)?, Element::Text | Element::TextInput)
            {
                continue;
            }
            measure_leaf(scene, *node, *asked, requested, text)?;
        }
        Ok(())
    }

    /// The root's laid-out size.
    pub(crate) fn size(&self) -> Result<EngineSize, LayoutError> {
        let layout = self.tree.unrounded_layout(self.root_id()?);
        Ok(EngineSize {
            width: f64::from(layout.size.width),
            height: f64::from(layout.size.height),
        })
    }

    /// Every node's geometry, absolute within the root at `origin`, in
    /// tree order, with whether it is a leaf whose own children the engine
    /// still has to place.
    pub(crate) fn geometries(&self, origin: Geometry) -> Result<Vec<Flexed>, LayoutError> {
        let mut out = Vec::with_capacity(self.ids.len());
        self.collect(self.root_id()?, origin.x, origin.y, &mut out)?;
        Ok(out)
    }

    fn collect(
        &self,
        id: NodeId,
        x: f64,
        y: f64,
        out: &mut Vec<Flexed>,
    ) -> Result<(), LayoutError> {
        for child in self.tree.child_ids(id) {
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
            let leaf = !self.nodes.get(&node).is_some_and(|entry| entry.container);
            out.push((node, geometry, leaf, location));
            if !leaf {
                self.collect(child, geometry.x, geometry.y, out)?;
            }
        }
        Ok(())
    }
}

fn taffy_error(error: taffy::TaffyError) -> LayoutError {
    LayoutError::Scene(error.to_string())
}

/// What a leaf answers when Taffy asks how big it is.
///
/// Text is shaped at the width on offer, so it wraps where the container
/// says; anything else is the size it asked for or was measured at.
fn measure_leaf(
    scene: &Scene,
    node: NodeHandle,
    (known, available): Asked,
    requested: &FastMap<NodeHandle, EngineSize>,
    text: &mut impl TextMeasurer,
) -> Result<Size<f32>, LayoutError> {
    if scene.element(node)? == Element::Text {
        let offered = known.width.or(match available.width {
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
                alignment: text_alignment(scene.directed_alignment(node)?)?,
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
        let offered = known.width.or(match available.width {
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
