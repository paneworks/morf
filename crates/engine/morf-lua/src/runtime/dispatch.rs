//! The scene a configuration built, how its effects have fared, and handing
//! a UI event to a node's handler.

use morf_scene::{NodeHandle, Scene};

use crate::runtime::state::{StateRef, StateRefMut};
use crate::{runtime::events::*, runtime::host_types::*, vm::types::*};

impl Runtime {
    /// Borrows the scene produced by executed configuration code.
    pub fn scene(&self) -> StateRef<'_, Scene> {
        StateRef::map(self.reactive.borrow(), |state| state.engine().scene())
    }

    /// Mutably borrows the scene for frame-pipeline structural operations.
    pub fn scene_mut(&mut self) -> StateRefMut<'_, Scene> {
        let mut state = self.reactive.borrow_mut();
        // Native writes must invalidate the same service definitions as Lua
        // writes (for example, a Timer started through the Scene API).
        state.revisions_mut().bump_scene();
        StateRefMut::map(state, |state| state.engine_mut().scene_mut())
    }

    /// Drains parent transitions queued by Lua handlers.
    pub fn take_parent_transitions(&mut self) -> Vec<ParentTransitionRequest> {
        self.reactive
            .borrow_mut()
            .windows_mut()
            .take_parent_transitions()
    }

    /// Returns the number of Lua effect evaluations performed by this runtime.
    pub fn effect_runs(&self) -> u64 {
        self.reactive.borrow().tracking().effect_runs()
    }

    /// How many errors this runtime's effects have raised, repeats included.
    pub fn effect_errors(&self) -> u64 {
        self.reactive.borrow().reactive().errors().total()
    }

    /// Runs one bounded Lua UI handler and retains failures as runtime logs.
    pub fn dispatch_ui_event(&mut self, node: NodeHandle, event: UiEvent) -> bool {
        self.dispatch_ui_event_with_args(node, event, &[])
    }

    /// Tells `node`'s handler for `event`; see [`morf_runtime::events::deliver`].
    pub(crate) fn dispatch_ui_event_with_args(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        args: &[IpcValue],
    ) -> bool {
        deliver(self, node, event, args)
    }
}
