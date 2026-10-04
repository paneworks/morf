//! The queues themselves: what handlers asked of the platform since the host
//! last looked, the handlers its answers go to, and the limits that keep a
//! runaway handler from filling them.

use std::collections::HashMap;

use crate::handler::Handler;

use super::{
    ClipboardRequest, DragRequest, GammaRequest, InputMethodRequest, OfferReadRequest,
    ScreencopyRequest, TextInputRequest, ToplevelRequest, VirtualKeyboardRequest, WorkspaceRequest,
};

/// How many of most requests may wait for the host at once: more in one turn
/// is a loop, not a shell.
const MAX_QUEUED: usize = 64;
/// Keys and text edits come in bursts, a word at a time.
const MAX_INPUT_QUEUED: usize = 256;
/// Drags out of the shell: one per press, a few in flight at most.
const MAX_DRAGS: usize = 4;
/// Subscriptions to the input method and text input.
const MAX_SUBSCRIBERS: usize = 64;

/// Everything handlers asked of the platform and have not yet had carried
/// out, with the handlers its answers go to.
#[derive(Default)]
pub struct Requests {
    /// The compositor's selection as last heard, for a text input to paste.
    pub clipboard_text: Option<String>,
    pub clipboard_requests: Vec<ClipboardRequest>,
    pub clipboard_callbacks: Vec<Handler>,
    /// `morf.clipboard.watch` callbacks, each with whether it wants the
    /// primary selection too.
    pub clipboard_watchers: Vec<(Handler, bool)>,
    pub offer_reads: Vec<OfferReadRequest>,
    pub offer_read_callbacks: HashMap<u64, Handler>,
    pub next_offer_read: u64,
    pub drag_requests: Vec<DragRequest>,
    pub drag_end_callbacks: Vec<Handler>,
    pub screencopy_requests: Vec<ScreencopyRequest>,
    pub screencopy_callbacks: HashMap<u64, Handler>,
    /// The names captures asked to be published under, by request.
    pub screencopy_names: HashMap<u64, String>,
    pub screencopy_releases: Vec<String>,
    pub next_screencopy: u64,
    pub gamma_requests: Vec<GammaRequest>,
    pub output_power_requests: Vec<bool>,
    /// Idle subscriptions by `(timeout ms, ignore inhibitors)`.
    pub idle_callbacks: HashMap<(u32, bool), Vec<(u64, Handler)>>,
    pub next_idle_subscription: u64,
    pub idle_timeouts_changed: bool,
    pub idle_inhibited: bool,
    pub idle_inhibit_changed: bool,
    pub shortcuts_inhibited: bool,
    pub shortcuts_inhibit_changed: bool,
    pub shortcuts_callbacks: Vec<Handler>,
    pub workspace_requests: Vec<WorkspaceRequest>,
    pub toplevel_requests: Vec<ToplevelRequest>,
    pub virtual_keyboard_requests: Vec<VirtualKeyboardRequest>,
    pub input_method_enable_requested: bool,
    pub input_method_requests: Vec<InputMethodRequest>,
    pub input_method_callbacks: Vec<Handler>,
    pub text_input_enable_requested: bool,
    pub text_input_requests: Vec<TextInputRequest>,
    pub text_input_callbacks: Vec<Handler>,
    pub keyboard_focus_callbacks: Vec<Handler>,
    pub backdrop_callbacks: Vec<Handler>,
}

fn bounded<T>(queue: &mut Vec<T>, limit: usize, item: T, what: &str) -> Result<(), String> {
    if queue.len() >= limit {
        return Err(format!("{what} request limit reached"));
    }
    queue.push(item);
    Ok(())
}

impl Requests {
    /// Queues a gamma change. Only the last word for an output matters: a
    /// slider dragged through a hundred temperatures in one turn sends one
    /// ramp.
    pub fn queue_gamma(&mut self, request: GammaRequest) -> Result<(), String> {
        if self.gamma_requests.len() >= MAX_QUEUED {
            return Err("gamma request limit reached".to_owned());
        }
        self.gamma_requests
            .retain(|queued| queued.output != request.output);
        self.gamma_requests.push(request);
        Ok(())
    }

    pub fn queue_clipboard(&mut self, request: ClipboardRequest) -> Result<(), String> {
        bounded(
            &mut self.clipboard_requests,
            MAX_QUEUED,
            request,
            "clipboard",
        )
    }

    /// Queues a drag out, and who hears how it ended.
    pub fn queue_drag(
        &mut self,
        request: DragRequest,
        done: Option<Handler>,
    ) -> Result<(), String> {
        bounded(&mut self.drag_requests, MAX_DRAGS, request, "drag")?;
        self.drag_end_callbacks.extend(done);
        Ok(())
    }

    pub fn queue_output_power(&mut self, on: bool) -> Result<(), String> {
        bounded(
            &mut self.output_power_requests,
            MAX_QUEUED,
            on,
            "output power",
        )
    }

    pub fn queue_workspace(&mut self, request: WorkspaceRequest) -> Result<(), String> {
        bounded(
            &mut self.workspace_requests,
            MAX_QUEUED,
            request,
            "workspace",
        )
    }

    pub fn queue_toplevel(&mut self, request: ToplevelRequest) -> Result<(), String> {
        bounded(&mut self.toplevel_requests, MAX_QUEUED, request, "window")
    }

    pub fn queue_virtual_keyboard(
        &mut self,
        request: VirtualKeyboardRequest,
    ) -> Result<(), String> {
        bounded(
            &mut self.virtual_keyboard_requests,
            MAX_INPUT_QUEUED,
            request,
            "virtual keyboard",
        )
    }

    pub fn queue_input_method(&mut self, request: InputMethodRequest) -> Result<(), String> {
        bounded(
            &mut self.input_method_requests,
            MAX_INPUT_QUEUED,
            request,
            "input method",
        )
    }

    pub fn queue_text_input(&mut self, request: TextInputRequest) -> Result<(), String> {
        bounded(
            &mut self.text_input_requests,
            MAX_INPUT_QUEUED,
            request,
            "text input",
        )
    }

    /// Subscribes to the input method's context, asking for the role.
    pub fn subscribe_input_method(&mut self, callback: Handler) -> Result<(), String> {
        if self.input_method_callbacks.len() >= MAX_SUBSCRIBERS {
            return Err("input method callback limit reached".to_owned());
        }
        self.input_method_callbacks.push(callback);
        self.input_method_enable_requested = true;
        Ok(())
    }

    /// Subscribes to text input's edits, asking for it to be made.
    pub fn subscribe_text_input(&mut self, callback: Handler) -> Result<(), String> {
        if self.text_input_callbacks.len() >= MAX_SUBSCRIBERS {
            return Err("text input callback limit reached".to_owned());
        }
        self.text_input_callbacks.push(callback);
        self.text_input_enable_requested = true;
        Ok(())
    }

    /// The idle thresholds subscribed to, each with whether it ignores
    /// inhibitors, in order.
    pub fn idle_timeouts(&self) -> Vec<(u32, bool)> {
        let mut timeouts = self.idle_callbacks.keys().copied().collect::<Vec<_>>();
        timeouts.sort_unstable();
        timeouts
    }

    /// The thresholds, when they changed since this was last asked.
    pub fn take_idle_timeouts_change(&mut self) -> Option<Vec<(u32, bool)>> {
        std::mem::take(&mut self.idle_timeouts_changed).then(|| self.idle_timeouts())
    }

    /// The callbacks subscribed to one threshold.
    pub fn idle_subscribers(&self, timeout_ms: u32, input_only: bool) -> Vec<Handler> {
        self.idle_callbacks
            .get(&(timeout_ms, input_only))
            .map(|callbacks| {
                callbacks
                    .iter()
                    .map(|(_, callback)| callback.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sets whether the session is held awake; the host hears each asking,
    /// a repeated one too.
    pub fn set_idle_inhibited(&mut self, inhibited: bool) {
        self.idle_inhibited = inhibited;
        self.idle_inhibit_changed = true;
    }

    /// Sets whether the compositor's shortcuts are held off the shell.
    pub fn set_shortcuts_inhibited(&mut self, inhibited: bool) {
        self.shortcuts_inhibited = inhibited;
        self.shortcuts_inhibit_changed = true;
    }

    /// A pending change to whether the session is held awake.
    pub fn take_idle_inhibit_change(&mut self) -> Option<bool> {
        std::mem::take(&mut self.idle_inhibit_changed).then_some(self.idle_inhibited)
    }

    /// A pending change to whether the compositor's shortcuts are held off.
    pub fn take_shortcuts_inhibit_change(&mut self) -> Option<bool> {
        std::mem::take(&mut self.shortcuts_inhibit_changed).then_some(self.shortcuts_inhibited)
    }

    pub fn take_gamma_requests(&mut self) -> Vec<GammaRequest> {
        std::mem::take(&mut self.gamma_requests)
    }

    pub fn take_output_power_requests(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.output_power_requests)
    }

    pub fn take_clipboard_requests(&mut self) -> Vec<ClipboardRequest> {
        std::mem::take(&mut self.clipboard_requests)
    }

    pub fn take_offer_reads(&mut self) -> Vec<OfferReadRequest> {
        std::mem::take(&mut self.offer_reads)
    }

    pub fn take_drag_requests(&mut self) -> Vec<DragRequest> {
        std::mem::take(&mut self.drag_requests)
    }

    pub fn take_screencopy_requests(&mut self) -> Vec<ScreencopyRequest> {
        std::mem::take(&mut self.screencopy_requests)
    }

    pub fn take_screencopy_name(&mut self, request_id: u64) -> Option<String> {
        self.screencopy_names.remove(&request_id)
    }

    pub fn take_screencopy_releases(&mut self) -> Vec<String> {
        std::mem::take(&mut self.screencopy_releases)
    }

    pub fn take_workspace_requests(&mut self) -> Vec<WorkspaceRequest> {
        std::mem::take(&mut self.workspace_requests)
    }

    pub fn take_toplevel_requests(&mut self) -> Vec<ToplevelRequest> {
        std::mem::take(&mut self.toplevel_requests)
    }

    pub fn take_virtual_keyboard_requests(&mut self) -> Vec<VirtualKeyboardRequest> {
        std::mem::take(&mut self.virtual_keyboard_requests)
    }

    pub fn take_input_method_enable_request(&mut self) -> bool {
        std::mem::take(&mut self.input_method_enable_requested)
    }

    pub fn take_input_method_requests(&mut self) -> Vec<InputMethodRequest> {
        std::mem::take(&mut self.input_method_requests)
    }

    pub fn take_text_input_enable_request(&mut self) -> bool {
        std::mem::take(&mut self.text_input_enable_requested)
    }

    pub fn take_text_input_requests(&mut self) -> Vec<TextInputRequest> {
        std::mem::take(&mut self.text_input_requests)
    }
}

#[cfg(test)]
mod tests;
