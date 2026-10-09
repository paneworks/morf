//! The layout of a scene: the output type, the two-pass entry points, and
//! placing a node -- measuring, placing children and exits live in `layout/`.

mod children;
mod exit;
mod measure;

use morf_scene::{FastMap, FastSet, NodeHandle, Scene};

use crate::incremental::{Basis, Dirty, Local};

use crate::custom::{CustomLayout, NoCustom};
use crate::geometry::{Geometry, Size, TextMeasurer};
use crate::helpers::LayoutError;

/// Complete layout output keyed by stable node handles.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub(crate) geometry: FastMap<NodeHandle, Geometry>,
    pub(crate) implicit: FastMap<NodeHandle, Size>,
    /// The size each node asked for, worked out once.
    ///
    /// Resolving it means reading five properties and parsing the attached
    /// layout map, and every node is asked at least twice in a pass — once
    /// while its parent measures, once while its parent places it, and again
    /// for a leaf inside a `Flex`. The answer cannot change between those, so
    /// it is worked out where the implicit size is and looked up thereafter.
    pub(crate) requested: FastMap<NodeHandle, Size>,
    /// Widths to measure text at on the second pass.
    ///
    /// A `Text` with no width of its own is measured unconstrained, then
    /// placed at whatever width its parent gives it -- an `Inset`, a fill, a
    /// stretch. If it wraps or elides, that is the width it should have
    /// been measured at: its height is different, and so is its parent's.
    /// The first pass records those widths here; the second measures with
    /// them. Two passes, never more.
    pub(crate) text_widths: FastMap<NodeHandle, f64>,
    /// Text that may need a second pass at its parent's resolved width.
    /// Updated when measuring changed text, so an animated gauge does not
    /// make every layout scan every node and reread every text style.
    pub(crate) reflow_text: FastSet<NodeHandle>,
    /// What this layout was computed from, so the next can start from it.
    pub(crate) basis: Option<Basis>,
    /// The first of the two passes, when a second one ran: an incremental
    /// update redoes both, each from its own last state.
    pub(crate) first: Option<Box<Layout>>,
    /// During a pass, which nodes it has to look at again; `None` for all.
    pub(crate) dirty: Option<Dirty>,
    /// How each node's position was worked out from its parent's, so a
    /// subtree that has only moved can be moved without being laid out.
    pub(crate) local: FastMap<NodeHandle, Local>,
}

/// Cached layout geometry used by native transform watchers.
#[derive(Debug, Default)]
pub struct TransformTracker {
    pub(crate) geometry: FastMap<NodeHandle, Geometry>,
}

/// Watches the geometry and transform chain between two scene nodes.
#[derive(Clone, Debug)]
pub struct TransformWatcher {
    pub(crate) a: NodeHandle,
    pub(crate) b: NodeHandle,
    pub(crate) common_parent: Option<NodeHandle>,
    pub(crate) signature: Option<u64>,
}

impl Layout {
    /// Resolves the layout rooted at `root` into the supplied surface area.
    ///
    /// Without a host for `Custom` containers: a scene that has one fails
    /// here, and a host that can run its functions uses `compute_with`.
    pub fn compute(
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
    ) -> Result<Self, LayoutError> {
        Self::compute_with(scene, root, available, text, &mut NoCustom)
    }

    /// Resolves the layout, with `host` answering for `Custom` containers.
    pub fn compute_with(
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<Self, LayoutError> {
        let revision = scene.layout_revision();
        let mut layout = Self::default();
        layout.pass(scene, root, available, text, host)?;
        let constrained = layout.texts_to_remeasure();
        if !constrained.is_empty() {
            let first = layout.clone();
            layout.text_widths = constrained;
            layout.geometry.clear();
            layout.implicit.clear();
            layout.requested.clear();
            layout.local.clear();
            layout.pass(scene, root, available, text, host)?;
            layout.first = Some(Box::new(first));
        }
        layout.basis = Some(Basis {
            root,
            available,
            revision,
        });
        Ok(layout)
    }

    pub(crate) fn pass(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        self.capture_exit_frames(scene);
        self.measure_implicit(scene, root, text, host)?;
        let geometry = Geometry {
            width: available.width,
            height: available.height,
            ..Geometry::default()
        };
        self.place(scene, root, geometry, text, host)
    }

    /// Fixes the box each node that has started to leave keeps, from where
    /// this layout last placed it, relative to its parent: before a pass
    /// overwrites either. A node this layout never placed is left for one
    /// that did, or for [`Layout::place_exiting`] to fix where it is now.
    /// Forgets every node that is no longer in the tree `root` heads --
    /// removed, or moved to another tree -- here and in the first pass's
    /// layout. What is left is a layout of nodes that are all still there,
    /// which an incremental pass can bring up to date: the parents they
    /// left were stamped as changed when they left.
    pub(crate) fn prune(&mut self, scene: &Scene, root: NodeHandle) {
        // Kept: a node still in this tree whose every ancestor up to the
        // root has geometry too -- as in a whole pass, where a hidden flex
        // child and all under it have none. A node moved under one keeps
        // no geometry from where it was.
        let mut kept: FastMap<NodeHandle, bool> = FastMap::default();
        let nodes: Vec<NodeHandle> = self.geometry.keys().copied().collect();
        for node in nodes {
            self.kept(scene, root, node, &mut kept);
        }
        let placed = |node: &NodeHandle| kept.get(node).copied().unwrap_or(false);
        self.geometry.retain(|node, _| placed(node));
        self.local.retain(|node, _| placed(node));
        // Measured is not placed: a size is kept for any node still here.
        let mut here: FastMap<NodeHandle, bool> = FastMap::default();
        let mut in_tree = |node: &NodeHandle| {
            *here
                .entry(*node)
                .or_insert_with(|| scene.root_of(*node) == Some(root))
        };
        self.implicit.retain(|node, _| in_tree(node));
        self.requested.retain(|node, _| in_tree(node));
        self.text_widths.retain(|node, _| in_tree(node));
        self.reflow_text.retain(|node| in_tree(node));
        if let Some(first) = self.first.as_mut() {
            first.prune(scene, root);
        }
    }

    fn kept(
        &self,
        scene: &Scene,
        root: NodeHandle,
        node: NodeHandle,
        memo: &mut FastMap<NodeHandle, bool>,
    ) -> bool {
        if let Some(known) = memo.get(&node) {
            return *known;
        }
        let answer = if !scene.contains(node) || !self.geometry.contains_key(&node) {
            false
        } else if node == root {
            true
        } else {
            match scene.parent(node).ok().flatten() {
                Some(parent) => self.kept(scene, root, parent, memo),
                None => false,
            }
        };
        memo.insert(node, answer);
        answer
    }

    /// Gives `node` its geometry and places what is under it -- unless
    /// nothing under it moved and it sits exactly where it did, when the
    /// last layout's answer for the whole subtree still holds.
    pub(crate) fn place(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        geometry: Geometry,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let before = self.geometry.insert(node, geometry);
        if scene.has_exits()
            && !scene.is_exiting(node)
            && let Some(parent) = scene.parent(node)?
            && let Some(around) = self.geometry.get(&parent)
        {
            scene.note_placed(
                node,
                [
                    geometry.x - around.x,
                    geometry.y - around.y,
                    geometry.width,
                    geometry.height,
                ],
            );
        }
        let status = self.status(scene, node)?;
        if !status.visit
            && let Some(before) = before
        {
            if before == geometry {
                return Ok(());
            }
            // Moved, not resized, with nothing under it changed: everything
            // under it moves with it, by the same arithmetic that put it
            // where it was.
            if before.width == geometry.width && before.height == geometry.height {
                return self.move_children(scene, node, geometry);
            }
        }
        let forcing = status.fresh && !self.forcing();
        if forcing {
            self.force(true);
        }
        let placed = self.resolve_children(scene, node, text, host);
        if forcing {
            self.force(false);
        }
        placed
    }
}
