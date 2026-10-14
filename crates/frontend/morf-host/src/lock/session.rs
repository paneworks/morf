//! What a held lock does with the events its compositor sends, whatever the
//! compositor is: the live one under ext-session-lock (`lock.rs`), or the
//! headless backend `morf check`, `morf render` and `morf test` run on.
//!
//! Drawing is the caller's: a live lock paints each surface on the GPU, a
//! headless one lays it out and paints only when asked.

use morf_app::{Backend, Event, Output};
use morf_lua::{Runtime, SessionLockState};
use morf_scene::NodeHandle;

use crate::host::surfaces::{PointerInput, animation_delta};
use crate::host::windows::{Kind, Windows};
use crate::input::cursor::CursorShapes;
use crate::input::keys::{KeyAction, dispatch_key_in_subtree, key_modifiers};
use crate::input::pointer::handle_pointer_event;
use crate::lock::outputs::{LockTrees, ensure_lock_tree, release_lock_tree};

/// One held lock: its trees, its surfaces and where its input goes.
pub struct LockSession {
    pub trees: LockTrees,
    /// Every output's lock surface: a window like the shell's others.
    pub outputs: Windows,
    /// The pointer, the fingers and which node each surface's keys go to: the
    /// same state, and the same routing, a layer surface has.
    pub input: PointerInput,
    /// The compositor said the session is hidden.
    pub locked: bool,
    unlock_pending: bool,
    last_frame: Option<u32>,
}

/// What an event left for the caller to do.
pub enum LockStep {
    /// Handled; whether something changed that wants a paint.
    Handled(bool),
    /// Output `index`'s lock surface was given this logical size.
    Configure {
        index: usize,
        width: u32,
        height: u32,
    },
    /// The compositor ended the lock, for this reason.
    Finished(String),
}

impl LockSession {
    /// Reads the configuration's trees; nothing is built until an output's
    /// lock surface is configured.
    pub fn new(runtime: &Runtime) -> Result<Self, String> {
        Ok(Self {
            trees: LockTrees::of(runtime)?,
            outputs: Windows::default(),
            input: PointerInput::default(),
            locked: false,
            unlock_pending: false,
            last_frame: None,
        })
    }

    /// Asks `client` for the lock. Asked for, not yet granted: the
    /// compositor says `locked` once every output shows a locked frame, and
    /// only then is the session hidden.
    pub fn request(runtime: &mut Runtime, client: &mut dyn Backend) -> Result<(), String> {
        client.lock()?;
        runtime.set_session_lock_state(SessionLockState::Pending);
        Ok(())
    }

    /// Gives the session back.
    pub fn unlock(
        &mut self,
        runtime: &mut Runtime,
        client: &mut dyn Backend,
    ) -> Result<(), String> {
        client.unlock()?;
        self.locked = false;
        runtime.set_session_lock_state(SessionLockState::Unlocked);
        Ok(())
    }

    /// Whether the lock is to be lifted now. The file lifts it the way it
    /// asked for it: by clearing `morf.surface.session_lock`. Which door was
    /// opened is the file's business, not the lock's.
    pub fn wants_unlock(&mut self, runtime: &mut Runtime) -> bool {
        self.unlock_pending |= runtime.take_session_unlock_request();
        self.unlock_pending |= !runtime.layer_surface_config().session_lock;
        self.locked && self.unlock_pending
    }

    /// The tree drawn on output `index`.
    pub fn root(&self, index: usize) -> Option<NodeHandle> {
        self.trees.root(&self.outputs, index)
    }

    /// Every lock surface's output index, in order.
    pub fn indices(&self) -> Vec<usize> {
        let mut indices = self.outputs.ids(Kind::Lock);
        indices.sort_unstable();
        indices.into_iter().map(|index| index as usize).collect()
    }

    /// Makes output `index`'s tree for a surface of `size` and records that
    /// size; returns the tree it draws.
    pub fn configure(
        &mut self,
        runtime: &mut Runtime,
        index: usize,
        screen: Option<Output>,
        size: (u32, u32),
    ) -> Result<NodeHandle, String> {
        ensure_lock_tree(runtime, self.trees, &mut self.outputs, index, screen, size)?;
        let root = self
            .root(index)
            .ok_or_else(|| "lock surface has no tree to draw".to_owned())?;
        let window = self
            .outputs
            .get_mut(Kind::Lock, index as u64)
            .ok_or_else(|| "lock surface has no window".to_owned())?;
        (window.width, window.height) = size;
        Ok(root)
    }

    /// Takes down every tree, as the lock goes.
    pub fn release(&mut self, runtime: &mut Runtime) {
        for window in self.outputs.values_mut() {
            release_lock_tree(runtime, window);
        }
        self.outputs.clear();
        self.input.reset();
    }

    /// Handles one event from the lock's compositor.
    pub fn handle(
        &mut self,
        runtime: &mut Runtime,
        cursor: &mut dyn CursorShapes,
        event: Event,
    ) -> Result<LockStep, String> {
        let event =
            match handle_pointer_event(runtime, cursor, &mut self.input, &self.outputs, event) {
                Ok(Ok(painted)) => return Ok(LockStep::Handled(painted)),
                Ok(Err(event)) => event,
                // A scene mid-change fails a hit test the way it fails a
                // layout; the event is dropped and the lock stays up.
                Err(error) => {
                    runtime.warn(format!("lock input: {error}"));
                    return Ok(LockStep::Handled(false));
                }
            };
        let repaint = match event {
            Event::SessionLocked => {
                self.locked = true;
                runtime.set_session_lock_state(SessionLockState::Locked)
            }
            Event::Screens(screens) => {
                // Locks have one runtime, outside the normal output-worker
                // supervisor. Keep its tracked screen list current too, so
                // Lua can move controls off an unplugged monitor.
                runtime.replace_screens(&crate::process::supervisor::lua_screens(&screens));
                true
            }
            Event::SessionLockConfigure {
                index,
                width,
                height,
            } => {
                return Ok(LockStep::Configure {
                    index,
                    width,
                    height,
                });
            }
            Event::SessionLockSurfaceRemoved { index } => {
                if let Some(mut gone) = self.outputs.remove_lock(index) {
                    release_lock_tree(runtime, &mut gone);
                }
                // The surfaces after it moved down one, and so did every
                // role the input state names.
                self.input.reset();
                false
            }
            Event::SessionLockFrame { time_ms, .. } => {
                let frame = runtime
                    .tick_frame_animations(animation_delta(self.last_frame, time_ms))
                    .map_err(|error| error.to_string())?;
                self.last_frame = frame.active.then_some(time_ms);
                frame.active || frame.changed > 0
            }
            Event::Key {
                surface,
                pressed,
                repeat,
                keysym,
                text,
                modifiers,
                ..
            } => {
                // The same routing every other surface gets: Tab traversal,
                // and focus remembered per surface, as a click set it.
                let Some(root) = self.trees.key_root(&self.outputs, surface) else {
                    return Ok(LockStep::Handled(false));
                };
                let mut focused = self.input.focused.get(&surface).copied();
                let repaint = dispatch_key_in_subtree(
                    runtime,
                    root,
                    &mut focused,
                    KeyAction::of(pressed, repeat),
                    keysym,
                    text.as_deref(),
                    key_modifiers(modifiers),
                );
                match focused {
                    Some(node) => self.input.focused.insert(surface, node),
                    None => self.input.focused.remove(&surface),
                };
                repaint
            }
            Event::SessionLockFinished => {
                // Told before the process goes, so a configuration can
                // say why: refused outright, or ended from outside.
                let (state, error) = if self.locked {
                    (SessionLockState::Unlocked, "compositor ended the session lock")
                } else {
                    (SessionLockState::Failed, "compositor refused the session lock")
                };
                runtime.set_session_lock_state(state);
                return Ok(LockStep::Finished(error.to_owned()));
            }
            Event::Clipboard { text } => runtime.dispatch_clipboard(text),
            Event::OfferRead { request_id, result } => {
                runtime.dispatch_offer_read(request_id, result)
            }
            Event::InputMethod(state) => runtime.dispatch_input_method(
                state.active,
                state.surrounding_text,
                state.cursor,
                state.anchor,
                state.serial,
            ),
            Event::TextInput(state) => runtime.dispatch_text_input(
                state.focused,
                state.preedit,
                state.preedit_begin,
                state.preedit_end,
                state.commit,
                state.delete_before,
                state.delete_after,
                state.serial,
            ),
            // A lock client has only lock surfaces: no popups, floating
            // windows or drags, in or out.
            Event::AuxScale { .. }
            | Event::ShortcutsInhibited { .. }
            | Event::KeyboardFocus { .. }
            | Event::SurfaceKeyboard { .. }
            | Event::SurfacePointer { .. }
            | Event::Configure { .. }
            | Event::Scale { .. }
            | Event::Frame { .. }
            // Taken above, by the pointer path every surface shares.
            | Event::PointerMotion { .. }
            | Event::PointerLeave { .. }
            | Event::PointerButton { .. }
            | Event::PointerAxis { .. }
            | Event::TouchDown { .. }
            | Event::TouchMotion { .. }
            | Event::TouchUp { .. }
            | Event::TouchCancel
            | Event::PopupConfigure { .. }
            | Event::PopupFrame { .. }
            | Event::PopupDone { .. }
            | Event::ToplevelConfigure { .. }
            | Event::ToplevelFrame { .. }
            | Event::ToplevelClose { .. }
            | Event::DragEnter { .. }
            | Event::DragMotion { .. }
            | Event::DragLeave { .. }
            | Event::Drop { .. }
            | Event::DragSourceEnded { .. }
            | Event::Closed { .. } => false,
        };
        Ok(LockStep::Handled(repaint))
    }

    /// The window of output `index`'s lock surface.
    pub fn window_mut(&mut self, index: usize) -> Option<&mut crate::host::surfaces::Window> {
        self.outputs.get_mut(Kind::Lock, index as u64)
    }
}
