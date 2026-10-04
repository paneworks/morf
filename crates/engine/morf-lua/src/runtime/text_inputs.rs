//! Text inputs from Lua's side: the reactive state is what editing edits.
//! The editing itself -- caret, selection, undo, the input method, which
//! field has the keyboard -- is `morf_runtime::editing`'; this lets it
//! write a field's properties through the bindings that follow them, and
//! reach the clipboard and the compositor.

use morf_runtime::editing::{EditHost, Editing};
use morf_scene::{NodeHandle, Scene, Value as SceneValue};

use crate::scene_bindings::assign_scene_property;
use crate::state::ReactiveState;
use crate::{surface_types::*, types::LogLevel};

pub(crate) use morf_runtime::editing::{
    KeyOutcome, blink, drag, history, input_method_commit, insert, key, next_blink, observe_shaped,
    press, pull, reconcile_focus, register, release, select, select_all, selected_text, set_focus,
    tracked,
};
pub use morf_runtime::events::KeyModifiers;

impl EditHost for ReactiveState {
    fn scene(&self) -> &Scene {
        &self.scene
    }

    fn editing(&self) -> &Editing {
        &self.editing
    }

    fn editing_mut(&mut self) -> &mut Editing {
        &mut self.editing
    }

    fn assign(
        &mut self,
        node: NodeHandle,
        property: &str,
        value: SceneValue,
    ) -> Result<(), String> {
        assign_scene_property(self, node, property, value)
    }

    fn warn(&mut self, message: String) {
        self.log(LogLevel::Warn, message);
    }

    fn clipboard_text(&self) -> Option<String> {
        self.requests.clipboard_text.clone()
    }

    fn copy(&mut self, text: String) {
        self.requests.clipboard_text = Some(text.clone());
        self.requests.clipboard_requests.push(ClipboardRequest {
            data: text.into_bytes(),
            mime: None,
            primary: false,
        });
    }

    fn text_input(&mut self, request: TextInputRequest) {
        self.requests.text_input_requests.push(request);
    }

    fn enable_text_input(&mut self) {
        self.requests.text_input_enable_requested = true;
    }
}
