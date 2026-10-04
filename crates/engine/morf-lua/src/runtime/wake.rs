//! When the shell's loop has to wake on its own, and why.
//!
//! Everything a thread produces -- a line from a child, a signal off the bus,
//! a decoded image -- rings the loop's alarm when it lands, so the loop needs
//! no timeout for any of that. What is left is time itself: a timer coming
//! due, a caret about to blink, a moving picture's next frame, a D-Bus call
//! that stops being waited for, the clock turning over. This is where the
//! runtime says which of those comes first, so an idle shell sleeps exactly
//! until then and not a moment sooner.

use std::time::Instant;

use crate::{Error, IpcValue, reactive_bindings::flush_reactive, types::Runtime};

pub use morf_runtime::wake::{ClockPrecision, DeadlineCause};

impl Runtime {
    /// Updates `morf.clock` ("HH:MM:SS"), and `morf.minute_clock` and
    /// `morf.hour_clock` with it, and recomputes what reads them.
    ///
    /// Only the grains that turned over are written, so a binding on the
    /// minute clock runs once a minute however often this is called. Returns
    /// whether the scene actually changed: a repaint of a shell that shows
    /// no time is pure cost.
    pub fn update_clock(&mut self, value: impl Into<String>) -> Result<bool, Error> {
        let (revision_before, hidden_before) = {
            let state = self.reactive.borrow();
            (state.scene_revision, state.hidden_revisions)
        };
        let value: String = value.into();
        // "HH:MM:SS" carries the coarser grains in its prefix; a value in
        // any other shape is written as it is and nothing is derived.
        let derived = value
            .get(..5)
            .filter(|_| value.len() == 8 && value.as_bytes()[2] == b':')
            .map(|minutes| (minutes.to_owned(), value[..2].to_owned()));
        let mut changed = false;
        {
            let mut state = self.reactive.borrow_mut();
            let mut writes = vec![(state.clock, value)];
            if let Some((minutes, hours)) = derived {
                writes.push((state.clock_minutes, minutes));
                writes.push((state.clock_hours, hours));
            }
            for (signal, text) in writes {
                let text = IpcValue::String(text);
                if state.reactive.values.get(&signal) == Some(&text) {
                    continue;
                }
                state
                    .reactive
                    .graph
                    .as_mut()
                    .ok_or_else(|| Error::Runtime("reactive graph is already running".to_owned()))?
                    .write(signal, text.clone())
                    .map_err(|error| Error::Runtime(error.to_string()))?;
                state.reactive.values.insert(signal, text);
                changed = true;
            }
        }
        if changed {
            self.lua
                .enter(|ctx| flush_reactive(&self.reactive, ctx, self.limits))
                .map_err(Error::Runtime)?;
        }
        let state = self.reactive.borrow();
        Ok(state.scene_revision.wrapping_sub(revision_before)
            > state.hidden_revisions.wrapping_sub(hidden_before))
    }

    /// The finest clock anything currently reads, or nothing when no binding
    /// shows the time: the grain the loop has to wake at for the clock.
    pub fn clock_precision(&self) -> Option<ClockPrecision> {
        let state = self.reactive.borrow();
        let graph = state.reactive.graph.as_ref()?;
        [
            (state.clock, ClockPrecision::Seconds),
            (state.clock_minutes, ClockPrecision::Minutes),
            (state.clock_hours, ClockPrecision::Hours),
        ]
        .into_iter()
        .find(|(signal, _)| graph.has_subscribers(*signal))
        .map(|(_, precision)| precision)
    }

    /// The earliest moment something in this runtime comes due on the wall
    /// clock, and what it is; nothing when only an event can bring work.
    ///
    /// The clock is not among them: [`Runtime::clock_precision`] says what
    /// grain it is read at, and the loop owns the wall-clock text.
    pub fn next_deadline(&self) -> Option<(Instant, DeadlineCause)> {
        let state = self.reactive.borrow();
        let timers = state
            .timers
            .next_wall_deadline()
            .map(|at| (at, DeadlineCause::Timer));
        let caret = crate::text_inputs::next_blink(&*state).map(|at| (at, DeadlineCause::Caret));
        let image = state.images.due().map(|at| (at, DeadlineCause::Image));
        let dbus = state
            .dbus_replies
            .iter()
            .filter_map(|entry| entry.reply.deadline())
            .min()
            .map(|at| (at, DeadlineCause::DbusTimeout));
        let terminal = state
            .terminals
            .next_deadline()
            .map(|at| (at, DeadlineCause::Terminal));
        let tray = state
            .status_notifiers
            .iter()
            .filter_map(|subscription| subscription.host.next_deadline())
            .min()
            .map(|at| (at, DeadlineCause::TrayRetry));
        // Due at once while the scene is still; moving, only once it has
        // waited as long as a preload waits for anything.
        let still = !state.scene.has_motion();
        let preload = state
            .preload_pending
            .values()
            .min()
            .map(|since| {
                if still {
                    *since
                } else {
                    *since + crate::runtime_services::PRELOAD_PATIENCE
                }
            })
            .map(|at| (at, DeadlineCause::Preload));
        let long_press =
            crate::gestures::long_press_due(&state).map(|at| (at, DeadlineCause::LongPress));
        morf_runtime::wake::earliest([
            timers, caret, image, dbus, terminal, tray, preload, long_press,
        ])
    }

    /// Whether the last turn left work for [`Runtime::poll_services`] that
    /// no thread will ring for: the scene changed since it last ran (a
    /// handler started a `ui.Timer`, activated a `Loader`), a model a view
    /// follows changed, a node waits to be torn down, a transform watcher
    /// owes its callback. The loop takes another turn at once for it rather
    /// than leaving it until something else wakes the shell.
    pub fn has_pending_work(&self) -> bool {
        let state = self.reactive.borrow();
        state.scene_revision != state.polled_revision
            || state.scene_revision != state.service_definitions_revision
            || !state.retained_destroy_queue.is_empty()
            || state
                .transform_watchers
                .values()
                .any(|watcher| watcher.pending && watcher.callback.is_some())
            || state
                .views
                .values()
                .any(|view| view.model.borrow().has_changes())
    }
}
