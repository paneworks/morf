//! A sound server that exists only in memory, for tests.
//!
//! It answers commands the way a real server would — a volume set comes back
//! as the device with its new volume, a default chosen comes back as the new
//! default — so a test drives the same round trip a shell does, and can also
//! read every command it was sent.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::beat::BeatEvent;
use crate::dsp::Meter;
use crate::{
    AudioState, Backend, Beat, Command, Control, DeviceKind, Events, Level, ObjectId, Stream,
    Tempo, Update,
};

#[derive(Default)]
struct Server {
    state: AudioState,
    seed: Vec<Update>,
    events: Option<Events>,
    commands: Vec<Command>,
    monitors: Vec<u64>,
    /// A meter per running monitor, as the real server runs, for
    /// [`FakeServer::play`].
    meters: BTreeMap<u64, Meter>,
    echo: bool,
}

impl Server {
    fn send(&mut self, update: Update) {
        self.state.apply(update.clone());
        if let Some(events) = &self.events {
            events.send(update);
        }
    }
}

/// The backend half, handed to [`crate::Audio::with_backend`].
pub struct FakeBackend {
    server: Arc<Mutex<Server>>,
}

/// The test's half: what the server says, and what it was told.
#[derive(Clone)]
pub struct FakeServer {
    server: Arc<Mutex<Server>>,
}

/// A fake server that reports `seed` (after `Available(true)`) the moment it
/// is started, and answers commands.
pub fn fake(seed: Vec<Update>) -> (FakeBackend, FakeServer) {
    let server = Arc::new(Mutex::new(Server {
        seed,
        echo: true,
        ..Server::default()
    }));
    (
        FakeBackend {
            server: Arc::clone(&server),
        },
        FakeServer { server },
    )
}

fn lock(server: &Mutex<Server>) -> MutexGuard<'_, Server> {
    server.lock().unwrap_or_else(|error| error.into_inner())
}

impl FakeServer {
    /// Reports something, as the server would on its own.
    pub fn push(&self, update: Update) {
        lock(&self.server).send(update);
    }

    /// Every command received so far, oldest first.
    pub fn commands(&self) -> Vec<Command> {
        lock(&self.server).commands.clone()
    }

    /// Monitors currently running.
    pub fn monitors(&self) -> Vec<u64> {
        lock(&self.server).monitors.clone()
    }

    /// Whether commands are answered; on by default. Off, they are only
    /// recorded, like a server that has not got round to it yet.
    pub fn set_echo(&self, echo: bool) {
        lock(&self.server).echo = echo;
    }

    /// Plays interleaved samples to every running monitor, through the same
    /// meter the real server runs, in buffers of 1024 frames: levels, and
    /// beats for a monitor that asked, come back as they would from it.
    pub fn play(&self, rate: u32, channels: u32, samples: &[f32]) {
        let mut server = lock(&self.server);
        let mut updates = Vec::new();
        for (&monitor, meter) in &mut server.meters {
            if meter.rate() != rate || meter.channels() != channels.max(1) {
                meter.set_format(rate, channels);
            }
            for chunk in samples.chunks(1024 * channels.max(1) as usize) {
                if let Some(reading) = meter.push(chunk) {
                    updates.push(Update::Level(Level {
                        monitor,
                        left: reading.left,
                        right: reading.right,
                        bands: reading.bands,
                    }));
                }
                for event in meter.beats() {
                    updates.push(match event {
                        BeatEvent::Beat { strength } => Update::Beat(Beat { monitor, strength }),
                        BeatEvent::Tempo { bpm, confidence } => Update::Tempo(Tempo {
                            monitor,
                            bpm,
                            confidence,
                        }),
                    });
                }
            }
        }
        for update in updates {
            server.send(update);
        }
    }

    /// Delivers a level to every running monitor.
    pub fn level(&self, left: f32, right: f32, bands: Vec<f32>) {
        let mut server = lock(&self.server);
        for monitor in server.monitors.clone() {
            server.send(Update::Level(Level {
                monitor,
                left,
                right,
                bands: bands.clone(),
            }));
        }
    }
}

impl Backend for FakeBackend {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn start(self: Box<Self>, events: Events) -> Box<dyn Control> {
        let mut server = lock(&self.server);
        server.events = Some(events);
        server.send(Update::Available(true));
        for update in std::mem::take(&mut server.seed) {
            server.send(update);
        }
        drop(server);
        Box::new(FakeControl {
            server: self.server,
        })
    }
}

struct FakeControl {
    server: Arc<Mutex<Server>>,
}

impl Control for FakeControl {
    fn send(&self, command: Command) {
        let mut server = lock(&self.server);
        server.commands.push(command.clone());
        match &command {
            Command::StartMonitor {
                monitor,
                rate_hz,
                bands,
                beat,
                ..
            } => {
                server.monitors.push(*monitor);
                let mut meter = Meter::new(*rate_hz, *bands).with_beats(*beat);
                meter.set_format(48_000, 2);
                server.meters.insert(*monitor, meter);
            }
            Command::StopMonitor { monitor } => {
                server.monitors.retain(|id| id != monitor);
                server.meters.remove(monitor);
            }
            _ => {}
        }
        if !server.echo {
            return;
        }
        let answer = answer(&server.state, command);
        for update in answer {
            server.send(update);
        }
    }
}

/// What a server would report after carrying a command out.
fn answer(state: &AudioState, command: Command) -> Vec<Update> {
    let with_stream = |id: ObjectId, change: &dyn Fn(&mut Stream)| {
        state.stream(id).cloned().map(|mut stream| {
            change(&mut stream);
            Update::Stream(stream)
        })
    };
    match command {
        Command::SetVolumes { id, volumes } => {
            if let Some(device) = state.device(id) {
                let mut device = device.clone();
                device.channel_volumes = volumes;
                vec![Update::Device(device)]
            } else {
                with_stream(id, &|stream| stream.channel_volumes = volumes.clone())
                    .into_iter()
                    .collect()
            }
        }
        Command::SetMute { id, muted } => {
            if let Some(device) = state.device(id) {
                let mut device = device.clone();
                device.muted = muted;
                vec![Update::Device(device)]
            } else {
                with_stream(id, &|stream| stream.muted = muted)
                    .into_iter()
                    .collect()
            }
        }
        Command::SetDefault { id } => match state.device(id) {
            Some(device) if device.kind == DeviceKind::Sink => {
                vec![Update::DefaultSink(Some(device.name.clone()))]
            }
            Some(device) => vec![Update::DefaultSource(Some(device.name.clone()))],
            None => Vec::new(),
        },
        Command::MoveStream { stream, device } => {
            with_stream(stream, &|stream| stream.device = Some(device))
                .into_iter()
                .collect()
        }
        Command::StartMonitor { .. } | Command::StopMonitor { .. } => Vec::new(),
    }
}
