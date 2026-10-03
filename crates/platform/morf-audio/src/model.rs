//! What the sound server has, as this side of the channel knows it.
//!
//! A backend reports what it sees as [`Update`]s; [`AudioState`] applies them
//! in order and says what moved. Nothing here knows which server sent them,
//! so the bookkeeping — which device is the default, which stream plays
//! where, what a device's volume reads as — is tested once, against a fake,
//! for every backend there is or will be.

use std::collections::BTreeMap;

use crate::volume;

/// A server's name for one device or stream. Unique while it exists; a
/// server may hand the number to something else once it is gone.
pub type ObjectId = u32;

/// Which way sound goes through a device.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeviceKind {
    /// Somewhere sound goes to be heard: speakers, headphones, an HDMI port.
    Sink,
    /// Somewhere sound comes from: a microphone, a line in.
    Source,
}

impl DeviceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sink => "sink",
            Self::Source => "source",
        }
    }
}

/// Which way an application's stream goes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Direction {
    /// An application playing sound into a sink.
    Playback,
    /// An application recording from a source.
    Record,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Record => "record",
        }
    }
}

/// One sink or source.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: ObjectId,
    /// The server's stable name for it, the one a default is remembered by.
    pub name: String,
    /// What a person would call it.
    pub description: String,
    pub kind: DeviceKind,
    /// Linear gain per channel; see [`crate::volume`].
    pub channel_volumes: Vec<f32>,
    pub muted: bool,
    pub icon_name: Option<String>,
}

impl Device {
    /// The volume a person sees, averaged over channels.
    pub fn volume(&self) -> f32 {
        volume::average(&self.channel_volumes)
    }

    /// How many channels the device has; at least one.
    pub fn channels(&self) -> usize {
        self.channel_volumes.len().max(1)
    }
}

/// One application's playback or recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub id: ObjectId,
    /// The application's own name for itself.
    pub app_name: String,
    /// An identifier for the application — a desktop id or a portal app id —
    /// when it gave one.
    pub app_id: Option<String>,
    /// The executable's name.
    pub binary: Option<String>,
    pub icon_name: Option<String>,
    /// What is playing: a track title, a call, a tab.
    pub media_name: Option<String>,
    pub direction: Direction,
    /// The device it plays to or records from, once the server has placed it.
    pub device: Option<ObjectId>,
    /// Linear gain per channel.
    pub channel_volumes: Vec<f32>,
    pub muted: bool,
    pub pid: Option<u32>,
}

impl Stream {
    pub fn volume(&self) -> f32 {
        volume::average(&self.channel_volumes)
    }

    pub fn channels(&self) -> usize {
        self.channel_volumes.len().max(1)
    }
}

/// One thing a backend saw.
#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    /// The server became reachable (with everything it has already
    /// reported), or stopped being. Going away forgets every device and
    /// stream: whatever comes back will announce itself again.
    Available(bool),
    /// A device appeared or changed; the whole of it, not a difference.
    Device(Device),
    DeviceRemoved(ObjectId),
    Stream(Stream),
    StreamRemoved(ObjectId),
    /// The default sink, by name, or none.
    DefaultSink(Option<String>),
    DefaultSource(Option<String>),
    /// Levels measured by a running monitor.
    Level(Level),
    /// A beat heard by a monitor that listens for them.
    Beat(Beat),
    /// Its tempo estimate, as it moves.
    Tempo(Tempo),
    /// Something went wrong that a configuration may want to hear about.
    Error(String),
}

/// One measurement from a monitor.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// Which monitor measured it.
    pub monitor: u64,
    /// The loudest sample of each side since the last measurement, 0 to 1.
    pub left: f32,
    pub right: f32,
    /// Band energies, 0 to 1, lowest frequency first; empty unless asked.
    pub bands: Vec<f32>,
}

/// A beat, the moment it was heard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub monitor: u64,
    /// How strong, 0 to 1, against the beats of the last few seconds.
    pub strength: f32,
}

/// A monitor's tempo estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tempo {
    pub monitor: u64,
    pub bpm: f32,
    /// How regular the beats are at that tempo, 0 to 1.
    pub confidence: f32,
}

/// What an update, or a run of them, changed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Changes {
    pub available: bool,
    pub devices: bool,
    pub streams: bool,
    pub defaults: bool,
}

impl Changes {
    pub fn any(&self) -> bool {
        self.available || self.devices || self.streams || self.defaults
    }

    pub fn merge(&mut self, other: Changes) {
        self.available |= other.available;
        self.devices |= other.devices;
        self.streams |= other.streams;
        self.defaults |= other.defaults;
    }
}

/// Everything known about the server.
#[derive(Clone, Debug, Default)]
pub struct AudioState {
    available: bool,
    devices: BTreeMap<ObjectId, Device>,
    streams: BTreeMap<ObjectId, Stream>,
    default_sink: Option<String>,
    default_source: Option<String>,
}

impl AudioState {
    /// Applies one update and says what it changed. Levels and errors change
    /// nothing here; the caller routes them.
    pub fn apply(&mut self, update: Update) -> Changes {
        let mut changes = Changes::default();
        match update {
            Update::Available(available) => {
                changes.available = self.available != available;
                self.available = available;
                if !available {
                    changes.devices = !self.devices.is_empty();
                    changes.streams = !self.streams.is_empty();
                    changes.defaults = self.default_sink.is_some() || self.default_source.is_some();
                    self.devices.clear();
                    self.streams.clear();
                    self.default_sink = None;
                    self.default_source = None;
                }
            }
            Update::Device(device) => {
                let before = self.devices.get(&device.id);
                if before != Some(&device) {
                    // A device that appears, or is renamed, may become or stop
                    // being a default without the default itself changing.
                    let was_default = before.is_some_and(|before| self.is_named_default(before));
                    changes.defaults = was_default != self.is_named_default(&device);
                    changes.devices = true;
                    self.devices.insert(device.id, device);
                }
            }
            Update::DeviceRemoved(id) => {
                if let Some(device) = self.devices.remove(&id) {
                    changes.devices = true;
                    changes.defaults = self.is_named_default(&device);
                }
            }
            Update::Stream(stream) => {
                if self.streams.get(&stream.id) != Some(&stream) {
                    changes.streams = true;
                    self.streams.insert(stream.id, stream);
                }
            }
            Update::StreamRemoved(id) => {
                changes.streams = self.streams.remove(&id).is_some();
            }
            Update::DefaultSink(name) => {
                changes.defaults = self.default_sink != name;
                self.default_sink = name;
            }
            Update::DefaultSource(name) => {
                changes.defaults = self.default_source != name;
                self.default_source = name;
            }
            Update::Level(_) | Update::Beat(_) | Update::Tempo(_) | Update::Error(_) => {}
        }
        changes
    }

    fn is_named_default(&self, device: &Device) -> bool {
        let default = match device.kind {
            DeviceKind::Sink => &self.default_sink,
            DeviceKind::Source => &self.default_source,
        };
        default.as_deref() == Some(device.name.as_str())
    }

    pub fn available(&self) -> bool {
        self.available
    }

    pub fn device(&self, id: ObjectId) -> Option<&Device> {
        self.devices.get(&id)
    }

    pub fn stream(&self, id: ObjectId) -> Option<&Stream> {
        self.streams.get(&id)
    }

    /// Every device of one kind, in the order the server made them.
    pub fn devices(&self, kind: DeviceKind) -> impl Iterator<Item = &Device> {
        self.devices
            .values()
            .filter(move |device| device.kind == kind)
    }

    pub fn streams(&self) -> impl Iterator<Item = &Stream> {
        self.streams.values()
    }

    /// The default device of one kind, when the server named one that exists.
    pub fn default_device(&self, kind: DeviceKind) -> Option<&Device> {
        let name = match kind {
            DeviceKind::Sink => self.default_sink.as_deref(),
            DeviceKind::Source => self.default_source.as_deref(),
        }?;
        self.devices(kind).find(|device| device.name == name)
    }

    /// Whether this device is the default of its kind.
    pub fn is_default(&self, id: ObjectId) -> bool {
        self.devices
            .get(&id)
            .is_some_and(|device| self.is_named_default(device))
    }

    /// Per-channel gains that set a device's or stream's volume, keeping its
    /// balance. `None` for an id that is neither.
    pub fn volumes_for(&self, id: ObjectId, volume: f32) -> Option<Vec<f32>> {
        let gains = match (self.devices.get(&id), self.streams.get(&id)) {
            (Some(device), _) => &device.channel_volumes,
            (None, Some(stream)) => &stream.channel_volumes,
            (None, None) => return None,
        };
        Some(volume::scale(gains, volume))
    }
}
