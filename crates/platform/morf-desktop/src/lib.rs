//! The desktop protocols a shell uses, on the same Wayland connection as its
//! windows but on an event queue of its own: gamma ramps, output power,
//! workspaces, idle notification and the clipboard over data control so far,
//! then the rest of what moves here from `morf-app` (capture, foreign
//! toplevels).
//!
//! The queue is its own because every protocol handler of smithay's toolkit
//! is implemented on one state type, and a state type of another crate
//! cannot be given them: this crate's [`Desktop`] has its own registry and
//! its own outputs, and the host dispatches it after each wake of its loop
//! (any read of the socket fills this queue too).

mod data_control;
mod gamma;
mod idle;
mod output_power;
mod workspaces;

use std::collections::VecDeque;

use morf_app::OfferInfo;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_seat};
use wayland_client::{Connection, EventQueue, QueueHandle};

pub use output_power::OutputPowerMode;
pub use workspaces::WorkspaceInfo;
pub use gamma::{
    GammaSettings, NEUTRAL as NEUTRAL_TEMPERATURE, TEMPERATURE_RANGE, ramps as gamma_ramps,
    white_point,
};

/// What the desktop protocols tell the host.
#[derive(Clone, Debug, PartialEq)]
pub enum DesktopEvent {
    /// An idle threshold was crossed, one way or the other.
    Idle {
        timeout_ms: u32,
        /// Whether this threshold counts input only, ignoring idle inhibitors.
        input_only: bool,
        idle: bool,
    },
    /// The selection changed, as data control sees it: with no focus needed,
    /// and before anything is read. `offer` is `None` when it was cleared.
    Selection {
        /// Whether this is the primary selection (middle-click paste).
        primary: bool,
        offer: Option<OfferInfo>,
    },
    /// A read asked for with [`Desktop::read_offer`] finished.
    OfferRead {
        request_id: u64,
        result: Result<Vec<u8>, String>,
    },
}

/// What the desktop protocols know, dispatched on their own queue.
pub struct DesktopState {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    events: VecDeque<DesktopEvent>,
    /// The output the shell's own surface sits on, by name, when the host
    /// has said: what a request naming no output is for.
    own_output: Option<String>,
    gamma: gamma::GammaState,
    output_power: output_power::OutputPowerState,
    workspaces: workspaces::WorkspaceState,
    idle: idle::IdleState,
    clipboard: data_control::ClipboardState,
}

/// The desktop protocols, on a connection a window backend opened.
pub struct Desktop {
    connection: Connection,
    queue: EventQueue<DesktopState>,
    state: DesktopState,
}

impl Desktop {
    /// Binds the desktop protocols the compositor offers on `connection`.
    pub fn new(connection: Connection) -> Result<Self, String> {
        let (globals, mut queue) = registry_queue_init::<DesktopState>(&connection)
            .map_err(|error| format!("could not read the Wayland globals: {error}"))?;
        let qh = queue.handle();
        let mut state = DesktopState {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            seats: SeatState::new(&globals, &qh),
            events: VecDeque::new(),
            own_output: None,
            gamma: gamma::GammaState::bind(&globals, &qh),
            output_power: output_power::OutputPowerState::bind(&globals, &qh),
            workspaces: workspaces::WorkspaceState::bind(&globals, &qh),
            idle: idle::IdleState::bind(&globals, &qh),
            clipboard: data_control::ClipboardState::bind(&globals, &qh),
        };
        // The outputs' names arrive as events: hear them before anything is
        // asked of an output by name.
        queue
            .roundtrip(&mut state)
            .map_err(|error| format!("could not hear the outputs: {error}"))?;
        // A seat already there when the registry was read is announced to no
        // `new_seat`: the clipboard is watched on it here.
        if let Some(seat) = state.seat() {
            state.clipboard.seat_added(&seat, &qh);
        }
        Ok(Self { connection, queue, state })
    }

    /// Hears what the compositor sent this queue. The host's loop reads the
    /// socket; this takes what it read.
    pub fn dispatch_pending(&mut self) -> Result<(), String> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| format!("Wayland dispatch failed: {error}"))?;
        self.connection
            .flush()
            .map_err(|error| format!("Wayland flush failed: {error}"))?;
        Ok(())
    }

    /// The next thing the desktop protocols have to tell.
    pub fn next_event(&mut self) -> Option<DesktopEvent> {
        self.state.clipboard.drain_reads(&mut self.state.events);
        self.state.events.pop_front()
    }

    /// Says which output the shell sits on (by name): a request that names
    /// no output is for that one.
    pub fn set_own_output(&mut self, name: Option<String>) {
        self.state.own_output = name;
    }

    fn handle(&self) -> QueueHandle<DesktopState> {
        self.queue.handle()
    }
}

impl DesktopState {
    /// The seat, when there is one.
    fn seat(&self) -> Option<wl_seat::WlSeat> {
        self.seats.seats().next()
    }

    /// The output named `name`.
    fn output_named(&self, name: &str) -> Option<wl_output::WlOutput> {
        self.outputs.outputs().find(|output| {
            self.outputs
                .info(output)
                .and_then(|info| info.name)
                .is_some_and(|candidate| candidate == name)
        })
    }

    /// The outputs a request is for: the one named, or the one the shell
    /// sits on, or else every output.
    fn targets(&self, output: Option<&str>) -> Result<Vec<wl_output::WlOutput>, String> {
        match output.or(self.own_output.as_deref()) {
            Some(name) => self
                .output_named(name)
                .map(|output| vec![output])
                .ok_or_else(|| format!("no output named `{name}`")),
            None => Ok(self.outputs.outputs().collect()),
        }
    }

    fn output_name(&self, output: &wl_output::WlOutput) -> String {
        self.outputs
            .info(output)
            .and_then(|info| info.name)
            .unwrap_or_default()
    }
}

impl OutputHandler for DesktopState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let for_every_output = self.own_output.is_none();
        self.output_power.output_added(&output, for_every_output, qh);
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.gamma.forget(&output);
        self.output_power.forget(&output);
    }
}

impl SeatHandler for DesktopState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.idle.refresh(Some(&seat), qh);
        self.clipboard.seat_added(&seat, qh);
    }

    fn new_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl ProvidesRegistryState for DesktopState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_registry!(DesktopState);
smithay_client_toolkit::delegate_dispatch2!(DesktopState);
