//! `morf.windows` — every window the compositor reports.
//!
//! Kept beside the screen list rather than folded into it because they answer
//! different questions and change at different rates: outputs are a handful of
//! things that move when hardware does, and windows are dozens of things that
//! move when a person does.

use luna::{Table, Value as LuaValue};

use crate::{
    api::desktop::toplevels::*, api::ui::bindings::flush_reactive, runtime::host_types::*,
    vm::types::*,
};

impl Runtime {
    /// Replaces `morf.windows` with the compositor's current window list.
    ///
    /// Updated in place, so a configuration that captured `morf.windows` keeps
    /// seeing the live list — the same contract `morf.screens` has, and for the
    /// same reason: a configuration should be able to hold the list and watch
    /// it rather than having to ask for it again.
    ///
    /// Each entry carries `identifier`, `title` and `app_id`. The identifier is
    /// the one to key on: titles change while a person reads them, and two
    /// windows of one application share an app id.
    ///
    /// Then `morf.toplevels` follows: its model is reconciled by identifier,
    /// its revision signal moves (re-running every binding that read the
    /// list), and its `on_changed` handlers hear what opened, closed and
    /// changed. A list that says nothing new moves none of them.
    pub fn set_windows(&mut self, windows: &[Toplevel]) {
        self.fill_windows_table(windows);
        self.follow_toplevels(windows);
    }

    fn follow_toplevels(&mut self, windows: &[Toplevel]) {
        let (change, listeners) = {
            let mut guard = self.reactive.borrow_mut();
            let (Some(host), engine) = guard.toplevels_and_engine() else {
                return;
            };
            let change = ToplevelChange::between(&host.windows, windows);
            let reordered = change.is_empty() && host.windows != windows;
            if change.is_empty() && !reordered {
                return;
            }
            host.windows = windows.to_vec();
            host.model.borrow_mut().reconcile(
                windows.iter().map(toplevel_row).collect(),
                Some("identifier"),
            );
            // A list nothing draws would keep its change journal forever;
            // one a view follows is drained by the view.
            let followed = engine
                .views()
                .values()
                .any(|view| std::rc::Rc::ptr_eq(&view.model, &host.model));
            if !followed {
                host.model.borrow_mut().take_changes();
            }
            host.revisions += 1;
            let (revision, value) = (host.revision, IpcValue::Integer(host.revisions));
            let listeners = if change.is_empty() {
                Vec::new()
            } else {
                host.listeners
                    .iter()
                    .map(|(_, callback)| callback.clone())
                    .collect::<Vec<_>>()
            };
            engine.reactive_mut().try_write(revision, value);
            (change.to_scene(), listeners)
        };
        if let Err(message) = self
            .lua
            .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
        {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("toplevels: {message}"));
        }
        for callback in listeners {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_toplevel_handler(ctx, &callback, &change, limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("toplevels handler: {message}"));
            }
        }
    }

    fn fill_windows_table(&mut self, windows: &[Toplevel]) {
        self.lua.enter(|ctx| {
            let Ok(morf) = ctx.get_global::<Table>("morf") else {
                return;
            };
            let LuaValue::Table(table) = morf.get_value(ctx, "windows") else {
                return;
            };
            // Cleared and refilled rather than diffed. The list is short, it is
            // rebuilt only when the compositor says something changed, and a
            // diff would have to answer what identity means for a window that
            // was renamed — which is exactly the question the identifier exists
            // to stop anybody asking.
            let previous = table.length(&ctx);
            for index in 1..=previous {
                let _ = table.set(ctx, index, LuaValue::Nil);
            }
            for (index, window) in windows.iter().enumerate() {
                let entry = Table::new(&ctx);
                entry.set_field(
                    ctx,
                    "identifier",
                    LuaValue::String(ctx.intern(window.identifier.as_bytes())),
                );
                entry.set_field(
                    ctx,
                    "title",
                    LuaValue::String(ctx.intern(window.title.as_bytes())),
                );
                entry.set_field(
                    ctx,
                    "app_id",
                    LuaValue::String(ctx.intern(window.app_id.as_bytes())),
                );
                entry.set_field(ctx, "activated", window.activated);
                entry.set_field(ctx, "maximized", window.maximized);
                entry.set_field(ctx, "minimized", window.minimized);
                entry.set_field(ctx, "fullscreen", window.fullscreen);
                entry.set_field(ctx, "controllable", window.controllable);
                let outputs = luna::Table::new(&ctx);
                for (index, name) in window.outputs.iter().enumerate() {
                    let _ = outputs.set(ctx, index as i64 + 1, ctx.intern(name.as_bytes()));
                }
                entry.set_field(ctx, "outputs", outputs);
                if let Some(first) = window.outputs.first() {
                    entry.set_field(ctx, "output", ctx.intern(first.as_bytes()));
                }
                if let Some(parent) = &window.parent {
                    entry.set_field(ctx, "parent", ctx.intern(parent.as_bytes()));
                }
                let _ = table.set(ctx, index as i64 + 1, entry);
            }
        });
    }

    /// Takes what a configuration asked to do to other windows.
    pub fn take_toplevel_requests(&mut self) -> Vec<ToplevelRequest> {
        self.reactive
            .borrow_mut()
            .requests_mut()
            .take_toplevel_requests()
    }
}
