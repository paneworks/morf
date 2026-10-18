//! How far the scene has moved, and how far each reader of it has seen.

pub struct Revisions {
    /// Moves on with every property write a binding may have read.
    property: i64,
    /// Advances whenever the scene actually changes: a property lands on a
    /// new value, or a node is created, reparented, or removed. This is what
    /// tells the host a repaint is due -- a service callback merely running
    /// is not.
    scene: u64,
    /// The scene's revision when the services were last polled: a scene that
    /// moved on since may hold a timer to start or a loader to fill.
    polled: u64,
    /// The revision seen before timers and loaders were reconciled, which can
    /// itself unload a tree that must preload again on the next turn.
    service_definitions: u64,
    /// How many of the scene's revisions were a property of a node nothing
    /// shows: work for the loop, not a reason to paint.
    hidden: u64,
}

/// Where the scene stood at one moment, to ask later what moved since.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    scene: u64,
    hidden: u64,
}

impl Default for Revisions {
    fn default() -> Self {
        Self {
            property: 0,
            scene: 0,
            polled: 0,
            service_definitions: u64::MAX,
            hidden: 0,
        }
    }
}

impl Revisions {
    /// The scene's revision.
    pub fn scene(&self) -> u64 {
        self.scene
    }

    /// The scene changed.
    pub fn bump_scene(&mut self) {
        self.advance_scene(1);
    }

    /// The scene changed `by` times.
    pub fn advance_scene(&mut self, by: u64) {
        self.scene = self.scene.wrapping_add(by);
    }

    /// The scene changed, in a node nothing shows.
    pub fn bump_hidden(&mut self) {
        self.hidden = self.hidden.wrapping_add(1);
    }

    /// The property revision, as a binding's signals carry it.
    pub fn property(&self) -> i64 {
        self.property
    }

    /// A property a binding may have read was written; the new revision.
    pub fn bump_property(&mut self) -> i64 {
        self.property = self.property.wrapping_add(1);
        self.property
    }

    /// Where the scene stands now.
    pub fn mark(&self) -> Mark {
        Mark {
            scene: self.scene,
            hidden: self.hidden,
        }
    }

    /// Whether the scene changed since `mark` in something shown.
    pub fn shown_since(&self, mark: Mark) -> bool {
        self.scene.wrapping_sub(mark.scene) > self.hidden.wrapping_sub(mark.hidden)
    }

    /// The services saw the scene as it stood at `mark`.
    pub fn polled_at(&mut self, mark: Mark) {
        self.polled = mark.scene;
    }

    /// Whether the scene moved since the services were reconciled with it;
    /// they are from now on.
    pub fn settle_services(&mut self) -> bool {
        let changed = self.scene != self.service_definitions;
        self.service_definitions = self.scene;
        changed
    }

    /// The services must be reconciled on the next poll.
    pub fn unsettle_services(&mut self) {
        self.service_definitions = self.scene.wrapping_sub(1);
    }

    /// Whether the scene moved since the services last saw it.
    pub fn unseen(&self) -> bool {
        self.scene != self.polled || self.scene != self.service_definitions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_change_is_not_shown() {
        let mut revisions = Revisions::default();
        let mark = revisions.mark();
        revisions.bump_scene();
        revisions.bump_hidden();
        assert!(!revisions.shown_since(mark));
        revisions.bump_scene();
        assert!(revisions.shown_since(mark));
    }

    #[test]
    fn the_services_settle_once() {
        let mut revisions = Revisions::default();
        assert!(revisions.unseen());
        assert!(revisions.settle_services());
        assert!(!revisions.settle_services());
        revisions.unsettle_services();
        assert!(revisions.settle_services());
    }
}
