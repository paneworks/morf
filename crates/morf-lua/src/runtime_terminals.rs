//! The runtime's side of `ui.Terminal`: turns of the loop, frames, keys
//! and the pointer, each handed to [`crate::terminals`].

use morf_layout::Layout;
use morf_scene::{Element, NodeHandle};
use morf_terminal::{Modifiers, MouseAction};
use morf_text::TextSystem;

use crate::events::UiEvent;
use crate::reactive_execute::{execute_handler_args, execute_ipc_handler};
use crate::surface_types::IpcValue;
use crate::text_inputs::KeyModifiers;
use crate::types::*;

impl Runtime {
    /// Whether a node is a terminal.
    pub fn is_terminal(&self, node: NodeHandle) -> bool {
        self.reactive.borrow().scene.element(node).ok() == Some(Element::Terminal)
    }

    /// Feeds every terminal what its program wrote, and runs the callbacks
    /// that owes. Returns whether anything on screen changed.
    pub(crate) fn poll_terminals(&mut self) -> bool {
        let (calls, changed, more) = {
            let mut state = self.reactive.borrow_mut();
            crate::terminals::pump(&mut state)
        };
        for call in &calls {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, &call.callback, &call.args, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("Terminal callback: {message}"));
            }
        }
        if more {
            // This turn's share is fed; the rest is for the next turn, which
            // this makes come at once rather than at the next event.
            morf_io::wake_all();
        }
        self.flush_after_event();
        changed || !calls.is_empty()
    }

    /// Fits every laid-out terminal to its box, and starts the programs of
    /// terminals laid out for the first time. Called once a frame, after
    /// layout, with the text system that frame is painted with — so the grid
    /// is the one the glyphs will be drawn on.
    pub fn sync_terminals(&mut self, layout: &Layout, text: &mut TextSystem) -> bool {
        let changed = {
            let mut state = self.reactive.borrow_mut();
            if state.terminals.len() == 0 {
                return false;
            }
            crate::terminals::sync(&mut state, layout, text)
        };
        self.flush_after_event();
        changed
    }

    /// Reads what the paint that just happened made of every image laid out
    /// in `layout`: its status, and whether it moves and is on screen. Called
    /// once a frame, after the render, with the cache that render drew from.
    /// Returns whether anything changed.
    pub fn sync_images(&mut self, layout: &Layout, cache: &mut morf_image::ImageCache) -> bool {
        let calls = {
            let mut state = self.reactive.borrow_mut();
            if state.images.is_empty() {
                return false;
            }
            let revision = state.scene_revision;
            let calls = crate::images::sync(&mut state, layout, cache);
            if calls.is_empty() && state.scene_revision == revision {
                return false;
            }
            calls
        };
        for call in &calls {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_handler_args(ctx, &call.callback, &call.args, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("Image on_status: {message}"));
            }
        }
        self.flush_after_event();
        true
    }

    /// Moves playing pictures on by the clock; true when a frame changed.
    pub(crate) fn poll_images(&mut self) -> bool {
        let changed = {
            let mut state = self.reactive.borrow_mut();
            crate::images::advance(&mut state, std::time::Instant::now())
        };
        if changed {
            self.flush_after_event();
        }
        changed
    }

    pub(crate) fn terminal_key(
        &mut self,
        node: NodeHandle,
        keysym: u32,
        text: Option<&str>,
        modifiers: KeyModifiers,
    ) -> bool {
        let modifiers = Modifiers {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            logo: modifiers.logo,
        };
        let mut state = self.reactive.borrow_mut();
        crate::terminals::key(&mut state, node, keysym, text, modifiers)
    }

    /// Whether the terminal's `on_key_pressed` took the key (returned
    /// true) before its program saw it.
    pub(crate) fn terminal_key_claimed(&mut self, node: NodeHandle, args: &[IpcValue]) -> bool {
        let handler = self
            .reactive
            .borrow()
            .handlers
            .get(&(node, UiEvent::KeyPressed))
            .cloned();
        let Some(handler) = handler else {
            return false;
        };
        match self.run_handler(|ctx, limits| execute_ipc_handler(ctx, &handler, args, limits)) {
            Ok(values) => values.first() == Some(&IpcValue::Boolean(true)),
            Err(message) => {
                self.reactive.borrow_mut().log(
                    LogLevel::Warn,
                    format!("{node:?}.on_key_pressed: {message}"),
                );
                false
            }
        }
    }

    pub(crate) fn terminal_pointer(
        &mut self,
        node: NodeHandle,
        action: MouseAction,
        button: Option<u32>,
        local: (f64, f64),
    ) -> bool {
        let mut state = self.reactive.borrow_mut();
        crate::terminals::pointer(&mut state, node, action, button, local)
    }

    pub(crate) fn terminal_wheel(
        &mut self,
        node: NodeHandle,
        local: (f64, f64),
        pixels: f64,
        steps: i32,
    ) -> bool {
        let mut state = self.reactive.borrow_mut();
        crate::terminals::wheel(&mut state, node, local, pixels, steps)
    }
}
