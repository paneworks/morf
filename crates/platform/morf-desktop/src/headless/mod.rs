//! The desktop protocols with no compositor behind them: what `morf check`,
//! `render` and `test` run against.
//!
//! Deterministic by construction. Every request is recorded; one for a
//! protocol not switched on is refused, as a compositor without it would.
//! Switched on, each acts on the stand-in's own lists (workspaces, windows,
//! the selection) the way a compositor would, and a capture answers at once
//! with a blank frame.

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::sync::Arc;

use morf_app::OfferInfo;

use crate::data_control::OFFER_ID_BASE;
use crate::desktop::IdleTimeout;
use crate::{
    CaptureBuffer, Desktop, DesktopCapabilities, DesktopEvent, GammaSettings, OutputPowerMode,
    ScreencopyFormat, ScreencopyFrame, ToplevelAction, ToplevelInfo, WorkspaceInfo,
};

/// One request a [`HeadlessDesktop`] was handed, in the order it came.
#[derive(Clone, Debug, PartialEq)]
pub enum DesktopRequest {
    SetSelection {
        primary: bool,
        mime_types: Vec<String>,
    },
    ClearSelection {
        primary: bool,
    },
    ReadOffer {
        request_id: u64,
        offer_id: u64,
        mime: String,
    },
    CaptureOutput {
        request_id: u64,
        include_cursor: bool,
        output: Option<String>,
    },
    CaptureOutputImage {
        request_id: u64,
        gpu: bool,
        output: Option<String>,
    },
    CaptureWindow {
        request_id: u64,
        identifier: String,
        gpu: bool,
    },
    AttachCaptureDmabuf {
        request_id: u64,
    },
    AttachCaptureShm {
        request_id: u64,
    },
    SetGamma {
        output: Option<String>,
        settings: GammaSettings,
    },
    ResetGamma {
        output: Option<String>,
    },
    IdleTimeouts(Vec<IdleTimeout>),
    OutputPower(OutputPowerMode),
    ActivateWorkspace(String),
    RemoveWorkspace(String),
    AssignWorkspace {
        key: String,
        output: String,
    },
    ControlToplevel {
        identifier: String,
        action: ToplevelAction,
    },
}

/// The selection the stand-in owns.
struct Selection {
    offer: u64,
    data: Vec<(String, Arc<Vec<u8>>)>,
}

/// A desktop with no compositor: no protocols unless asked for, empty lists
/// unless given some.
#[derive(Default)]
pub struct HeadlessDesktop {
    capabilities: DesktopCapabilities,
    outputs: Vec<String>,
    workspaces: Vec<WorkspaceInfo>,
    workspaces_changed: bool,
    toplevels: Vec<ToplevelInfo>,
    toplevels_changed: bool,
    selections: [Option<Selection>; 2],
    next_offer: u64,
    idle_timeouts: Vec<IdleTimeout>,
    events: VecDeque<DesktopEvent>,
    requests: Vec<DesktopRequest>,
}

impl HeadlessDesktop {
    /// No protocols, no outputs, nothing listed.
    pub fn new() -> Self {
        Self::default()
    }

    /// The protocols this stand-in answers for.
    pub fn with_capabilities(mut self, capabilities: DesktopCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// The outputs a request may name.
    pub fn with_outputs<I: IntoIterator<Item = S>, S: Into<String>>(mut self, names: I) -> Self {
        self.outputs = names.into_iter().map(Into::into).collect();
        self
    }

    /// Workspaces to list, reported as a change on the next look.
    pub fn with_workspaces(mut self, workspaces: Vec<WorkspaceInfo>) -> Self {
        self.workspaces = workspaces;
        self.workspaces_changed = true;
        self
    }

    /// Windows to list, reported as a change on the next look.
    pub fn with_toplevels(mut self, toplevels: Vec<ToplevelInfo>) -> Self {
        self.toplevels = toplevels;
        self.toplevels_changed = true;
        self
    }

    /// Every request handed over so far.
    pub fn requests(&self) -> &[DesktopRequest] {
        &self.requests
    }

    /// Every request handed over since the last take.
    pub fn take_requests(&mut self) -> Vec<DesktopRequest> {
        std::mem::take(&mut self.requests)
    }

    /// The idle thresholds last asked for.
    pub fn idle_timeouts(&self) -> &[IdleTimeout] {
        &self.idle_timeouts
    }

    /// Queues news as a compositor would send it (an idle threshold crossed,
    /// a selection from another client).
    pub fn push_event(&mut self, event: DesktopEvent) {
        self.events.push_back(event);
    }

    fn knows_output(&self, output: Option<&str>) -> bool {
        output.is_none_or(|name| self.outputs.iter().any(|known| known == name))
    }

    /// Answers a capture at once with a blank frame.
    fn capture(&mut self, request_id: u64) -> bool {
        self.events.push_back(DesktopEvent::Screencopy {
            request_id,
            result: Ok(ScreencopyFrame {
                width: 1,
                height: 1,
                stride: 4,
                format: ScreencopyFormat::Argb8888,
                y_invert: false,
                pixels: vec![0; 4],
                dmabuf: false,
            }),
        });
        true
    }

    fn workspace_mut(&mut self, key: &str) -> Option<&mut WorkspaceInfo> {
        self.workspaces
            .iter_mut()
            .find(|workspace| workspace.key == key)
    }
}

impl Desktop for HeadlessDesktop {
    fn capabilities(&self) -> DesktopCapabilities {
        self.capabilities
    }

    fn dispatch_pending(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn next_event(&mut self) -> Option<DesktopEvent> {
        self.events.pop_front()
    }

    fn set_waker(&mut self, _waker: fn()) {}

    fn set_selection(&mut self, data: Vec<(String, Arc<Vec<u8>>)>, primary: bool) -> bool {
        let mime_types: Vec<String> = data.iter().map(|(mime, _)| mime.clone()).collect();
        self.requests.push(DesktopRequest::SetSelection {
            primary,
            mime_types: mime_types.clone(),
        });
        if !self.capabilities.data_control || (primary && !self.capabilities.primary_selection) {
            return false;
        }
        self.next_offer += 1;
        let offer = OFFER_ID_BASE + self.next_offer;
        self.selections[usize::from(primary)] = Some(Selection { offer, data });
        self.events.push_back(DesktopEvent::Selection {
            primary,
            offer: Some(OfferInfo {
                id: offer,
                mime_types,
            }),
        });
        true
    }

    fn clear_selection(&mut self, primary: bool) -> bool {
        self.requests
            .push(DesktopRequest::ClearSelection { primary });
        if !self.capabilities.data_control || (primary && !self.capabilities.primary_selection) {
            return false;
        }
        self.selections[usize::from(primary)] = None;
        self.events.push_back(DesktopEvent::Selection {
            primary,
            offer: None,
        });
        true
    }

    fn owns_offer(&self, offer_id: u64) -> bool {
        offer_id > OFFER_ID_BASE
    }

    fn read_offer(&mut self, request_id: u64, offer_id: u64, mime: &str) {
        self.requests.push(DesktopRequest::ReadOffer {
            request_id,
            offer_id,
            mime: mime.to_owned(),
        });
        let selection = self
            .selections
            .iter()
            .flatten()
            .find(|s| s.offer == offer_id);
        let result = match selection {
            None => Err("the offer is gone".to_owned()),
            Some(selection) => {
                let offered: Vec<String> = selection.data.iter().map(|(m, _)| m.clone()).collect();
                morf_app::mime::resolve_mime(mime, &offered)
                    .and_then(|chosen| selection.data.iter().find(|(m, _)| *m == chosen))
                    .map(|(_, bytes)| bytes.as_ref().clone())
                    .ok_or_else(|| format!("the offer has no `{mime}`"))
            }
        };
        self.events
            .push_back(DesktopEvent::OfferRead { request_id, result });
    }

    fn has_output(&self, name: &str) -> bool {
        self.outputs.iter().any(|known| known == name)
    }

    fn capture_output(
        &mut self,
        request_id: u64,
        include_cursor: bool,
        output: Option<&str>,
    ) -> bool {
        self.requests.push(DesktopRequest::CaptureOutput {
            request_id,
            include_cursor,
            output: output.map(str::to_owned),
        });
        self.capabilities.screencopy && self.knows_output(output) && self.capture(request_id)
    }

    fn capture_output_image(&mut self, request_id: u64, gpu: bool, output: Option<&str>) -> bool {
        self.requests.push(DesktopRequest::CaptureOutputImage {
            request_id,
            gpu,
            output: output.map(str::to_owned),
        });
        self.capabilities.image_capture && self.knows_output(output) && self.capture(request_id)
    }

    fn capture_window(&mut self, request_id: u64, identifier: &str, gpu: bool) -> bool {
        self.requests.push(DesktopRequest::CaptureWindow {
            request_id,
            identifier: identifier.to_owned(),
            gpu,
        });
        let known = self.toplevels.iter().any(|t| t.identifier == identifier);
        self.capabilities.window_capture && known && self.capture(request_id)
    }

    fn attach_capture_dmabuf(
        &mut self,
        request_id: u64,
        _buffer: &CaptureBuffer<'_>,
    ) -> Result<(), String> {
        self.requests
            .push(DesktopRequest::AttachCaptureDmabuf { request_id });
        Err("no capture is waiting for a buffer".to_owned())
    }

    fn attach_capture_shm(&mut self, request_id: u64) -> bool {
        self.requests
            .push(DesktopRequest::AttachCaptureShm { request_id });
        false
    }

    fn set_gamma(&mut self, output: Option<&str>, settings: GammaSettings) -> Result<(), String> {
        self.requests.push(DesktopRequest::SetGamma {
            output: output.map(str::to_owned),
            settings,
        });
        if !self.capabilities.gamma_control {
            return Err("this compositor has no gamma control".to_owned());
        }
        match output {
            Some(name) if !self.knows_output(Some(name)) => {
                Err(format!("no output named `{name}`"))
            }
            _ => Ok(()),
        }
    }

    fn reset_gamma(&mut self, output: Option<&str>) -> Result<(), String> {
        self.requests.push(DesktopRequest::ResetGamma {
            output: output.map(str::to_owned),
        });
        match output {
            Some(name) if !self.knows_output(Some(name)) => {
                Err(format!("no output named `{name}`"))
            }
            _ => Ok(()),
        }
    }

    fn take_gamma_failures(&mut self) -> Vec<String> {
        Vec::new()
    }

    fn set_idle_timeouts(&mut self, timeouts: &[IdleTimeout]) -> bool {
        self.update_idle_timeouts(timeouts)
    }

    fn update_idle_timeouts(&mut self, timeouts: &[IdleTimeout]) -> bool {
        self.requests
            .push(DesktopRequest::IdleTimeouts(timeouts.to_vec()));
        self.idle_timeouts = timeouts.to_vec();
        self.capabilities.idle_notify
    }

    fn set_output_power(&mut self, mode: OutputPowerMode) -> bool {
        self.requests.push(DesktopRequest::OutputPower(mode));
        self.capabilities.output_power
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let mut workspaces = self.workspaces.clone();
        workspaces.sort_by(|a, b| a.coordinates.cmp(&b.coordinates).then(a.key.cmp(&b.key)));
        workspaces
    }

    fn take_workspaces_changed(&mut self) -> bool {
        std::mem::take(&mut self.workspaces_changed)
    }

    fn activate_workspace(&mut self, key: &str) -> bool {
        self.requests
            .push(DesktopRequest::ActivateWorkspace(key.to_owned()));
        if !self.capabilities.workspaces {
            return false;
        }
        let Some(output) = self
            .workspace_mut(key)
            .filter(|workspace| workspace.activatable)
            .map(|workspace| workspace.output.clone())
        else {
            return false;
        };
        for workspace in &mut self.workspaces {
            if workspace.output == output {
                workspace.active = workspace.key == key;
            }
        }
        self.workspaces_changed = true;
        true
    }

    fn remove_workspace(&mut self, key: &str) -> bool {
        self.requests
            .push(DesktopRequest::RemoveWorkspace(key.to_owned()));
        let before = self.workspaces.len();
        if self.capabilities.workspaces {
            self.workspaces
                .retain(|workspace| workspace.key != key || !workspace.removable);
        }
        self.workspaces_changed |= self.workspaces.len() != before;
        self.workspaces.len() != before
    }

    fn assign_workspace(&mut self, key: &str, output: &str) -> bool {
        self.requests.push(DesktopRequest::AssignWorkspace {
            key: key.to_owned(),
            output: output.to_owned(),
        });
        if !self.capabilities.workspaces || !self.has_output(output) {
            return false;
        }
        let Some(workspace) = self.workspace_mut(key).filter(|w| w.assignable) else {
            return false;
        };
        workspace.output = output.to_owned();
        self.workspaces_changed = true;
        true
    }

    fn toplevels(&self) -> Vec<ToplevelInfo> {
        self.toplevels.clone()
    }

    fn take_toplevels_changed(&mut self) -> bool {
        std::mem::take(&mut self.toplevels_changed)
    }

    fn control_toplevel(&mut self, identifier: &str, action: ToplevelAction) -> bool {
        self.requests.push(DesktopRequest::ControlToplevel {
            identifier: identifier.to_owned(),
            action,
        });
        let Some(index) = self
            .toplevels
            .iter()
            .position(|t| t.identifier == identifier && t.controllable)
            .filter(|_| self.capabilities.toplevel_control)
        else {
            return false;
        };
        match action {
            ToplevelAction::Activate => {
                for (at, window) in self.toplevels.iter_mut().enumerate() {
                    window.activated = at == index;
                }
            }
            ToplevelAction::Close => {
                self.toplevels.remove(index);
            }
            ToplevelAction::Maximized(on) => self.toplevels[index].maximized = on,
            ToplevelAction::Minimized(on) => self.toplevels[index].minimized = on,
            ToplevelAction::Fullscreen(on) => self.toplevels[index].fullscreen = on,
            ToplevelAction::MinimizeTarget { .. } => return true,
        }
        self.toplevels_changed = true;
        true
    }
}
