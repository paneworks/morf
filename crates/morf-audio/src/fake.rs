//! A sound server that exists only in memory, for tests.
//!
//! It answers commands the way a real server would — a volume set comes back
//! as the device with its new volume, a default chosen comes back as the new
//! default — so a test drives the same round trip a shell does, and can also
//! read every command it was sent.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::{
    AudioState, Backend, Command, Control, DeviceKind, Events, Level, ObjectId, Stream, Update,
};

#[derive(Default)]
struct Server {
    state: AudioState,
    seed: Vec<Update>,
    events: Option<Events>,
    commands: Vec<Command>,
    monitors: Vec<u64>,
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
            Command::StartMonitor { monitor, .. } => server.monitors.push(*monitor),
            Command::StopMonitor { monitor } => server.monitors.retain(|id| id != monitor),
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
