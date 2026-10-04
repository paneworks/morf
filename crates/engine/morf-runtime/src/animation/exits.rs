//! Exits: a node on its way out plays its exit before it goes, held by
//! retention meanwhile.

use morf_scene::retain::Retention;
use morf_scene::{NodeHandle, Scene};

use super::Animation;

/// Starts `node`'s exit, if it has one. Returns whether it is now leaving
/// (also when it already was). The caller marks the scene changed when this
/// started one.
pub fn begin_exit(
    scene: &mut Scene,
    retention: &mut Retention<NodeHandle>,
    animation: &mut Animation,
    node: NodeHandle,
) -> ExitStart {
    if scene.is_exiting(node) {
        return ExitStart::Already;
    }
    if !matches!(scene.begin_exit(node), Ok(true)) {
        return ExitStart::None;
    }
    if retention.state(node).is_none() {
        retention.register(node);
        animation.exit_registered.insert(node);
    }
    let _ = retention.lock(node);
    let _ = retention.begin_drop(node);
    ExitStart::Started
}

/// What [`begin_exit`] did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitStart {
    /// The node has no exit: it goes at once.
    None,
    /// It was already leaving.
    Already,
    /// Its exit began now.
    Started,
}

impl ExitStart {
    pub fn leaving(self) -> bool {
        self != Self::None
    }
}

/// Takes back a node that was on its way out: it rejoins the flow and its
/// properties go back to where they were aimed. `false` if it was not leaving.
pub fn cancel_exit(
    scene: &mut Scene,
    retention: &mut Retention<NodeHandle>,
    animation: &mut Animation,
    node: NodeHandle,
) -> bool {
    if !scene.cancel_exit(node).unwrap_or(false) {
        return false;
    }
    if animation.exit_registered.remove(&node) {
        retention.unregister(node);
    } else {
        let _ = retention.unlock(node);
        let _ = retention.cancel_drop(node);
    }
    true
}

/// Lets go of the hold an ended exit had. Returns whether nothing else
/// holds the node, so it is removed now.
pub fn finish_exit(
    scene: &Scene,
    retention: &mut Retention<NodeHandle>,
    node: NodeHandle,
) -> Option<bool> {
    if !scene.contains(node) {
        return None;
    }
    let _ = retention.unlock(node);
    Some(retention.should_destroy(node).unwrap_or(true))
}
