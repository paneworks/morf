//! `wlr-output-power-management`: outputs switched off and on, as a lock
//! screen or an idle policy asks.

use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_output;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::output_power_management::v1::client::{
    zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1,
    zwlr_output_power_v1::{self, ZwlrOutputPowerV1},
};

use crate::{Desktop, DesktopState};

/// An output's power state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputPowerMode {
    /// The output is powered down.
    Off,
    /// The output is powered on.
    On,
}

struct OutputPowerControl {
    output: wl_output::WlOutput,
    control: ZwlrOutputPowerV1,
}

#[derive(Default)]
pub(crate) struct OutputPowerState {
    manager: Option<ZwlrOutputPowerManagerV1>,
    controls: Vec<OutputPowerControl>,
    /// The mode last asked for: an output that arrives later gets it too,
    /// when the request was for every output.
    mode: Option<OutputPowerMode>,
}

impl OutputPowerState {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<DesktopState>) -> Self {
        Self {
            manager: globals.bind(qh, 1..=1, ()).ok(),
            ..Self::default()
        }
    }

    fn apply(
        &mut self,
        output: &wl_output::WlOutput,
        mode: OutputPowerMode,
        qh: &QueueHandle<DesktopState>,
    ) {
        let Some(manager) = self.manager.clone() else {
            return;
        };
        let control = self
            .controls
            .iter()
            .find(|control| control.output == *output)
            .map(|control| control.control.clone())
            .unwrap_or_else(|| {
                let control = manager.get_output_power(output, qh, output.clone());
                self.controls.push(OutputPowerControl {
                    output: output.clone(),
                    control: control.clone(),
                });
                control
            });
        control.set_mode(match mode {
            OutputPowerMode::Off => zwlr_output_power_v1::Mode::Off,
            OutputPowerMode::On => zwlr_output_power_v1::Mode::On,
        });
    }

    /// An output arrived: it gets the mode every output was asked for.
    pub(crate) fn output_added(
        &mut self,
        output: &wl_output::WlOutput,
        for_every_output: bool,
        qh: &QueueHandle<DesktopState>,
    ) {
        if for_every_output && let Some(mode) = self.mode {
            self.apply(output, mode, qh);
        }
    }

    /// Lets go of the control of an output that went away.
    pub(crate) fn forget(&mut self, output: &wl_output::WlOutput) {
        if let Some(index) = self.controls.iter().position(|control| control.output == *output) {
            self.controls.remove(index).control.destroy();
        }
    }
}

impl Desktop {
    /// Asks for a power state for the shell's own output, or for every
    /// output when it has none (a lock screen). Returns whether the
    /// compositor offers output power management at all.
    pub fn set_output_power(&mut self, mode: OutputPowerMode) -> bool {
        if self.state.output_power.manager.is_none() {
            return false;
        }
        self.state.output_power.mode = Some(mode);
        let outputs = match self.state.own_output.as_deref() {
            Some(name) => self.state.output_named(name).into_iter().collect(),
            None => self.state.outputs.outputs().collect::<Vec<_>>(),
        };
        let qh = self.handle();
        for output in outputs {
            self.state.output_power.apply(&output, mode, &qh);
        }
        true
    }
}

impl Dispatch<ZwlrOutputPowerManagerV1, ()> for DesktopState {
    fn event(
        _: &mut Self,
        _: &ZwlrOutputPowerManagerV1,
        _: <ZwlrOutputPowerManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrOutputPowerV1, wl_output::WlOutput> for DesktopState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputPowerV1,
        event: zwlr_output_power_v1::Event,
        _: &wl_output::WlOutput,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The mode the compositor reports is recorded nowhere: nothing asks.
        if let zwlr_output_power_v1::Event::Failed = event
            && let Some(index) = state
                .output_power
                .controls
                .iter()
                .position(|control| control.control == *proxy)
        {
            state.output_power.controls.remove(index).control.destroy();
        }
    }
}
