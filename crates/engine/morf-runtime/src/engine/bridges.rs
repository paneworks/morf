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

    /// Where the session lock stands, as last recorded.
    pub fn session_lock_state(&self) -> crate::session::SessionLockState {
        self.session.lock_state(&self.reactive)
    }

    /// Records the session lock's new state: the value written, or `None`
    /// when nothing changed.
    pub fn set_session_lock_state(
        &mut self,
        next: crate::session::SessionLockState,
    ) -> Option<(morf_value::IpcValue, Result<(), String>)> {
        self.session.set_lock_state(&mut self.reactive, next)
    }

    /// Makes this runtime primary, or not: the value written, or `None`
    /// when nothing changed.
    pub fn set_primary(
        &mut self,
        primary: bool,
    ) -> Option<(morf_value::IpcValue, Result<(), String>)> {
        self.session.set_primary(&mut self.reactive, primary)
    }

    /// The output list is now `screens`. Returns whether the screens'
    /// revision moved.
    pub fn screens_changed(&mut self, screens: &[crate::screens::Screen]) -> bool {
        self.session.screens_changed(&mut self.reactive, screens)
    }

    /// Moves the screens' revision though the outputs are the same.
    pub fn touch_screens(&mut self) {
        self.session.touch_screens(&mut self.reactive);
    }

    /// The long press whose time came by `now`, on a node with a
    /// `LongPressed` handler: its node and where it was pressed.
    pub fn take_long_press(
        &mut self,
        now: std::time::Duration,
    ) -> Option<(morf_scene::NodeHandle, crate::events::EventPoint)> {
        let events = &self.events;
        self.gestures.take_long_press(now, |node| {
            events.has(node, crate::events::UiEvent::LongPressed)
        })
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
