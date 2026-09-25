//! Bringing a layout up to date by redoing only what moved.
//!
//! A frame in which one box grows -- a panel opening, a capsule morphing --
//! changes the geometry of that box, of the boxes it pushes, and of what is
//! inside it. The rest of the tree is where it was. A whole pass measures
//! and places every node regardless, text included; on a shell of a few
//! thousand nodes that is most of what such a frame costs.
//!
//! The scene stamps every node it changes with the layout revision of the
//! change, and every ancestor with the newest stamp beneath it (see
//! [`morf_scene::LayoutStamps`]). An update made from a layout computed at
//! revision `r` then:
//!
//! * measures again only nodes with a stamp newer than `r` somewhere in
//!   their subtree, taking every other node's size from the last layout;
//!   and stops climbing as soon as a node's size is worked out from nothing
//!   that moved -- its own properties unchanged and every child asking for
//!   what it asked for before;
//! * places again every child of a node it places, which is cheap, but goes
//!   down into a child only when something under it moved or the child
//!   itself landed somewhere else. A subtree that is untouched and sits
//!   exactly where it did keeps its geometry whole.
//!
//! A node that joined the tree since `r` is new to the old layout, so its
//! subtree is done in full. A node that left the tree is harder -- the old
//! layout still holds its geometry and nothing says which entries are dead
//! -- so a tree that lost a node since `r` gets a whole pass instead.
//!
//! The answer is the same as a whole pass's, to the bit: the same
//! arithmetic runs on the same inputs, and only work whose inputs are
//! provably unchanged is skipped. The randomized tests in
//! `tests/incremental.rs` hold it to that.

use morf_scene::{FastMap, FastSet, NodeHandle, Scene};

use crate::custom::{CustomLayout, NoCustom};
use crate::flex_style::is_flex_root;
use crate::geometry::{Geometry, Size, TextMeasurer};
use crate::helpers::LayoutError;
use crate::layout::Layout;

/// What a layout was computed from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Basis {
    pub(crate) root: NodeHandle,
    pub(crate) available: Size,
    /// [`Scene::layout_revision`] when it was computed.
    pub(crate) revision: u64,
}

/// How a node's position was worked out from its parent's.
///
/// Kept so that a subtree which moved without changing -- a panel inside a
/// capsule that is centred as it grows -- can follow its root by the same
/// additions, in the same order, that placed it: the result is the one a
/// whole pass computes, to the bit, without reading a property or asking
/// the text system anything.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Local {
    /// Placed by the engine's own rules: `x` inside the parent's box, which
    /// starts `inset` in from its edge (a clip's border), less the parent's
    /// scroll, plus the node's own `transition_x`.
    Placed {
        x: f64,
        y: f64,
        inset: Option<f64>,
        scrolled: Option<(f64, f64)>,
        transition: (f64, f64),
    },
    /// Placed by a `Custom` container's `place`.
    Custom {
        x: f64,
        y: f64,
        transition: (f64, f64),
    },
    /// Placed by Taffy, at `x` from the Taffy parent's position before its
    /// own transition.
    Flexed {
        x: f64,
        y: f64,
        transition: (f64, f64),
    },
}

impl Local {
    /// The node's position given its parent's resolved geometry; for a
    /// `Flexed` node, given its Taffy parent's position before transition.
    pub(crate) fn position(self, parent: Geometry) -> (f64, f64) {
        match self {
            Self::Placed {
                x,
                y,
                inset,
                scrolled,
                transition,
            } => {
                let (mut left, mut top) = (parent.x, parent.y);
                if let Some(border) = inset {
                    left += border;
                    top += border;
                }
                let (mut x, mut y) = (x + left, y + top);
                if let Some((content_x, content_y)) = scrolled {
                    x -= content_x;
                    y -= content_y;
                }
                (x + transition.0, y + transition.1)
            }
            Self::Custom { x, y, transition } => {
                (parent.x + x + transition.0, parent.y + y + transition.1)
            }
            Self::Flexed { x, y, transition } => {
                (parent.x + x + transition.0, parent.y + y + transition.1)
            }
        }
    }
}

/// Which nodes one incremental pass has to look at again.
#[derive(Clone, Debug)]
pub(crate) struct Dirty {
    /// Anything stamped after this has moved.
    since: u64,
    /// Doing a subtree in full: everything counts as moved.
    forced: bool,
    /// Nodes moved for a reason the scene does not know about: text whose
    /// width to measure at changed between passes.
    extra: FastSet<NodeHandle>,
    /// Those nodes and their ancestors.
    extra_path: FastSet<NodeHandle>,
}

impl Dirty {
    fn new(
        scene: &Scene,
        since: u64,
        extra: impl IntoIterator<Item = NodeHandle>,
    ) -> Result<Self, LayoutError> {
        let extra: FastSet<NodeHandle> = extra.into_iter().collect();
        let mut extra_path = FastSet::default();
        for &node in &extra {
            let mut current = Some(node);
            while let Some(node) = current {
                if !extra_path.insert(node) {
                    break;
                }
                current = scene.parent(node)?;
            }
        }
        Ok(Self {
            since,
            forced: false,
            extra,
            extra_path,
        })
    }
}

/// What a pass needs to do about one node.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Status {
    /// Something in its subtree moved, itself included.
    pub(crate) visit: bool,
    /// Something layout reads on the node itself moved.
    pub(crate) own: bool,
    /// It joined the tree since: do its subtree in full.
    pub(crate) fresh: bool,
}

const EVERYTHING: Status = Status {
    visit: true,
    own: true,
    fresh: false,
};

impl Layout {
    pub(crate) fn status(&self, scene: &Scene, node: NodeHandle) -> Result<Status, LayoutError> {
        let Some(dirty) = &self.dirty else {
            return Ok(EVERYTHING);
        };
        if dirty.forced {
            return Ok(EVERYTHING);
        }
        let stamps = scene.layout_stamps(node)?;
        let fresh = stamps.attached > dirty.since;
        let extra = dirty.extra.contains(&node);
        Ok(Status {
            visit: fresh || stamps.subtree > dirty.since || dirty.extra_path.contains(&node),
            own: fresh || extra || stamps.own > dirty.since,
            fresh,
        })
    }

    /// Moves everything under `node`, which has just been given `geometry`,
    /// along with it: nothing under it changed, and it kept its size, so
    /// each descendant is where its [`Local`] puts it relative to its
    /// parent's new place.
    pub(crate) fn move_children(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        geometry: Geometry,
    ) -> Result<(), LayoutError> {
        if is_flex_root(scene, node)? {
            return self.move_flexed(scene, node, geometry);
        }
        for &child in scene.children(node)? {
            let Some(local) = self.local.get(&child).copied() else {
                continue;
            };
            let Some(placed) = self.move_one(child, local, geometry) else {
                continue;
            };
            self.move_children(scene, child, placed)?;
        }
        Ok(())
    }

    /// [`Layout::move_children`] for a flex container: its children chain
    /// from `origin`, and a flex container inside it passes its children
    /// its own position before its transition, as Taffy's placement does.
    fn move_flexed(
        &mut self,
        scene: &Scene,
        container: NodeHandle,
        origin: Geometry,
    ) -> Result<(), LayoutError> {
        for &child in scene.children(container)? {
            let Some(local @ Local::Flexed { x, y, .. }) = self.local.get(&child).copied() else {
                continue;
            };
            let Some(placed) = self.move_one(child, local, origin) else {
                continue;
            };
            if is_flex_root(scene, child)? {
                let before = Geometry {
                    x: origin.x + x,
                    y: origin.y + y,
                    ..placed
                };
                self.move_flexed(scene, child, before)?;
            } else {
                self.move_children(scene, child, placed)?;
            }
        }
        Ok(())
    }

    fn move_one(&mut self, node: NodeHandle, local: Local, parent: Geometry) -> Option<Geometry> {
        let (x, y) = local.position(parent);
        let geometry = self.geometry.get_mut(&node)?;
        geometry.x = x;
        geometry.y = y;
        Some(*geometry)
    }

    /// Whether everything counts as moved already: a whole pass, or a
    /// subtree being done in full.
    pub(crate) fn forcing(&self) -> bool {
        self.dirty.as_ref().is_none_or(|dirty| dirty.forced)
    }

    pub(crate) fn force(&mut self, on: bool) {
        if let Some(dirty) = &mut self.dirty {
            dirty.forced = on;
        }
    }

    /// Brings the layout up to date with the scene, laid out at `root` in
    /// `available`, redoing only what moved since it was computed.
    ///
    /// The result is exactly what [`Layout::compute`] would return. A layout
    /// that was not computed for this root -- or was never computed at all,
    /// like [`Layout::default`] -- is computed whole.
    pub fn update(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
    ) -> Result<(), LayoutError> {
        self.update_with(scene, root, available, text, &mut NoCustom)
    }

    /// [`Layout::update`], with `host` answering for `Custom` containers.
    pub fn update_with(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let revision = scene.layout_revision();
        // Before anything is thrown away or overwritten: where each node that
        // has started to leave was, so it stays there.
        self.capture_exit_frames(scene);
        let basis = self.basis.filter(|basis| {
            basis.root == root
                && scene.contains(root)
                && scene.layout_detached_revision(root) <= basis.revision
        });
        let Some(basis) = basis else {
            *self = Self::compute_with(scene, root, available, text, host)?;
            return Ok(());
        };
        if basis.revision == revision && basis.available == available {
            return Ok(());
        }
        let updated = self.update_passes(scene, root, available, basis.revision, text, host);
        match updated {
            Ok(()) => {
                self.basis = Some(Basis {
                    root,
                    available,
                    revision,
                });
                Ok(())
            }
            Err(error) => {
                // Half updated is not a layout; the next update starts over.
                *self = Self::default();
                Err(error)
            }
        }
    }

    fn update_passes(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        since: u64,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        // Two states, as a whole pass has: the first pass's, measuring text
        // at its own width, and the second's, at the width it was placed at.
        // Without a second pass last time, this layout is the first's.
        let had_second = self.first.is_some();
        let mut first = match self.first.take() {
            Some(first) => *first,
            None => std::mem::take(self),
        };
        first.incremental_pass(
            scene,
            root,
            available,
            Dirty::new(scene, since, [])?,
            text,
            host,
        )?;
        let constrained = first.texts_to_remeasure(scene)?;
        if constrained.is_empty() {
            // The text measured at a width last time is not any more. A whole
            // pass would have measured it at its own width last; the text
            // system keeps what it shaped last, and that is what is drawn.
            if had_second && !self.text_widths.is_empty() {
                let stale = self.text_widths.keys().copied().collect::<Vec<_>>();
                let dirty = Dirty::new(scene, scene.layout_revision(), stale)?;
                first.incremental_pass(scene, root, available, dirty, text, host)?;
            }
            *self = first;
            return Ok(());
        }
        let (mut second, dirty) = if had_second {
            // Moved since: what the scene says, and every text whose width
            // to measure at is not the one it was measured at last time.
            let changed = constrained
                .iter()
                .filter(|(node, width)| self.text_widths.get(node) != Some(width))
                .map(|(node, _)| *node)
                .chain(
                    self.text_widths
                        .keys()
                        .filter(|node| !constrained.contains_key(node))
                        .copied(),
                )
                .collect::<Vec<_>>();
            (std::mem::take(self), Dirty::new(scene, since, changed)?)
        } else {
            // No second pass to start from: the first, just brought up to
            // date, differs from one only in the text measured at a width.
            (
                first.clone(),
                Dirty::new(scene, scene.layout_revision(), constrained.keys().copied())?,
            )
        };
        second.text_widths = constrained;
        second.incremental_pass(scene, root, available, dirty, text, host)?;
        second.first = Some(Box::new(first));
        *self = second;
        Ok(())
    }

    fn incremental_pass(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        dirty: Dirty,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        self.dirty = Some(dirty);
        let passed = self.pass(scene, root, available, text, host);
        self.dirty = None;
        passed
    }

    /// Where this layout and `other` disagree, if anywhere: the first
    /// node whose geometry, implicit or requested size differs, or a node
    /// one has and the other does not. For tests and benchmarks holding an
    /// incremental layout to a whole pass.
    pub fn difference(&self, other: &Layout) -> Option<String> {
        fn compare<V: PartialEq + std::fmt::Debug>(
            what: &str,
            a: &FastMap<NodeHandle, V>,
            b: &FastMap<NodeHandle, V>,
        ) -> Option<String> {
            for (node, value) in a {
                match b.get(node) {
                    Some(other) if other == value => {}
                    other => return Some(format!("{what} of {node:?}: {value:?} vs {other:?}")),
                }
            }
            b.keys()
                .find(|node| !a.contains_key(node))
                .map(|node| format!("{what} of {node:?}: missing vs {:?}", b.get(node)))
        }
        compare("geometry", &self.geometry, &other.geometry)
            .or_else(|| compare("implicit size", &self.implicit, &other.implicit))
            .or_else(|| compare("requested size", &self.requested, &other.requested))
            .or_else(|| compare("text width", &self.text_widths, &other.text_widths))
    }
}
