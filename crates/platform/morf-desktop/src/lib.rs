//! The desktop protocols a shell uses, on the same Wayland connection as its
//! windows but on an event queue of its own: screen capture (outputs
//! and windows, into shared memory or dmabufs), gamma ramps, the clipboard
//! over data control, workspaces, foreign toplevels (listed and acted on),
//! idle notification and output power.
//!
//! The queue is its own because every protocol handler of smithay's toolkit
//! is implemented on one state type, and a state type of another crate
//! cannot be given them: this crate's [`Desktop`] has its own registry and
//! its own outputs, and the host dispatches it after each wake of its loop
//! (any read of the socket fills this queue too).

mod capture;
mod data_control;
mod gamma;
mod idle;
mod output_power;
mod toplevels;
mod workspaces;

use std::collections::{HashMap, VecDeque};

use morf_app::OfferInfo;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_client::backend::ObjectId;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_seat, wl_surface};
use wayland_client::{Connection, EventQueue, QueueHandle};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1,
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
    zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1,
};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

pub use capture::{CaptureBuffer, ScreencopyFormat, ScreencopyFrame};
pub use gamma::{
    GammaSettings, NEUTRAL as NEUTRAL_TEMPERATURE, TEMPERATURE_RANGE, ramps as gamma_ramps,
    white_point,
};
pub use output_power::OutputPowerMode;
pub use toplevels::{ToplevelAction, ToplevelInfo};
pub use workspaces::WorkspaceInfo;

/// What the desktop protocols tell the host.
#[derive(Clone, Debug, PartialEq)]
pub enum DesktopEvent {
    /// A capture asked for on the GPU has been described by its session.
    ///
    /// The compositor has said what size it will produce, which device the
    /// buffer must live on, and which formats and modifiers it will draw
    /// into. Nothing is allocated yet: the renderer answers with a dmabuf
    /// through `attach_capture_dmabuf`, or falls back to shared memory
    /// through `attach_capture_shm`, and the capture continues either way.
    CaptureOffer {
        /// Runtime-local request identifier.
        request_id: u64,
        /// Pixel width the compositor will produce.
        width: u32,
        /// Pixel height the compositor will produce.
        height: u32,
        /// The `dev_t` of the device the buffer must be allocated on, when
        /// the compositor named one.
        device: Option<u64>,
        /// DRM fourcc codes and, for each, the modifiers the compositor can
        /// draw with, in its order of preference.
        formats: Vec<(u32, Vec<u64>)>,
    },
    /// An output capture completed or failed.
    Screencopy {
        /// Runtime-local request identifier.
        request_id: u64,
        /// Captured pixels or compositor failure.
        result: Result<ScreencopyFrame, String>,
    },
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
    /// Shared memory for captures that are not dmabufs.
    shm: Option<Shm>,
    screencopy_manager: Option<ZwlrScreencopyManagerV1>,
    /// Output captures in flight on `wlr-screencopy`.
    screencopies: Vec<capture::PendingScreencopy>,
    /// `ext-image-copy-capture-v1` and the two source factories, when offered.
    ///
    /// The replacement for `wlr-screencopy`, and the reason to want it: that
    /// one captures outputs and only outputs, so a thumbnail of a *window*
    /// could not be had at all -- cropping an output gives whatever is on top
    /// at that rectangle, not the window.
    capture_manager: Option<ExtImageCopyCaptureManagerV1>,
    output_source_manager: Option<ExtOutputImageCaptureSourceManagerV1>,
    toplevel_source_manager: Option<ExtForeignToplevelImageCaptureSourceManagerV1>,
    /// Captures in flight on the newer protocol.
    captures: Vec<capture::PendingCapture>,
    /// `zwp_linux_dmabuf_v1`, to turn a dmabuf the renderer exported into a
    /// `wl_buffer` a capture frame can be given. The formats it advertises on
    /// its own are not consulted: the capture session says what *it* will
    /// draw into, which is the narrower and the right answer.
    linux_dmabuf: Option<ZwpLinuxDmabufV1>,
    /// `ext-foreign-toplevel-list-v1`, when the compositor offers it.
    toplevel_list: Option<ExtForeignToplevelListV1>,
    /// Every window the compositor has told us about, keyed by its handle.
    ///
    /// Held as a map because the protocol describes a window over several
    /// events and finishes with `done`: a handle arrives bare, then its
    /// title, app id and identifier follow, and only after `done` is it worth
    /// showing anybody.
    toplevels: HashMap<ObjectId, ToplevelInfo>,
    /// Whether the list changed since a caller last looked.
    toplevels_changed: bool,
    /// The handle behind each window, kept so a capture can name one.
    toplevel_handles: HashMap<String, ExtForeignToplevelHandleV1>,
    toplevel_control_manager: Option<ZwlrForeignToplevelManagerV1>,
    toplevel_controls: HashMap<ObjectId, toplevels::ToplevelControl>,
    toplevel_control_handles: HashMap<ObjectId, ZwlrForeignToplevelHandleV1>,
    /// The shell's own surface, when the host has said: what a window
    /// minimizes towards a rectangle of.
    shell_surface: Option<wl_surface::WlSurface>,
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
            shm: Shm::bind(&globals, &qh).ok(),
            screencopy_manager: globals.bind(&qh, 1..=3, ()).ok(),
            screencopies: Vec::new(),
            // The newer capture protocol, and the two things that name what
            // to capture. Bound separately because a compositor may offer the
            // copy machinery and only one kind of source.
            capture_manager: globals.bind(&qh, 1..=1, ()).ok(),
            output_source_manager: globals.bind(&qh, 1..=1, ()).ok(),
            toplevel_source_manager: globals.bind(&qh, 1..=1, ()).ok(),
            captures: Vec::new(),
            // Version 2 is where `create_immed` arrived, and nothing newer is
            // needed.
            linux_dmabuf: globals.bind(&qh, 2..=5, ()).ok(),
            // Every window the compositor knows about, and it tells us as
            // they come and go.
            toplevel_list: globals.bind(&qh, 1..=1, ()).ok(),
            toplevels: HashMap::new(),
            toplevels_changed: false,
            toplevel_handles: HashMap::new(),
            // Version 3 where offered, for `set_fullscreen`; 1 is enough for
            // activate, close and the maximize/minimize pair.
            toplevel_control_manager: globals.bind(&qh, 1..=3, ()).ok(),
            toplevel_controls: HashMap::new(),
            toplevel_control_handles: HashMap::new(),
            shell_surface: None,
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
        Ok(Self {
            connection,
            queue,
            state,
        })
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

    /// Says which surface is the shell's own: the one a minimized window's
    /// rectangle is on.
    pub fn set_shell_surface(&mut self, surface: Option<wl_surface::WlSurface>) {
        self.state.shell_surface = surface;
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
        self.output_power
            .output_added(&output, for_every_output, qh);
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

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl ShmHandler for DesktopState {
    fn shm_state(&mut self) -> &mut Shm {
        self.shm
            .as_mut()
            .expect("a capture asks for shared memory only when there is some")
    }
}

impl ProvidesRegistryState for DesktopState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_registry!(DesktopState);
smithay_client_toolkit::delegate_dispatch2!(DesktopState);
wayland_client::delegate_noop!(DesktopState: ignore ZwlrScreencopyManagerV1);
