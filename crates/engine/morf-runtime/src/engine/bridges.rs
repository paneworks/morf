//! What reaches two subsystems at once, so a caller never takes the
//! engine apart to lend one to the other.

use std::cell::RefCell;
use std::rc::Rc;

use morf_scene::ListModel;
use morf_scene::reactive::SignalId;

use super::Engine;

impl Engine {
    /// The revision signal a binding reading `model` depends on, made on
    /// first read.
    pub fn model_signal(&mut self, model: &Rc<RefCell<ListModel>>) -> Result<SignalId, String> {
        self.model_revisions
            .signal_or_make(&mut self.reactive, model)
    }

    /// Publishes this copy's `morf.shared` writes and takes the others':
    /// whether a signal here changed, and whether this copy published.
    pub fn sync_shared(&mut self) -> (bool, bool) {
        self.shared.sync(&mut self.reactive)
    }

    /// This copy's signal for `morf.shared` value `name`, made on first ask.
    pub fn shared_signal(
        &mut self,
        name: String,
        initial: morf_value::IpcValue,
    ) -> Result<SignalId, String> {
        self.shared.register(&mut self.reactive, name, initial)
    }

    /// Writes the time ("HH:MM:SS") to the clocks whose grain turned over.
    /// Returns whether any did.
    pub fn update_clocks(&mut self, time: String) -> Result<bool, String> {
        self.clocks.update(&mut self.reactive, time)
    }

    /// Takes the nodes destroyed since the last frame, and drops their
    /// transforms.
    pub fn take_removed_nodes(&mut self) -> Vec<morf_scene::NodeHandle> {
        let removed = self.scene.take_removed_nodes();
        if !removed.is_empty() {
            self.transform_tracker.retain_scene(&self.scene);
        }
        removed
    }

    /// Moves each popup anchored to a node to where that node was laid
    /// out, and marks the window surfaces changed when one moved.
    pub fn place_popup_anchors(&mut self) -> bool {
        let moved = crate::layout::place_popup_anchors(
            &self.windows.popup_node_anchors,
            &self.transform_tracker,
            &mut self.windows.window_surfaces,
        );
        if moved {
            self.windows.window_surfaces_changed = true;
        }
        moved
    }
}
