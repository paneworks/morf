use crate::{animation::*, motion::*, types::*};

impl Scene {
    /// Records that a property layout reads has moved on `node`.
    ///
    /// Conservative on purpose: a spurious bump costs one extra layout pass, a
    /// missed one leaves the scene drawn at stale geometry.
    pub(crate) fn touch_layout(&mut self, node: NodeId, property: &str) {
        if affects_layout(property) {
            self.bump_layout(node);
        }
    }

    /// Moves the layout revision, for the whole scene and for the tree
    /// `node` is in now, and stamps `node` as changed.
    ///
    /// A move between trees changes both, so whatever moves a node calls this
    /// once before and once after.
    ///
    /// The stamp goes on the node itself (`own`) and on every ancestor
    /// (`subtree`), in the one walk to the root the tree's revision needs
    /// anyway, so an incremental layout can find what moved by descending
    /// only into subtrees stamped since it was made.
    pub(crate) fn bump_layout(&mut self, node: NodeId) {
        self.layout_revision = self.layout_revision.wrapping_add(1);
        let revision = self.layout_revision;
        let Some(entry) = self.nodes.get_mut(node) else {
            return;
        };
        entry.stamps.own = revision;
        let mut current = node;
        loop {
            let entry = &mut self.nodes[current];
            entry.stamps.subtree = revision;
            match entry.parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        self.root_revisions.insert(current, revision);
    }

    /// Records that the tree `node` is in lost a node just now.
    pub(crate) fn mark_detached(&mut self, node: NodeId) {
        let root = self.root_id(node);
        self.detached_revisions.insert(root, self.layout_revision);
    }

    /// The root of the tree a node is in: the node itself when it has no
    /// parent.
    fn root_id(&self, mut node: NodeId) -> NodeId {
        while let Some(parent) = self.nodes.get(node).and_then(|node| node.parent) {
            node = parent;
        }
        node
    }

    /// When layout last had a reason to look at `node`; see [`LayoutStamps`].
    pub fn layout_stamps(&self, node: NodeHandle) -> Result<LayoutStamps, SceneError> {
        Ok(self.nodes[self.live(node)?].stamps)
    }

    /// The root of the tree `node` is in (the node itself when it has no
    /// parent), or `None` for a node no longer in the scene.
    pub fn root_of(&self, node: NodeHandle) -> Option<NodeHandle> {
        let id = self.live(node).ok()?;
        Some(NodeHandle(self.root_id(id)))
    }

    /// The layout revision at which the tree `root` is in last lost a node,
    /// removed or moved elsewhere; 0 if it never has.
    ///
    /// A layout of that tree made before this cannot be brought up to date
    /// piecemeal: it still holds the geometry of nodes no longer there.
    pub fn layout_detached_revision(&self, root: NodeHandle) -> u64 {
        let root = self.root_id(root.id());
        self.detached_revisions.get(&root).copied().unwrap_or(0)
    }

    /// How many times something layout reads has changed, anywhere.
    ///
    /// A paint that finds this unmoved since its last one may reuse that
    /// layout instead of computing another; one that lays out a single tree
    /// wants [`Scene::layout_revision_of`], which a change elsewhere does not
    /// move.
    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    /// The layout revision of the tree `root` is in: moved by every change
    /// layout reads inside that tree — a property, a node made, moved in or
    /// out, reordered or removed — and by nothing outside it.
    ///
    /// A surface laying out one root compares this with the one its cached
    /// layout was computed at. Given a node that is not a root, the answer
    /// is its whole tree's, which is never less careful.
    pub fn layout_revision_of(&self, root: NodeHandle) -> u64 {
        let root = self.root_id(root.id());
        self.root_revisions.get(&root).copied().unwrap_or(0)
    }
}

impl Scene {
    /// Points a property's target at wherever its motion actually stopped.
    ///
    /// Motion moves `current`; `target` is what the last write asked for, and
    /// the two part company whenever the motion did not land where it was
    /// aimed — an alternating repetition resting on its start value, or a
    /// fling, which is never aimed anywhere at all.
    ///
    /// Leaving them apart is not cosmetic. [`Scene::assign`] answers "is this
    /// property already what you are asking for" by reading `target` alone, so
    /// a stale target makes a later write to the pre-motion value a silent
    /// no-op: fling `y` away from zero, and `node.y = 0` afterwards does
    /// nothing at all. Every path that ends a motion has to come through here.
    pub(crate) fn settle_target(&mut self, key: PropertyKey) -> Result<(), SceneError> {
        let Some(node) = self.nodes.get(key.node) else {
            return Ok(());
        };
        let slot = node.properties[key.property];
        let settled = self.properties.read(slot.current)?.clone();
        if self.properties.read(slot.target)? != &settled {
            self.properties.write(slot.target, settled)?;
        }
        Ok(())
    }
}

impl Scene {
    /// Takes the nodes destroyed since this was last called.
    ///
    /// Everything holding state keyed on a node lives in another crate and
    /// cannot see one die. Whoever drives the frame drains this once and hands
    /// it to them; nobody else has both the scene and those caches in scope.
    pub fn take_removed_nodes(&mut self) -> Vec<NodeHandle> {
        std::mem::take(&mut self.removed)
    }

    /// Whether anything was destroyed since the list was last drained.
    pub fn has_removed_nodes(&self) -> bool {
        !self.removed.is_empty()
    }
}

impl Scene {
    /// Stops whatever is moving a property, and says so.
    ///
    /// Every path that replaces one kind of motion with another has to come
    /// through here. Two of the four used to do the removals inline and skip
    /// the event, so a configuration waiting on `on_finished` never heard from
    /// an animation that a `set_physics` had quietly thrown away.
    pub(crate) fn cancel_motion(&mut self, key: PropertyKey) {
        let stopped = self.animations.remove(&key).is_some() | self.physics.remove(&key).is_some();
        self.paused_physics.remove(&key);
        if stopped {
            self.push_event(key, AnimationEnd::Canceled);
        }
    }
}
