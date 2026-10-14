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
}
