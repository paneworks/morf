//! Which handler each node has for each event, and the nodes something has
//! asked `contains_pointer` of: the event state one runtime holds.

use std::collections::{HashMap, HashSet};

use morf_scene::NodeHandle;

use super::{PointerWatch, UiEvent};
use crate::Handler;

#[derive(Default)]
pub struct Events {
    handlers: HashMap<(NodeHandle, UiEvent), Handler>,
    pub pointer_watch: PointerWatch,
}

impl Events {
    /// Sets `node`'s handler for `event`, or takes it away with `None`.
    pub fn set(&mut self, node: NodeHandle, event: UiEvent, handler: Option<Handler>) {
        match handler {
            Some(handler) => {
                self.handlers.insert((node, event), handler);
            }
            None => {
                self.handlers.remove(&(node, event));
            }
        }
    }

    pub fn handler(&self, node: NodeHandle, event: UiEvent) -> Option<Handler> {
        self.handlers.get(&(node, event)).cloned()
    }

    pub fn has(&self, node: NodeHandle, event: UiEvent) -> bool {
        self.handlers.contains_key(&(node, event))
    }

    /// Whether `node` has a handler for key presses or releases.
    pub fn handles_keys(&self, node: NodeHandle) -> bool {
        self.has(node, UiEvent::KeyPressed) || self.has(node, UiEvent::KeyReleased)
    }

    /// How many handlers there are, for `morf.debug`.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Forgets everything about nodes that are gone.
    pub fn forget(&mut self, removed: &HashSet<NodeHandle>) {
        self.handlers.retain(|(node, _), _| !removed.contains(node));
        for node in removed {
            self.pointer_watch.forget(*node);
        }
    }
}
