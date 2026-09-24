//! What the compositor tells a popup or a floating window after it is made:
//! the size it was configured to, that someone asked to close it, and that
//! it went away.
//!
//! A configuration asks for a size and the compositor decides — a tiling
//! compositor fills a tile, a person drags an edge — so the size a window is
//! drawn at is only known once it has been configured. `win.width` and
//! `win.height` are that size, read reactively, so a binding that lays the
//! root out to the window follows every resize. `on_resize(w, h)` hears the
//! same change as a callback.
//!
//! The close button a compositor draws, or its keybinding, asks a toplevel
//! to close (`xdg_toplevel.close`); it does not close it. `on_close_requested`
//! hears the request, and the window is hidden unless the callback returns
//! `false` — a settings window with unsaved changes can ask first. Whatever
//! took a window off screen — that, `win:close()`, a parent hidden, a popup
//! dismissed — `on_closed` hears it once it is gone.

use luna::{Callback, CallbackReturn, Closure, Context, UserRef, Value as LuaValue};
use morf_reactive::SignalId;
use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    reactive_bindings::*, reactive_execute::*, scene_bindings::*, state::*, surface_types::*,
    types::*,
};

/// The three things a window hears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WindowEvent {
    Resized,
    CloseRequested,
    Closed,
}

impl WindowEvent {
    pub(crate) const ALL: [Self; 3] = [Self::Resized, Self::CloseRequested, Self::Closed];

    /// The method that sets it, and the constructor key.
    pub(crate) fn method(self) -> &'static str {
        match self {
            Self::Resized => "on_resize",
            Self::CloseRequested => "on_close_requested",
            Self::Closed => "on_closed",
        }
    }
}

/// One window's configured size and the signals its reads track.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowSize {
    pub(crate) width: SignalId,
    pub(crate) height: SignalId,
    pub(crate) size: (u32, u32),
}

/// The size a popup or floating window asked for, which is what it reads as
/// until the compositor configures it.
fn requested_size(kind: &WindowSurfaceKind) -> Option<(u32, u32)> {
    match kind {
        WindowSurfaceKind::Popup(config) => Some((config.width, config.height)),
        WindowSurfaceKind::Floating(config) => Some((config.width, config.height)),
        WindowSurfaceKind::Layer(_) => None,
    }
}

/// Gives a newly registered popup or floating window its two size signals.
///
/// Made with the window rather than on first read, because a first read is
/// usually inside a binding — while the graph is running and cannot make one.
pub(crate) fn register_window_size(state: &mut ReactiveState, id: u64) {
    let Some(size) = state
        .window_surfaces
        .get(&id)
        .and_then(|surface| requested_size(&surface.kind))
    else {
        return;
    };
    let Some(graph) = state.graph.as_mut() else {
        return;
    };
    let width = graph.signal(
        format!("window.{id}.width"),
        IpcValue::Integer(i64::from(size.0)),
    );
    let height = graph.signal(
        format!("window.{id}.height"),
        IpcValue::Integer(i64::from(size.1)),
    );
    state
        .values
        .insert(width, IpcValue::Integer(i64::from(size.0)));
    state
        .values
        .insert(height, IpcValue::Integer(i64::from(size.1)));
    state.signals.extend([width, height]);
    state.window_sizes.insert(
        id,
        WindowSize {
            width,
            height,
            size,
        },
    );
}

/// `win.width` / `win.height` on a popup or floating window: the configured
/// size, tracked by whatever binding reads it. `None` for anything else.
pub(crate) fn window_size_field<'gc>(
    state: &mut ReactiveState,
    id: u64,
    key: &str,
) -> Option<LuaValue<'gc>> {
    if !matches!(key, "width" | "height") {
        return None;
    }
    let requested = state
        .window_surfaces
        .get(&id)
        .and_then(|surface| requested_size(&surface.kind))?;
    let Some(size) = state.window_sizes.get(&id).copied() else {
        let value = if key == "width" {
            requested.0
        } else {
            requested.1
        };
        return Some(LuaValue::Integer(i64::from(value)));
    };
    let (signal, value) = if key == "width" {
        (size.width, size.size.0)
    } else {
        (size.height, size.size.1)
    };
    if let Some(active) = &mut state.active {
        active.reads.insert(signal);
    }
    Some(LuaValue::Integer(i64::from(value)))
}

/// `win:on_resize(fn)`, `win:on_close_requested(fn)`, `win:on_closed(fn)`:
/// sets the callback, or clears it given `nil`.
pub(crate) fn window_handler_method<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    event: WindowEvent,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (surface, callback): (UserRef<WindowSurfaceToken>, Option<Closure>) =
            stack.consume(ctx)?;
        let mut state = state.borrow_mut();
        let Some(window) = state.window_surfaces.get(&surface.id) else {
            return Err(HostError("window surface is stale".into()).into());
        };
        if event == WindowEvent::CloseRequested
            && !matches!(window.kind, WindowSurfaceKind::Floating(_))
        {
            return Err(HostError(format!(
                "{} is only valid for floating windows",
                event.method()
            ))
            .into());
        }
        match callback {
            Some(callback) => {
                state
                    .window_handlers
                    .insert((surface.id, event), ctx.stash(callback));
            }
            None => {
                state.window_handlers.remove(&(surface.id, event));
            }
        }
        Ok(CallbackReturn::Return)
    })
}

/// The window callbacks a `window.popup { ... }` or `window.floating { ... }`
/// table may carry, set as the methods would set them.
pub(crate) fn window_handlers_from_options<'gc>(
    ctx: Context<'gc>,
    state: &mut ReactiveState,
    id: u64,
    options: luna::Table<'gc>,
) -> Result<(), HostError> {
    for event in WindowEvent::ALL {
        match options.get_value(ctx, event.method()) {
            LuaValue::Nil => {}
            LuaValue::Function(luna::Function::Closure(callback)) => {
                let floating = state
                    .window_surfaces
                    .get(&id)
                    .is_some_and(|window| matches!(window.kind, WindowSurfaceKind::Floating(_)));
                if event == WindowEvent::CloseRequested && !floating {
                    return Err(HostError(format!(
                        "{} is only valid for floating windows",
                        event.method()
                    )));
                }
                state
                    .window_handlers
                    .insert((id, event), ctx.stash(callback));
            }
            _ => {
                return Err(HostError(format!(
                    "window `{}` must be a Lua function",
                    event.method()
                )));
            }
        }
    }
    Ok(())
}

impl Runtime {
    /// Records the size the compositor configured a popup or floating window
    /// to: `win.width`/`win.height` follow it and `on_resize(width, height)`
    /// runs. Returns whether anything changed, so the caller lays the window
    /// out afresh with the bindings that read it.
    pub fn set_window_surface_size(&mut self, id: u64, width: u32, height: u32) -> bool {
        let changed = {
            let mut state = self.reactive.borrow_mut();
            let Some(size) = state.window_sizes.get_mut(&id) else {
                return false;
            };
            if size.size == (width, height) {
                return false;
            }
            size.size = (width, height);
            let size = *size;
            let mut changed = false;
            for (signal, value) in [(size.width, width), (size.height, height)] {
                let value = IpcValue::Integer(i64::from(value));
                if state.values.get(&signal) == Some(&value) {
                    continue;
                }
                let Some(graph) = state.graph.as_mut() else {
                    continue;
                };
                if graph.write(signal, value.clone()).is_ok() {
                    state.values.insert(signal, value);
                    changed = true;
                }
            }
            changed
        };
        if changed
            && let Err(message) = self
                .lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("window resize: {message}"));
        }
        self.run_window_handler(
            id,
            WindowEvent::Resized,
            &[
                IpcValue::Integer(i64::from(width)),
                IpcValue::Integer(i64::from(height)),
            ],
        );
        true
    }

    /// The compositor asked a floating window to close — its close button,
    /// its keybinding. Runs `on_close_requested`, and hides the window
    /// unless that returned `false`. Returns whether it was hidden.
    pub fn request_window_close(&mut self, id: u64) -> bool {
        let keep = self
            .run_window_handler(id, WindowEvent::CloseRequested, &[])
            .is_some_and(|values| values.first() == Some(&IpcValue::Boolean(false)));
        !keep && self.set_window_surface_visible(id, false)
    }

    /// A popup or floating window went off screen, whatever took it: runs its
    /// `on_closed`. Returns whether there was one to run.
    pub fn dispatch_window_closed(&mut self, id: u64) -> bool {
        self.run_window_handler(id, WindowEvent::Closed, &[])
            .is_some()
    }

    /// Runs one window callback, returning what it returned, or `None` when
    /// there was none (or it failed, which is logged).
    fn run_window_handler(
        &mut self,
        id: u64,
        event: WindowEvent,
        args: &[IpcValue],
    ) -> Option<Vec<IpcValue>> {
        let handler = self
            .reactive
            .borrow()
            .window_handlers
            .get(&(id, event))
            .cloned()?;
        match self.run_handler(|ctx, limits| execute_ipc_handler(ctx, &handler, args, limits)) {
            Ok(values) => Some(values),
            Err(message) => {
                self.reactive.borrow_mut().log(
                    LogLevel::Warn,
                    format!("window {id} {}: {message}", event.method()),
                );
                None
            }
        }
    }
}
