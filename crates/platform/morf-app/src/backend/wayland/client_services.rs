use rustix::event::{PollFd, PollFlags, poll};
use rustix::fd::BorrowedFd;
use rustix::time::Timespec;
use std::time::Duration;

use crate::backend::wayland::surface_types::*;

pub use crate::backend::Woke;

impl LayerClient {
    /// Blocks until at least one Wayland event is dispatched.
    pub fn blocking_dispatch(&mut self) -> Result<(), WaylandError> {
        self.queue
            .blocking_dispatch(&mut self.state)
            .map_err(|error| WaylandError(format!("Wayland dispatch failed: {error}")))?;
        self.connection
            .flush()
            .map_err(|error| WaylandError(format!("Wayland flush failed: {error}")))?;
        Ok(())
    }

    /// Dispatches Wayland events or returns when the timeout expires.
    pub fn dispatch_timeout(&mut self, timeout: Duration) -> Result<bool, WaylandError> {
        self.dispatch_timeout_or(timeout, None)
    }

    /// As [`Self::dispatch_timeout`], returning early too when `wake` becomes
    /// readable: the loop's own alarm, which a service thread rings when it
    /// has something for the loop to collect.
    pub fn dispatch_timeout_or(
        &mut self,
        timeout: Duration,
        wake: Option<BorrowedFd<'_>>,
    ) -> Result<bool, WaylandError> {
        self.wait_for(Some(timeout), wake)
            .map(|woke| matches!(woke, Woke::Queued | Woke::Compositor))
    }

    /// Sleeps until the compositor sends something, `wake` becomes readable,
    /// or `timeout` passes -- with no timeout, only the first two -- and
    /// dispatches what the compositor sent. Says which of them ended the
    /// sleep, for a loop that wants to know why it is awake.
    pub fn wait_for(
        &mut self,
        timeout: Option<Duration>,
        wake: Option<BorrowedFd<'_>>,
    ) -> Result<Woke, WaylandError> {
        if self
            .queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| WaylandError(format!("Wayland dispatch failed: {error}")))?
            > 0
        {
            return Ok(Woke::Queued);
        }
        self.queue
            .flush()
            .map_err(|error| WaylandError(format!("Wayland flush failed: {error}")))?;
        // A held key repeats on the client's clock: the wait ends when the
        // next repeat is due, and the repeats that are due are queued.
        if self.fire_key_repeats() {
            return Ok(Woke::Queued);
        }
        let timeout = match self.state.key_repeat.deadline() {
            Some(due) => {
                let until = due.saturating_duration_since(std::time::Instant::now());
                Some(timeout.map_or(until, |timeout| timeout.min(until)))
            }
            None => timeout,
        };
        let Some(guard) = self.queue.prepare_read() else {
            self.queue
                .dispatch_pending(&mut self.state)
                .map_err(|error| WaylandError(format!("Wayland dispatch failed: {error}")))?;
            return Ok(Woke::Queued);
        };
        let timeout = timeout.map(|timeout| Timespec {
            tv_sec: timeout.as_secs().min(i64::MAX as u64) as i64,
            tv_nsec: timeout.subsec_nanos() as i64,
        });
        let mut fds = Vec::with_capacity(2);
        fds.push(PollFd::new(&self.queue, PollFlags::IN));
        if let Some(wake) = wake {
            fds.push(PollFd::from_borrowed_fd(wake, PollFlags::IN));
        }
        let ready = loop {
            match poll(&mut fds, timeout.as_ref()) {
                // A signal landing mid-sleep is not a reason to wake the
                // shell; with no deadline it would otherwise look like one.
                Err(rustix::io::Errno::INTR) if timeout.is_none() => continue,
                Err(rustix::io::Errno::INTR) => break 0,
                Err(error) => return Err(WaylandError(format!("Wayland poll failed: {error}"))),
                Ok(ready) => break ready,
            }
        };
        let alarm = fds.get(1).is_some_and(|fd| !fd.revents().is_empty());
        if ready == 0 || fds[0].revents().is_empty() {
            drop(guard);
            if self.fire_key_repeats() {
                return Ok(Woke::Queued);
            }
            return Ok(if alarm { Woke::Alarm } else { Woke::Timeout });
        }
        guard
            .read()
            .map_err(|error| WaylandError(format!("Wayland read failed: {error}")))?;
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| WaylandError(format!("Wayland dispatch failed: {error}")))?;
        Ok(Woke::Compositor)
    }

    /// Queues the repeats of a held key that are due; true when there were any.
    fn fire_key_repeats(&mut self) -> bool {
        let due = self.state.key_repeat.due(std::time::Instant::now());
        let fired = !due.is_empty();
        for event in due {
            self.state.push_key(event, true, true);
        }
        fired
    }

    /// Holds the compositor's shortcuts off the shell, and reports whether
    /// the compositor speaks the protocol at all -- whether it *agrees* comes
    /// later, as `Event::ShortcutsInhibited`.
    pub fn set_shortcuts_inhibited(&mut self, inhibited: bool) -> bool {
        self.state
            .set_shortcuts_inhibited(inhibited, &self.queue.handle());
        self.state.shortcuts_inhibit_manager.is_some()
    }

    /// One surface's scale in 120ths, whatever kind of surface it is.
    ///
    /// A layer surface answers from its own record; a popup or floating window
    /// from `aux_scales`. 120 -- one to one -- when the surface is unknown or
    /// the compositor offers no fractional scale, which is what every surface
    /// but the primary layer used to get.
    pub fn surface_scale_120(&self, role: WindowId) -> u32 {
        match role {
            WindowId::Layer(id) => self.layer_scale_120(id).unwrap_or(120),
            WindowId::Lock(index) => self.lock_scale_120(index).unwrap_or(120),
            other => self
                .state
                .aux_scales
                .get(&other)
                .map_or(120, |entry| entry.scale_120),
        }
    }

    /// Whether the compositor lets a surface keep the session from idling
    /// (`zwp_idle_inhibit_manager_v1`).
    pub fn supports_idle_inhibit(&self) -> bool {
        self.state.idle_inhibit_manager.is_some()
    }

    /// Holds the session awake, and reports whether the compositor allows it.
    ///
    /// `false` means no compositor support rather than failure to apply: a
    /// configuration can tell the difference between "not inhibiting" and
    /// "cannot inhibit here", which otherwise look identical from Lua.
    pub fn set_idle_inhibited(&mut self, inhibited: bool) -> bool {
        self.state
            .set_idle_inhibited(inhibited, &self.queue.handle());
        self.state.idle_inhibit_manager.is_some()
    }

    /// Whether the compositor speaks layer-shell at all. Without it the
    /// shell's surface is an ordinary window, and there is no edge to hold,
    /// nothing to reserve and nothing to put a backdrop under.
    pub fn supports_layer_shell(&self) -> bool {
        self.state.layer_shell.is_some()
    }

    /// Whether a layer surface opened after the shell's own lands where its
    /// anchors, margins and size put it: through layer-shell, or without it
    /// as a subsurface of the fallback toplevel, placed the same way. False
    /// only with neither, when each would be a window of its own.
    pub fn supports_layer_surfaces(&self) -> bool {
        self.state.layer_shell.is_some() || self.state.subcompositor.is_some()
    }

    /// Publishes UTF-8 text to the clipboard after a compositor input serial is available.
    pub fn set_clipboard(&mut self, text: impl Into<String>) -> bool {
        let Some(manager) = &self.state.data_device_manager else {
            return false;
        };
        let Some(device) = self.state.data_devices.first() else {
            return false;
        };
        let Some(serial) = self.state.latest_input_serial else {
            return false;
        };
        let source = manager.create_copy_paste_source(
            &self.queue.handle(),
            ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"],
        );
        source.set_selection(device, serial);
        self.state.clipboard_text = text.into();
        self.state.clipboard_source = Some(source);
        true
    }

    /// Returns whether clipboard publication has a data device and a current input serial.
    pub fn can_set_clipboard(&self) -> bool {
        self.supports_clipboard() && self.state.latest_input_serial.is_some()
    }

    /// Returns whether the compositor exposes a clipboard data device.
    pub fn supports_clipboard(&self) -> bool {
        self.state.data_device_manager.is_some() && !self.state.data_devices.is_empty()
    }
}
