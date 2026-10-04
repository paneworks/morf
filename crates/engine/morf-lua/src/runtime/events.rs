//! The events a node's handlers are told, and where each goes:
//! morf-runtime's. This side runs the handlers and writes what a pointer
//! area keeps.

pub use morf_runtime::events::*;

use morf_scene::{NodeHandle, Scene};

use crate::reactive_execute::{execute_handler_args, execute_ipc_handler};
use crate::scene_bindings::assign_scene_property;
use crate::types::LogLevel;
use crate::{IpcValue, Runtime};

impl EventHost for Runtime {
    fn with_events<R>(&self, read: impl FnOnce(&Scene, &Events) -> R) -> R {
        let state = self.reactive.borrow();
        read(&state.scene, &state.events)
    }

    fn assign_pointer_state(&mut self, node: NodeHandle, property: &str, value: bool) -> bool {
        let mut state = self.reactive.borrow_mut();
        assign_scene_property(&mut state, node, property, morf_scene::Value::Bool(value)).is_ok()
    }

    fn flush_after_event(&mut self) {
        Runtime::flush_after_event(self);
    }

    fn run_event_handler(
        &mut self,
        handler: &morf_runtime::Handler,
        args: &[IpcValue],
    ) -> Result<(), String> {
        self.run_handler(|ctx, limits| execute_handler_args(ctx, handler, args, limits))
    }

    fn run_key_handler(
        &mut self,
        handler: &morf_runtime::Handler,
        args: &[IpcValue],
    ) -> Result<Vec<IpcValue>, String> {
        self.run_handler(|ctx, limits| execute_ipc_handler(ctx, handler, args, limits))
    }

    fn press_key(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: KeyModifiers,
        repeat: bool,
    ) -> bool {
        self.dispatch_key_press(node, keysym, text, modifiers, repeat)
    }

    fn warn(&mut self, message: String) {
        self.reactive.borrow_mut().log(LogLevel::Warn, message);
    }
}
