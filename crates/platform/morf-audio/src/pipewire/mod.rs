//! The PipeWire backend: libpipewire, opened at run time, on its own thread.
//!
//! What it watches, and where each fact comes from:
//!
//! - sinks and sources are nodes of class `Audio/Sink` and `Audio/Source`;
//!   application streams are `Stream/Output/Audio` (playback) and
//!   `Stream/Input/Audio` (recording). Names come from each node's info
//!   properties, volumes and mutes from its `Props` parameter.
//! - which device a stream uses is whichever node it is linked to.
//! - the defaults are the `default.audio.sink` and `default.audio.source`
//!   keys of the `default` metadata, the same place WirePlumber keeps them.
//!
//! Changing things goes where the session manager expects it: a device with a
//! hardware route (an ALSA card's port) has its volume set on the route, so
//! the card's mixer moves and WirePlumber remembers it; anything else has its
//! node's `Props` set. A default is chosen by writing the configured default
//! to the metadata, and a stream is moved by giving it a target there — so
//! the session manager, not this client, decides how to relink, exactly as
//! when a mixer does it.
//!
//! Levels come from a capture stream per monitor: on a sink's monitor ports
//! for a sink, on the device for a source. It asks for stereo `f32` and is
//! marked passive, so it never keeps a device awake that nothing else is
//! using.

mod ffi;
pub(crate) mod pod;

mod callbacks;
mod control;
mod registry;
mod report;
mod thread;

use std::collections::{HashMap, VecDeque};
use std::ffi::{c_int, c_void};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::dsp::Meter;
use crate::{Backend, Command, Control, Events, MonitorDelay, Update};
use ffi::*;

use callbacks::{
    CORE_EVENTS, DEVICE_EVENTS, METADATA_EVENTS, NODE_EVENTS, REGISTRY_EVENTS, STREAM_EVENTS,
    on_command,
};
use thread::run;

/// How long to wait before trying a server that was not there again.
const RETRY: Duration = Duration::from_secs(3);

/// The property that marks this backend's own meter streams, so they are
/// not reported as an application recording.
const OWN_STREAM: &str = "morf.level-monitor";

pub(crate) struct PipeWire {
    pw: Arc<Pw>,
}

impl PipeWire {
    /// The backend, when libpipewire can be opened.
    ///
    /// The library is loaded once for the process and never unloaded:
    /// `pw_init` sets up global state inside that copy, so a second copy
    /// loaded after the first was dropped would start uninitialised and
    /// find no plugins at all.
    pub(crate) fn open() -> Option<Self> {
        static LIBRARY: std::sync::OnceLock<Option<Arc<Pw>>> = std::sync::OnceLock::new();
        LIBRARY
            .get_or_init(|| Pw::open().ok().map(Arc::new))
            .clone()
            .map(|pw| Self { pw })
    }
}

enum Message {
    Command(Command),
    Quit,
}

/// How another thread pokes the loop: its `signal_event`, on the event
/// source whose callback drains the command channel.
struct Waker {
    object: *mut c_void,
    signal: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
    source: *mut c_void,
}

// SAFETY: signalling a loop event is the one loop operation PipeWire makes
// safe from other threads, and the waker is cleared (under its lock) before
// the loop it belongs to is destroyed.
unsafe impl Send for Waker {}

type SharedWaker = Arc<Mutex<Option<Waker>>>;

fn wake(waker: &SharedWaker) {
    let waker = waker.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(waker) = &*waker {
        // SAFETY: see `Waker`.
        unsafe { (waker.signal)(waker.object, waker.source) };
    }
}

struct PipeWireControl {
    sender: Sender<Message>,
    waker: SharedWaker,
}

impl Control for PipeWireControl {
    fn send(&self, command: Command) {
        let _ = self.sender.send(Message::Command(command));
        wake(&self.waker);
    }
}

impl Drop for PipeWireControl {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Quit);
        wake(&self.waker);
    }
}

impl Backend for PipeWire {
    fn name(&self) -> &'static str {
        "pipewire"
    }

    fn start(self: Box<Self>, events: Events) -> Box<dyn Control> {
        let (sender, receiver) = mpsc::channel();
        let waker: SharedWaker = Arc::new(Mutex::new(None));
        let thread_waker = Arc::clone(&waker);
        let pw = self.pw;
        let spawned = std::thread::Builder::new()
            .name("morf-audio".into())
            .spawn(move || run(pw, events, receiver, thread_waker));
        if let Err(error) = spawned {
            eprintln!("morf-audio: cannot start the PipeWire thread: {error}");
        }
        Box::new(PipeWireControl { sender, waker })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Outcome {
    /// Asked to stop.
    Quit,
    /// Connected, then lost the server.
    Lost,
    /// Never reached a server.
    NoServer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Class {
    Sink,
    Source,
    Playback,
    Record,
}

impl Class {
    fn of(media_class: &str) -> Option<Self> {
        match media_class {
            "Audio/Sink" => Some(Self::Sink),
            "Audio/Source" | "Audio/Source/Virtual" => Some(Self::Source),
            "Stream/Output/Audio" => Some(Self::Playback),
            "Stream/Input/Audio" => Some(Self::Record),
            _ => None,
        }
    }

    fn is_device(self) -> bool {
        matches!(self, Self::Sink | Self::Source)
    }
}

/// A listener's registration and who it reports to. Boxed, so the hook the
/// library links into its lists never moves.
struct Listener {
    session: *mut Session,
    id: u32,
    hook: SpaHook,
}

impl Listener {
    fn new(session: *mut Session, id: u32) -> Box<Self> {
        Box::new(Self {
            session,
            id,
            hook: SpaHook::zeroed(),
        })
    }
}

struct Node {
    proxy: *mut c_void,
    listener: Box<Listener>,
    class: Class,
    serial: Option<String>,
    props: HashMap<String, String>,
    own: bool,
    have_info: bool,
    /// Reported once its first volumes arrived, or once the server has
    /// answered everything asked of it since it appeared, whichever is first.
    settled: bool,
    volumes: Vec<f32>,
    muted: bool,
    reported: Option<Update>,
    /// How long it takes to play what it is given, from its Latency param
    /// (the input side of a sink): 250 ms and more for a Bluetooth headset.
    latency: Duration,
}

struct Route {
    index: i32,
    direction: u32,
    device: i32,
}

struct DeviceObject {
    proxy: *mut c_void,
    _listener: Box<Listener>,
    icon_name: Option<String>,
    routes: Vec<Route>,
}

struct Metadata {
    id: u32,
    proxy: *mut c_void,
    _listener: Box<Listener>,
}

struct Monitor {
    pw: Arc<Pw>,
    events: Events,
    id: u64,
    stream: *mut c_void,
    hook: SpaHook,
    meter: Meter,
    /// The device it listens to, or none for the default output.
    device: Option<u32>,
    delay: MonitorDelay,
    /// What `delay` comes to now.
    hold: Duration,
    /// Measured, and waiting for its time.
    held: VecDeque<(Instant, Update)>,
}

impl Monitor {
    /// Hands `update` on when its delay has passed.
    fn emit(&mut self, update: Update) {
        if self.hold.is_zero() && self.held.is_empty() {
            self.events.send(update);
        } else {
            self.held.push_back((Instant::now() + self.hold, update));
        }
    }

    /// Hands on whatever has waited long enough. Samples arrive every
    /// quantum (a few milliseconds) while anything plays, which is the
    /// clock this runs on.
    fn release(&mut self) {
        let now = Instant::now();
        while self.held.front().is_some_and(|(due, _)| *due <= now) {
            if let Some((_, update)) = self.held.pop_front() {
                self.events.send(update);
            }
        }
    }
}

/// The band analysis looks at 2048 frames: at 48 kHz its picture is of
/// sound about 21 ms old already, which a device's delay need not repeat.
const ANALYSIS_LAG: Duration = Duration::from_millis(21);

struct Session {
    pw: Arc<Pw>,
    events: Events,
    receiver: Receiver<Message>,
    main_loop: *mut c_void,
    core: *mut c_void,
    registry: *mut c_void,
    core_hook: SpaHook,
    registry_hook: SpaHook,
    outcome: Outcome,
    pending_sync: Option<c_int>,
    /// Round trips completed before the first report: one for the globals,
    /// one for what binding them brought back.
    rounds: u8,
    ready: bool,
    nodes: HashMap<u32, Node>,
    devices: HashMap<u32, DeviceObject>,
    /// Each link's output node and input node.
    links: HashMap<u32, (u32, u32)>,
    metadata: Option<Metadata>,
    default_sink: Option<String>,
    default_source: Option<String>,
    reported_defaults: (Option<String>, Option<String>),
    monitors: HashMap<u64, Box<Monitor>>,
}

/// The `name` of a metadata value such as `{ "name": "alsa_output.pci" }`.
pub(crate) fn json_name(value: &str) -> Option<String> {
    let at = value.find("\"name\"")? + "\"name\"".len();
    let rest = value[at..].trim_start().strip_prefix(':')?.trim_start();
    let mut chars = rest.strip_prefix('"')?.chars();
    let mut name = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(name),
            '\\' => match chars.next()? {
                'n' => name.push('\n'),
                't' => name.push('\t'),
                'r' => name.push('\r'),
                'u' => {
                    let code: String = chars.by_ref().take(4).collect();
                    name.push(char::from_u32(u32::from_str_radix(&code, 16).ok()?)?);
                }
                other => name.push(other),
            },
            other => name.push(other),
        }
    }
}

/// A string as a JSON string literal.
pub(crate) fn json_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}
