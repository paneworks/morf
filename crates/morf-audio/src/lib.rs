//! Audio for morf: the devices a machine has, the applications using them,
//! the volumes of both, and how loud things are right now.
//!
//! Nothing in the API names a sound server. A shell asks for sinks, sources
//! and streams, sets volumes, picks defaults and meters levels; which server
//! answers is a [`Backend`], chosen by [`Audio::connect`] and replaceable by
//! [`Audio::with_backend`] — a test hands in [`fake::fake`].
//!
//! # The PipeWire backend
//!
//! The one real backend speaks to PipeWire, which is what a Linux desktop
//! runs for sound now, through `libpipewire-0.3`. It is **opened at run time,
//! not linked**: morf's distributable binary depends on nothing beyond the
//! C library and libxkbcommon (Wayland and Vulkan are opened the same way),
//! and a hard dependency on libpipewire would make the shell refuse to start
//! on a machine without it — a kiosk, a phone image, a container — even when
//! the configuration never plays a sound. Opened lazily, a missing library is
//! simply an [`Audio`] that is not [`available`](AudioState::available).
//!
//! That rules out the `pipewire` crate, whose `-sys` crates link the library
//! at build time (and need its headers and libclang to build). The backend
//! instead declares the few structures it uses itself — the stable C ABI of
//! PipeWire's interfaces, which is versioned per interface and has only ever
//! grown at the end — and builds and reads the SPA parameters it exchanges
//! with its own small POD encoder. See `pipewire/ffi.rs` for what is used.
//!
//! The server runs on its own thread with its own loop. Commands go to it
//! over a channel and an event on its loop; what it sees comes back over a
//! channel, and each report pokes morf's loop awake ([`morf_io::wake_all`]),
//! so a volume key's change reaches an on-screen display within a frame.

pub mod beat;
pub mod dsp;
pub mod fake;
mod model;
mod pipewire;
pub mod volume;

use std::collections::BTreeMap;
use std::sync::mpsc;

pub use model::{
    AudioState, Beat, Changes, Device, DeviceKind, Direction, Level, ObjectId, Stream, Tempo,
    Update,
};

/// Something asked of the server.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Linear gains per channel for a device or stream.
    SetVolumes {
        id: ObjectId,
        volumes: Vec<f32>,
    },
    SetMute {
        id: ObjectId,
        muted: bool,
    },
    /// Makes a device the default of its kind.
    SetDefault {
        id: ObjectId,
    },
    /// Sends a stream to another device.
    MoveStream {
        stream: ObjectId,
        device: ObjectId,
    },
    /// Starts metering a device — the default sink when `device` is none.
    /// A sink is metered by what it plays, a source by what it hears.
    StartMonitor {
        monitor: u64,
        device: Option<ObjectId>,
        rate_hz: f32,
        bands: usize,
        /// Listen for beats and a tempo as well.
        beat: bool,
    },
    StopMonitor {
        monitor: u64,
    },
}

/// Where a backend sends what it sees. Each report wakes morf's loop.
#[derive(Clone)]
pub struct Events {
    sender: mpsc::Sender<Update>,
}

impl Events {
    /// Reports one update; false once nobody is listening, which is the
    /// backend's cue to stop.
    pub fn send(&self, update: Update) -> bool {
        let sent = self.sender.send(update).is_ok();
        morf_io::wake_all();
        sent
    }
}

/// A sound server, before it is started.
pub trait Backend: Send + 'static {
    /// A name for logs: "pipewire", "fake".
    fn name(&self) -> &'static str;
    /// Starts reporting to `events` and returns the way to send it commands.
    fn start(self: Box<Self>, events: Events) -> Box<dyn Control>;
}

/// The way commands reach a started backend.
pub trait Control {
    fn send(&self, command: Command);
}

struct NoControl;

impl Control for NoControl {
    fn send(&self, _command: Command) {}
}

/// What one [`Audio::poll`] collected.
#[derive(Debug, Default)]
pub struct Poll {
    pub changes: Changes,
    /// The latest level of each monitor that measured one, loudest peak
    /// kept: several readings since the last poll are one, not a backlog.
    /// A silent reading last is kept as it is: the sound stopped.
    pub levels: Vec<Level>,
    /// Every beat heard since the last poll, oldest first.
    pub beats: Vec<Beat>,
    /// The latest tempo estimate of each monitor whose estimate moved.
    pub tempos: Vec<Tempo>,
    pub errors: Vec<String>,
}

/// The audio of the machine, as a shell sees it.
pub struct Audio {
    backend: &'static str,
    state: AudioState,
    updates: Option<mpsc::Receiver<Update>>,
    control: Box<dyn Control>,
    next_monitor: u64,
}

impl Audio {
    /// The machine's sound server, or an [`Audio`] that is never available
    /// when there is none to be had.
    pub fn connect() -> Self {
        match pipewire::PipeWire::open() {
            Some(backend) => Self::with_backend(backend),
            None => Self::unavailable(),
        }
    }

    /// Audio from a backend of the caller's choosing.
    pub fn with_backend(backend: impl Backend) -> Self {
        let (sender, receiver) = mpsc::channel();
        let name = backend.name();
        let control = Box::new(backend).start(Events { sender });
        Self {
            backend: name,
            state: AudioState::default(),
            updates: Some(receiver),
            control,
            next_monitor: 1,
        }
    }

    /// Audio with no server behind it: empty, and every command a no-op.
    pub fn unavailable() -> Self {
        Self {
            backend: "none",
            state: AudioState::default(),
            updates: None,
            control: Box::new(NoControl),
            next_monitor: 1,
        }
    }

    pub fn backend(&self) -> &'static str {
        self.backend
    }

    pub fn state(&self) -> &AudioState {
        &self.state
    }

    /// Takes in everything the backend reported since the last call.
    pub fn poll(&mut self) -> Poll {
        let mut poll = Poll::default();
        let Some(updates) = &self.updates else {
            return poll;
        };
        let mut levels: BTreeMap<u64, Level> = BTreeMap::new();
        let mut tempos: BTreeMap<u64, Tempo> = BTreeMap::new();
        while let Ok(update) = updates.try_recv() {
            match update {
                Update::Level(level) => match levels.get_mut(&level.monitor) {
                    // Silence last means the sound stopped: it wins, or a
                    // meter would hold the peak before it.
                    Some(kept) if level.left == 0.0 && level.right == 0.0 => *kept = level,
                    Some(kept) => {
                        kept.left = kept.left.max(level.left);
                        kept.right = kept.right.max(level.right);
                        kept.bands = level.bands;
                    }
                    None => {
                        levels.insert(level.monitor, level);
                    }
                },
                Update::Beat(beat) => poll.beats.push(beat),
                Update::Tempo(tempo) => {
                    tempos.insert(tempo.monitor, tempo);
                }
                Update::Error(message) => poll.errors.push(message),
                update => poll.changes.merge(self.state.apply(update)),
            }
        }
        poll.levels = levels.into_values().collect();
        poll.tempos = tempos.into_values().collect();
        poll
    }

    /// Sets a device's or stream's volume, as a person sees it (0 to
    /// [`volume::MAX_VOLUME`]), keeping its balance. False for an unknown id.
    pub fn set_volume(&self, id: ObjectId, volume: f32) -> bool {
        let Some(volumes) = self.state.volumes_for(id, volume) else {
            return false;
        };
        self.control.send(Command::SetVolumes { id, volumes });
        true
    }

    pub fn set_mute(&self, id: ObjectId, muted: bool) -> bool {
        if self.state.device(id).is_none() && self.state.stream(id).is_none() {
            return false;
        }
        self.control.send(Command::SetMute { id, muted });
        true
    }

    /// Makes a device the default of its kind. False for an unknown device.
    pub fn set_default(&self, id: ObjectId) -> bool {
        if self.state.device(id).is_none() {
            return false;
        }
        self.control.send(Command::SetDefault { id });
        true
    }

    /// Sends a stream to a device of the matching kind: playback to a sink,
    /// recording to a source.
    pub fn move_stream(&self, stream: ObjectId, device: ObjectId) -> bool {
        let (Some(found), Some(target)) = (self.state.stream(stream), self.state.device(device))
        else {
            return false;
        };
        let fits = matches!(
            (found.direction, target.kind),
            (Direction::Playback, DeviceKind::Sink) | (Direction::Record, DeviceKind::Source)
        );
        if fits {
            self.control.send(Command::MoveStream { stream, device });
        }
        fits
    }

    /// Starts metering; levels arrive through [`Audio::poll`] under the
    /// returned id. `rate_hz` is held to 1–120, `bands` to [`dsp::MAX_BANDS`].
    pub fn monitor(&mut self, device: Option<ObjectId>, rate_hz: f32, bands: usize) -> u64 {
        self.monitor_beats(device, rate_hz, bands, false)
    }

    /// [`Audio::monitor`], listening for beats too when `beat` is set: they
    /// and the tempo estimate arrive through [`Audio::poll`] as well. Off,
    /// the audio thread does no more than for a plain meter.
    pub fn monitor_beats(
        &mut self,
        device: Option<ObjectId>,
        rate_hz: f32,
        bands: usize,
        beat: bool,
    ) -> u64 {
        let monitor = self.next_monitor;
        self.next_monitor += 1;
        self.control.send(Command::StartMonitor {
            monitor,
            device,
            rate_hz: if rate_hz.is_finite() {
                rate_hz.clamp(1.0, 120.0)
            } else {
                30.0
            },
            bands: bands.min(dsp::MAX_BANDS),
            beat,
        });
        monitor
    }

    pub fn stop_monitor(&self, monitor: u64) {
        self.control.send(Command::StopMonitor { monitor });
    }
}

#[cfg(test)]
mod tests;
