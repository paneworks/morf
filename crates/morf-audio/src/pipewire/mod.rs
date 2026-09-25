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

use std::collections::HashMap;
use std::ffi::{CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use crate::beat::BeatEvent;
use crate::dsp::Meter;
use crate::{
    Backend, Beat, Command, Control, Device, DeviceKind, Direction, Events, Level, ObjectId,
    Stream, Tempo, Update,
};
use ffi::*;
use pod::Pod;

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

/// Tells libpipewire where its plugins and modules are when nothing else
/// has: beside the library that was actually loaded.
///
/// A libpipewire found through a library path that is not its build's own
/// prefix -- a Nix profile, a bundle -- knows no plugin directory, and fails
/// with "plugin directory undefined" and no sound at all. The directories
/// sit next to the library in every layout PipeWire installs, so they are
/// found from where `dladdr` says `pw_init` lives. An environment that
/// already names them is left alone.
fn point_at_plugins(pw: &Pw) {
    let Some(library_dir) = library_dir_of(pw.init as *const std::ffi::c_void) else {
        return;
    };
    for (variable, folder) in [
        ("SPA_PLUGIN_DIR", "spa-0.2"),
        ("PIPEWIRE_MODULE_DIR", "pipewire-0.3"),
    ] {
        if std::env::var_os(variable).is_some() {
            continue;
        }
        let candidate = library_dir.join(folder);
        if candidate.is_dir() {
            // SAFETY: set once, from the audio thread, before libpipewire
            // reads it; nothing else in morf reads these variables, and
            // this runs before any PipeWire thread exists.
            unsafe { std::env::set_var(variable, &candidate) };
        }
    }
}

/// The directory of the shared object containing `address`.
fn library_dir_of(address: *const std::ffi::c_void) -> Option<std::path::PathBuf> {
    // SAFETY: dladdr only reads the loader's tables; `info` is written by it.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    if unsafe { libc::dladdr(address, &mut info) } == 0 || info.dli_fname.is_null() {
        return None;
    }
    // SAFETY: dli_fname is a NUL-terminated path owned by the loader.
    let path = unsafe { std::ffi::CStr::from_ptr(info.dli_fname) };
    let path = std::path::Path::new(
        <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(path.to_bytes()),
    );
    std::fs::canonicalize(path)
        .ok()?
        .parent()
        .map(std::path::Path::to_path_buf)
}

fn run(pw: Arc<Pw>, events: Events, mut receiver: Receiver<Message>, waker: SharedWaker) {
    static INIT: Once = Once::new();
    // SAFETY: pw_init takes optional argc/argv and is safe to call with none.
    INIT.call_once(|| {
        point_at_plugins(&pw);
        // SAFETY: pw_init takes optional argc/argv and is safe to call with none.
        unsafe { (pw.init)(ptr::null_mut(), ptr::null_mut()) }
    });
    loop {
        let (outcome, back) = session(&pw, &events, receiver, &waker);
        receiver = back;
        match outcome {
            Outcome::Quit => return,
            Outcome::Lost => {
                if !events.send(Update::Available(false)) {
                    return;
                }
            }
            Outcome::NoServer => {}
        }
        // Commands sent while there is no server name objects that will not
        // exist on the next one; they are dropped.
        let deadline = Instant::now() + RETRY;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(left) {
                Ok(Message::Quit) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Message::Command(_)) => {}
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
    }
}

/// One connection, from connect to loss or quit.
fn session(
    pw: &Arc<Pw>,
    events: &Events,
    receiver: Receiver<Message>,
    waker: &SharedWaker,
) -> (Outcome, Receiver<Message>) {
    // SAFETY: every pointer below comes from libpipewire and is used on this
    // thread only, and torn down in the reverse order it was made.
    unsafe {
        let main_loop = (pw.main_loop_new)(ptr::null());
        if main_loop.is_null() {
            return (Outcome::NoServer, receiver);
        }
        let pw_loop = (pw.main_loop_get_loop)(main_loop);
        let context = (pw.context_new)(pw_loop, ptr::null_mut(), 0);
        if context.is_null() {
            (pw.main_loop_destroy)(main_loop);
            return (Outcome::NoServer, receiver);
        }
        let core = (pw.context_connect)(context, ptr::null_mut(), 0);
        if core.is_null() {
            (pw.context_destroy)(context);
            (pw.main_loop_destroy)(main_loop);
            return (Outcome::NoServer, receiver);
        }
        let session = Box::into_raw(Box::new(Session {
            pw: Arc::clone(pw),
            events: events.clone(),
            receiver,
            main_loop,
            core,
            registry: ptr::null_mut(),
            core_hook: SpaHook::zeroed(),
            registry_hook: SpaHook::zeroed(),
            outcome: Outcome::Quit,
            pending_sync: None,
            rounds: 0,
            ready: false,
            nodes: HashMap::new(),
            devices: HashMap::new(),
            links: HashMap::new(),
            metadata: None,
            default_sink: None,
            default_source: None,
            reported_defaults: (None, None),
            monitors: HashMap::new(),
        }));
        let data = session.cast::<c_void>();
        let mut source = ptr::null_mut();
        let mut utils = None;
        if let Some((methods, object)) = methods::<CoreMethods>(core) {
            if let Some(add_listener) = methods.add_listener {
                add_listener(object, &raw mut (*session).core_hook, &CORE_EVENTS, data);
            }
            if let Some(get_registry) = methods.get_registry {
                (*session).registry = get_registry(object, VERSION_REGISTRY, 0);
            }
        }
        if let Some((methods, object)) = methods::<RegistryMethods>((*session).registry)
            && let Some(add_listener) = methods.add_listener
        {
            add_listener(
                object,
                &raw mut (*session).registry_hook,
                &REGISTRY_EVENTS,
                data,
            );
        }
        (*session).sync();
        if let Some((methods, object)) = methods::<LoopUtilsMethods>((*pw_loop).utils.cast())
            && let (Some(add_event), Some(signal)) = (methods.add_event, methods.signal_event)
        {
            source = add_event(object, Some(on_command), data);
            utils = Some((methods, object));
            if !source.is_null() {
                *waker.lock().unwrap_or_else(|error| error.into_inner()) = Some(Waker {
                    object,
                    signal,
                    source,
                });
                // Anything sent before the waker existed is waiting.
                signal(object, source);
            }
        }

        (pw.main_loop_run)(main_loop);

        *waker.lock().unwrap_or_else(|error| error.into_inner()) = None;
        if let Some((methods, object)) = utils
            && let Some(destroy) = methods.destroy_source
            && !source.is_null()
        {
            destroy(object, source);
        }
        for (_, monitor) in (*session).monitors.drain() {
            (pw.stream_destroy)(monitor.stream);
        }
        // Destroys every proxy, which unhooks every listener, so the boxes
        // the hooks live in can go after it.
        (pw.core_disconnect)(core);
        let session = Box::from_raw(session);
        let outcome = session.outcome;
        let Session { receiver, .. } = *session;
        (pw.context_destroy)(context);
        (pw.main_loop_destroy)(main_loop);
        (outcome, receiver)
    }
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
}

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

static CORE_EVENTS: CoreEvents = CoreEvents {
    version: 0,
    info: None,
    done: Some(on_core_done),
    ping: None,
    error: Some(on_core_error),
    remove_id: None,
    bound_id: None,
    add_mem: None,
    remove_mem: None,
};

static REGISTRY_EVENTS: RegistryEvents = RegistryEvents {
    version: 0,
    global: Some(on_global),
    global_remove: Some(on_global_remove),
};

static NODE_EVENTS: NodeEvents = NodeEvents {
    version: 0,
    info: Some(on_node_info),
    param: Some(on_node_param),
};

static DEVICE_EVENTS: DeviceEvents = DeviceEvents {
    version: 0,
    info: None,
    param: Some(on_device_param),
};

static METADATA_EVENTS: MetadataEvents = MetadataEvents {
    version: 0,
    property: Some(on_metadata_property),
};

static STREAM_EVENTS: StreamEvents = StreamEvents {
    version: 0,
    destroy: None,
    state_changed: Some(on_stream_state),
    control_info: None,
    io_changed: None,
    param_changed: Some(on_stream_param),
    add_buffer: None,
    remove_buffer: None,
    process: Some(on_stream_process),
    drained: None,
};

// The callbacks. Each finds the session through its data pointer and hands
// over to a method; the library calls them only from `pw_main_loop_run`,
// on this backend's thread, while nothing else holds the session.

unsafe extern "C" fn on_core_done(data: *mut c_void, id: u32, seq: c_int) {
    // SAFETY: registered with the session as data.
    let session = unsafe { &mut *data.cast::<Session>() };
    if id == PW_ID_CORE {
        session.done(seq);
    }
}

unsafe extern "C" fn on_core_error(
    data: *mut c_void,
    id: u32,
    _seq: c_int,
    res: c_int,
    message: *const c_char,
) {
    // SAFETY: registered with the session as data; the message is a C string.
    let (session, message) = unsafe { (&mut *data.cast::<Session>(), string(message)) };
    if id != PW_ID_CORE {
        return;
    }
    if res == -libc::EPIPE {
        session.outcome = Outcome::Lost;
        // SAFETY: the loop is running on this thread.
        unsafe { (session.pw.main_loop_quit)(session.main_loop) };
    } else {
        session.events.send(Update::Error(format!(
            "PipeWire: {}",
            message.unwrap_or_else(|| format!("error {res}"))
        )));
    }
}

unsafe extern "C" fn on_global(
    data: *mut c_void,
    id: u32,
    _permissions: u32,
    kind: *const c_char,
    _version: u32,
    props: *const SpaDict,
) {
    // SAFETY: registered with the session as data; the strings and dict are
    // the library's, valid for this call.
    unsafe {
        let session = data.cast::<Session>();
        let (Some(kind), props) = (string(kind), dict_entries(props)) else {
            return;
        };
        let props: HashMap<String, String> = props.into_iter().collect();
        Session::global(session, id, &kind, props);
    }
}

unsafe extern "C" fn on_global_remove(data: *mut c_void, id: u32) {
    // SAFETY: registered with the session as data.
    unsafe { (*data.cast::<Session>()).global_remove(id) };
}

unsafe extern "C" fn on_node_info(data: *mut c_void, info: *const NodeInfo) {
    // SAFETY: registered with a listener as data; the info is the library's.
    unsafe {
        let listener = data.cast::<Listener>();
        let (session, id) = ((*listener).session, (*listener).id);
        if info.is_null() {
            return;
        }
        let props = dict_entries((*info).props);
        (*session).node_info(id, props);
    }
}

unsafe extern "C" fn on_node_param(
    data: *mut c_void,
    _seq: c_int,
    param: u32,
    _index: u32,
    _next: u32,
    value: *const SpaPod,
) {
    // SAFETY: as for `on_node_info`; the POD is the library's.
    unsafe {
        let listener = data.cast::<Listener>();
        let (session, id) = ((*listener).session, (*listener).id);
        if param == PARAM_PROPS
            && let Some(value) = pod::read(value)
        {
            (*session).node_props(id, &value);
        }
    }
}

unsafe extern "C" fn on_device_param(
    data: *mut c_void,
    _seq: c_int,
    param: u32,
    _index: u32,
    _next: u32,
    value: *const SpaPod,
) {
    // SAFETY: as for `on_node_param`.
    unsafe {
        let listener = data.cast::<Listener>();
        let (session, id) = ((*listener).session, (*listener).id);
        if param == PARAM_ROUTE
            && let Some(value) = pod::read(value)
        {
            (*session).device_route(id, &value);
        }
    }
}

unsafe extern "C" fn on_metadata_property(
    data: *mut c_void,
    subject: u32,
    key: *const c_char,
    _kind: *const c_char,
    value: *const c_char,
) -> c_int {
    // SAFETY: as for `on_node_info`; the strings are the library's.
    unsafe {
        let listener = data.cast::<Listener>();
        let session = (*listener).session;
        (*session).metadata_property(subject, string(key), string(value));
    }
    0
}

unsafe extern "C" fn on_command(data: *mut c_void, _count: u64) {
    // SAFETY: registered with the session as data.
    let session = unsafe { &mut *data.cast::<Session>() };
    while let Ok(message) = session.receiver.try_recv() {
        match message {
            Message::Quit => {
                session.outcome = Outcome::Quit;
                // SAFETY: the loop is running on this thread.
                unsafe { (session.pw.main_loop_quit)(session.main_loop) };
                return;
            }
            Message::Command(command) => session.command(command),
        }
    }
}

unsafe extern "C" fn on_stream_state(
    data: *mut c_void,
    old: c_int,
    state: c_int,
    error: *const c_char,
) {
    // SAFETY: registered with the monitor as data.
    let monitor = unsafe { &mut *data.cast::<Monitor>() };
    // A device with nothing playing goes idle and the stream is paused: no
    // more samples, so no more readings. Say silence once, or a meter would
    // hold whatever it last showed.
    if old == STREAM_STATE_STREAMING && state != STREAM_STATE_STREAMING {
        let reading = monitor.meter.silence();
        monitor.events.send(Update::Level(Level {
            monitor: monitor.id,
            left: reading.left,
            right: reading.right,
            bands: reading.bands,
        }));
    }
    if state == STREAM_STATE_ERROR {
        // SAFETY: a C string or null.
        let error = unsafe { string(error) }.unwrap_or_default();
        monitor
            .events
            .send(Update::Error(format!("level monitor: {error}")));
    }
}

unsafe extern "C" fn on_stream_param(data: *mut c_void, id: u32, value: *const SpaPod) {
    // SAFETY: registered with the monitor as data; the POD is the library's.
    let (monitor, value) = unsafe { (&mut *data.cast::<Monitor>(), pod::read(value)) };
    if id != PARAM_FORMAT {
        return;
    }
    let Some(format) = value else {
        return;
    };
    let rate = format
        .property(pod::FORMAT_AUDIO_RATE)
        .and_then(Pod::as_int)
        .unwrap_or(48_000);
    let channels = format
        .property(pod::FORMAT_AUDIO_CHANNELS)
        .and_then(Pod::as_int)
        .unwrap_or(2);
    monitor
        .meter
        .set_format(rate.max(1) as u32, channels.max(1) as u32);
}

/// Reports what the meter's beat tracker heard, if it has one.
fn send_beats(monitor: &mut Monitor) {
    let id = monitor.id;
    for event in monitor.meter.beats() {
        monitor.events.send(match event {
            BeatEvent::Beat { strength } => Update::Beat(Beat {
                monitor: id,
                strength,
            }),
            BeatEvent::Tempo { bpm, confidence } => Update::Tempo(Tempo {
                monitor: id,
                bpm,
                confidence,
            }),
        });
    }
}

unsafe extern "C" fn on_stream_process(data: *mut c_void) {
    // SAFETY: registered with the monitor as data; the buffer and its memory
    // are the stream's until queued back, and mapped (MAP_BUFFERS).
    unsafe {
        let monitor = &mut *data.cast::<Monitor>();
        let buffer = (monitor.pw.stream_dequeue_buffer)(monitor.stream);
        if buffer.is_null() {
            return;
        }
        let spa = (*buffer).buffer;
        if !spa.is_null() && (*spa).n_datas > 0 && !(*spa).datas.is_null() {
            let first = &*(*spa).datas;
            if !first.data.is_null() && !first.chunk.is_null() && first.maxsize > 0 {
                let chunk = &*first.chunk;
                let offset = chunk.offset % first.maxsize;
                let size = chunk.size.min(first.maxsize - offset);
                let start = first.data.cast::<u8>().add(offset as usize);
                if start.align_offset(std::mem::align_of::<f32>()) == 0 {
                    let samples =
                        std::slice::from_raw_parts(start.cast::<f32>(), size as usize / 4);
                    if let Some(reading) = monitor.meter.push(samples) {
                        monitor.events.send(Update::Level(Level {
                            monitor: monitor.id,
                            left: reading.left,
                            right: reading.right,
                            bands: reading.bands,
                        }));
                    }
                    send_beats(monitor);
                }
            }
        }
        (monitor.pw.stream_queue_buffer)(monitor.stream, buffer);
    }
}

impl Session {
    fn sync(&mut self) {
        // SAFETY: the core is live for the session.
        unsafe {
            if let Some((methods, object)) = methods::<CoreMethods>(self.core)
                && let Some(sync) = methods.sync
            {
                self.pending_sync = Some(sync(object, PW_ID_CORE, 0));
            }
        }
    }

    fn done(&mut self, seq: c_int) {
        if self.pending_sync != Some(seq) {
            return;
        }
        self.pending_sync = None;
        if !self.ready {
            self.rounds += 1;
            if self.rounds < 2 {
                self.sync();
                return;
            }
            self.ready = true;
            self.events.send(Update::Available(true));
            self.report_defaults();
        }
        let ids: Vec<u32> = self.nodes.keys().copied().collect();
        for id in ids {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.settled = true;
            }
            self.report(id);
        }
    }

    /// A new global. Takes the session as a pointer, because binding hands
    /// the library pointers into it.
    ///
    /// # Safety
    /// `session` is the live session, and nothing else borrows it.
    unsafe fn global(session: *mut Session, id: u32, kind: &str, props: HashMap<String, String>) {
        // SAFETY: promised by the caller.
        let this = unsafe { &mut *session };
        match kind {
            TYPE_NODE => {
                let Some(class) = props.get("media.class").and_then(|class| Class::of(class))
                else {
                    return;
                };
                let mut listener = Listener::new(session, id);
                // SAFETY: the registry is live; the listener is boxed.
                let proxy = unsafe {
                    this.bind(
                        id,
                        TYPE_NODE,
                        VERSION_NODE,
                        &mut listener,
                        |methods, object, hook, data| {
                            let methods = &*(methods as *const ParamMethods<NodeEvents>);
                            if let Some(add_listener) = methods.add_listener {
                                add_listener(object, hook, &NODE_EVENTS, data);
                            }
                            if let Some(subscribe) = methods.subscribe_params {
                                let mut ids = [PARAM_PROPS];
                                subscribe(object, ids.as_mut_ptr(), 1);
                            }
                        },
                    )
                };
                let Some(proxy) = proxy else {
                    return;
                };
                this.nodes.insert(
                    id,
                    Node {
                        proxy,
                        listener,
                        class,
                        serial: props.get("object.serial").cloned(),
                        props,
                        own: false,
                        have_info: false,
                        settled: false,
                        volumes: Vec::new(),
                        muted: false,
                        reported: None,
                    },
                );
                if this.ready && this.pending_sync.is_none() {
                    this.sync();
                }
            }
            TYPE_DEVICE => {
                if props.get("media.class").map(String::as_str) != Some("Audio/Device") {
                    return;
                }
                let mut listener = Listener::new(session, id);
                // SAFETY: as above.
                let proxy = unsafe {
                    this.bind(
                        id,
                        TYPE_DEVICE,
                        VERSION_DEVICE,
                        &mut listener,
                        |methods, object, hook, data| {
                            let methods = &*(methods as *const ParamMethods<DeviceEvents>);
                            if let Some(add_listener) = methods.add_listener {
                                add_listener(object, hook, &DEVICE_EVENTS, data);
                            }
                            if let Some(subscribe) = methods.subscribe_params {
                                let mut ids = [PARAM_ROUTE];
                                subscribe(object, ids.as_mut_ptr(), 1);
                            }
                        },
                    )
                };
                if let Some(proxy) = proxy {
                    this.devices.insert(
                        id,
                        DeviceObject {
                            proxy,
                            _listener: listener,
                            icon_name: props.get("device.icon-name").cloned(),
                            routes: Vec::new(),
                        },
                    );
                }
            }
            TYPE_LINK => {
                let node = |key: &str| props.get(key).and_then(|value| value.parse::<u32>().ok());
                if let (Some(output), Some(input)) =
                    (node("link.output.node"), node("link.input.node"))
                {
                    this.links.insert(id, (output, input));
                    this.report(output);
                    this.report(input);
                }
            }
            TYPE_METADATA => {
                if this.metadata.is_some()
                    || props.get("metadata.name").map(String::as_str) != Some("default")
                {
                    return;
                }
                let mut listener = Listener::new(session, id);
                // SAFETY: as above.
                let proxy = unsafe {
                    this.bind(
                        id,
                        TYPE_METADATA,
                        VERSION_METADATA,
                        &mut listener,
                        |methods, object, hook, data| {
                            let methods = &*(methods as *const MetadataMethods);
                            if let Some(add_listener) = methods.add_listener {
                                add_listener(object, hook, &METADATA_EVENTS, data);
                            }
                        },
                    )
                };
                if let Some(proxy) = proxy {
                    this.metadata = Some(Metadata {
                        id,
                        proxy,
                        _listener: listener,
                    });
                }
            }
            _ => {}
        }
    }

    /// Binds a global and lets `listen` attach to it.
    ///
    /// # Safety
    /// The registry must be live; `listen` receives the proxy's method table
    /// and must read it as the interface's own.
    unsafe fn bind(
        &mut self,
        id: u32,
        kind: &str,
        version: u32,
        listener: &mut Listener,
        listen: impl FnOnce(*const c_void, *mut c_void, *mut SpaHook, *mut c_void),
    ) -> Option<*mut c_void> {
        // SAFETY: promised by the caller.
        unsafe {
            let (methods, object) = methods::<RegistryMethods>(self.registry)?;
            let bind = methods.bind?;
            let kind = CString::new(kind).ok()?;
            let proxy = bind(object, id, kind.as_ptr(), version, 0);
            if proxy.is_null() {
                return None;
            }
            let interface = &*(proxy as *const SpaInterface);
            let data = (&raw mut *listener).cast::<c_void>();
            listen(
                interface.cb.funcs,
                interface.cb.data,
                &raw mut listener.hook,
                data,
            );
            Some(proxy)
        }
    }

    fn global_remove(&mut self, id: u32) {
        if let Some(node) = self.nodes.remove(&id) {
            // SAFETY: the proxy is ours and live; destroying it unhooks the
            // listener, which is dropped after.
            unsafe { (self.pw.proxy_destroy)(node.proxy) };
            if node.reported.is_some() {
                self.events.send(if node.class.is_device() {
                    Update::DeviceRemoved(id)
                } else {
                    Update::StreamRemoved(id)
                });
            }
            drop(node.listener);
        } else if let Some(device) = self.devices.remove(&id) {
            // SAFETY: as above.
            unsafe { (self.pw.proxy_destroy)(device.proxy) };
        } else if let Some((output, input)) = self.links.remove(&id) {
            self.report(output);
            self.report(input);
        } else if self
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.id == id)
        {
            let metadata = self.metadata.take().expect("checked above");
            // SAFETY: as above.
            unsafe { (self.pw.proxy_destroy)(metadata.proxy) };
            self.default_sink = None;
            self.default_source = None;
            self.report_defaults();
        }
    }

    fn node_info(&mut self, id: u32, props: Vec<(String, String)>) {
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        if !props.is_empty() {
            node.props = props.into_iter().collect();
            node.own = node.props.contains_key(OWN_STREAM);
            if let Some(serial) = node.props.get("object.serial") {
                node.serial = Some(serial.clone());
            }
        }
        node.have_info = true;
        self.report(id);
    }

    fn node_props(&mut self, id: u32, value: &Pod) {
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        let mut touched = false;
        if let Some(volumes) = value
            .property(pod::PROP_CHANNEL_VOLUMES)
            .and_then(Pod::as_floats)
            && !volumes.is_empty()
        {
            node.volumes = volumes;
            touched = true;
        }
        if let Some(muted) = value.property(pod::PROP_MUTE).and_then(Pod::as_bool) {
            node.muted = muted;
            touched = true;
        }
        if touched {
            node.settled = true;
            self.report(id);
        }
    }

    fn device_route(&mut self, id: u32, value: &Pod) {
        let Some(device) = self.devices.get_mut(&id) else {
            return;
        };
        let (Some(index), Some(direction), Some(card_device)) = (
            value.property(pod::ROUTE_INDEX).and_then(Pod::as_int),
            value.property(pod::ROUTE_DIRECTION).and_then(Pod::as_id),
            value.property(pod::ROUTE_DEVICE).and_then(Pod::as_int),
        ) else {
            return;
        };
        device
            .routes
            .retain(|route| !(route.direction == direction && route.device == card_device));
        device.routes.push(Route {
            index,
            direction,
            device: card_device,
        });
    }

    fn metadata_property(&mut self, subject: u32, key: Option<String>, value: Option<String>) {
        if subject != PW_ID_CORE {
            return;
        }
        let name = value.as_deref().and_then(json_name);
        match key.as_deref() {
            None => {
                self.default_sink = None;
                self.default_source = None;
            }
            Some("default.audio.sink") => self.default_sink = name,
            Some("default.audio.source") => self.default_source = name,
            _ => return,
        }
        self.report_defaults();
    }

    fn report_defaults(&mut self) {
        if !self.ready {
            return;
        }
        if self.reported_defaults.0 != self.default_sink {
            self.reported_defaults.0 = self.default_sink.clone();
            self.events
                .send(Update::DefaultSink(self.default_sink.clone()));
        }
        if self.reported_defaults.1 != self.default_source {
            self.reported_defaults.1 = self.default_source.clone();
            self.events
                .send(Update::DefaultSource(self.default_source.clone()));
        }
    }

    /// Reports a node as it now is, if that differs from the last report.
    fn report(&mut self, id: u32) {
        if !self.ready {
            return;
        }
        let Some(update) = self.describe(id) else {
            return;
        };
        let node = self.nodes.get_mut(&id).expect("described nodes exist");
        if node.reported.as_ref() != Some(&update) {
            node.reported = Some(update.clone());
            self.events.send(update);
        }
    }

    fn describe(&self, id: u32) -> Option<Update> {
        let node = self.nodes.get(&id)?;
        if node.own || !node.have_info || !node.settled {
            return None;
        }
        let prop = |key: &str| {
            node.props
                .get(key)
                .filter(|value| !value.is_empty())
                .cloned()
        };
        if node.class.is_device() {
            let name = prop("node.name").unwrap_or_else(|| format!("node-{id}"));
            let icon_name = prop("device.icon-name").or_else(|| {
                let device = prop("device.id")?.parse::<u32>().ok()?;
                self.devices.get(&device)?.icon_name.clone()
            });
            return Some(Update::Device(Device {
                id,
                description: prop("node.description")
                    .or_else(|| prop("node.nick"))
                    .unwrap_or_else(|| name.clone()),
                name,
                kind: if node.class == Class::Sink {
                    DeviceKind::Sink
                } else {
                    DeviceKind::Source
                },
                channel_volumes: node.volumes.clone(),
                muted: node.muted,
                icon_name,
            }));
        }
        let direction = if node.class == Class::Playback {
            Direction::Playback
        } else {
            Direction::Record
        };
        let device = self.links.values().find_map(|&(output, input)| {
            let (this, peer) = match direction {
                Direction::Playback => (output, input),
                Direction::Record => (input, output),
            };
            (this == id
                && self
                    .nodes
                    .get(&peer)
                    .is_some_and(|peer| peer.class.is_device()))
            .then_some(peer)
        });
        Some(Update::Stream(Stream {
            id,
            app_name: prop("application.name")
                .or_else(|| prop("node.description"))
                .or_else(|| prop("node.name"))
                .unwrap_or_default(),
            app_id: prop("application.id").or_else(|| prop("pipewire.access.portal.app_id")),
            binary: prop("application.process.binary"),
            icon_name: prop("application.icon-name").or_else(|| prop("media.icon-name")),
            media_name: prop("media.name"),
            direction,
            device,
            channel_volumes: node.volumes.clone(),
            muted: node.muted,
            pid: prop("application.process.id").and_then(|pid| pid.parse().ok()),
        }))
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::SetVolumes { id, volumes } => {
                self.set_props(id, vec![(pod::PROP_CHANNEL_VOLUMES, Pod::floats(&volumes))]);
            }
            Command::SetMute { id, muted } => {
                self.set_props(id, vec![(pod::PROP_MUTE, Pod::Bool(muted))]);
            }
            Command::SetDefault { id } => {
                let Some(node) = self.nodes.get(&id) else {
                    return;
                };
                let key = match node.class {
                    Class::Sink => "default.configured.audio.sink",
                    Class::Source => "default.configured.audio.source",
                    _ => return,
                };
                let Some(name) = node.props.get("node.name") else {
                    return;
                };
                let value = format!("{{ \"name\": {} }}", json_quote(name));
                self.set_metadata(PW_ID_CORE, key, Some(("Spa:String:JSON", &value)));
            }
            Command::MoveStream { stream, device } => {
                let Some(serial) = self.nodes.get(&device).and_then(|node| node.serial.clone())
                else {
                    return;
                };
                if !self.nodes.contains_key(&stream) {
                    return;
                }
                self.set_metadata(stream, "target.node", None);
                self.set_metadata(stream, "target.object", Some(("Spa:Id", &serial)));
            }
            Command::StartMonitor {
                monitor,
                device,
                rate_hz,
                bands,
                beat,
            } => self.start_monitor(monitor, device, rate_hz, bands, beat),
            Command::StopMonitor { monitor } => {
                if let Some(monitor) = self.monitors.remove(&monitor) {
                    // SAFETY: the stream is ours; destroying it unhooks the
                    // listener before the box goes.
                    unsafe { (self.pw.stream_destroy)(monitor.stream) };
                }
            }
        }
    }

    /// Sets properties of a device or stream: on the device's active route
    /// when it has one, on the node otherwise.
    fn set_props(&mut self, id: ObjectId, properties: Vec<(u32, Pod)>) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        let props = Pod::object(pod::OBJECT_PROPS, PARAM_PROPS, properties);
        if node.class.is_device()
            && let Some(device_id) = node
                .props
                .get("device.id")
                .and_then(|id| id.parse::<u32>().ok())
            && let Some(card_device) = node
                .props
                .get("card.profile.device")
                .and_then(|device| device.parse::<i32>().ok())
            && let Some(device) = self.devices.get(&device_id)
        {
            let direction = if node.class == Class::Sink {
                DIRECTION_OUTPUT
            } else {
                DIRECTION_INPUT
            };
            if let Some(route) = device
                .routes
                .iter()
                .find(|route| route.device == card_device && route.direction == direction)
            {
                let route = Pod::object(
                    pod::OBJECT_ROUTE,
                    PARAM_ROUTE,
                    vec![
                        (pod::ROUTE_INDEX, Pod::Int(route.index)),
                        (pod::ROUTE_DEVICE, Pod::Int(card_device)),
                        (pod::ROUTE_PROPS, props),
                        (pod::ROUTE_SAVE, Pod::Bool(true)),
                    ],
                )
                .encode();
                // SAFETY: the device proxy is live and a device.
                unsafe { set_param::<DeviceEvents>(device.proxy, PARAM_ROUTE, &route) };
                return;
            }
        }
        let props = props.encode();
        // SAFETY: the node proxy is live and a node.
        unsafe { set_param::<NodeEvents>(node.proxy, PARAM_PROPS, &props) };
    }

    fn set_metadata(&mut self, subject: u32, key: &str, value: Option<(&str, &str)>) {
        let Some(metadata) = &self.metadata else {
            return;
        };
        let Ok(key) = CString::new(key) else {
            return;
        };
        let value = value
            .and_then(|(kind, value)| Some((CString::new(kind).ok()?, CString::new(value).ok()?)));
        // SAFETY: the metadata proxy is live; the strings outlive the call.
        unsafe {
            if let Some((methods, object)) = methods::<MetadataMethods>(metadata.proxy)
                && let Some(set_property) = methods.set_property
            {
                let (kind, value) = value
                    .as_ref()
                    .map_or((ptr::null(), ptr::null()), |(kind, value)| {
                        (kind.as_ptr(), value.as_ptr())
                    });
                set_property(object, subject, key.as_ptr(), kind, value);
            }
        }
    }

    fn start_monitor(
        &mut self,
        id: u64,
        device: Option<ObjectId>,
        rate_hz: f32,
        bands: usize,
        beat: bool,
    ) {
        let mut props = vec![
            ("media.type", "Audio".to_owned()),
            ("media.category", "Capture".to_owned()),
            ("media.role", "DSP".to_owned()),
            ("node.name", "morf-level".to_owned()),
            ("node.description", "morf level meter".to_owned()),
            ("application.name", "morf".to_owned()),
            ("node.passive", "true".to_owned()),
            ("node.dont-reconnect", "false".to_owned()),
            (OWN_STREAM, "true".to_owned()),
        ];
        match device {
            None => props.push(("stream.capture.sink", "true".to_owned())),
            Some(device) => {
                let Some(node) = self
                    .nodes
                    .get(&device)
                    .filter(|node| node.class.is_device())
                else {
                    self.events
                        .send(Update::Error(format!("level monitor: no device {device}")));
                    return;
                };
                if node.class == Class::Sink {
                    props.push(("stream.capture.sink", "true".to_owned()));
                }
                let target = node
                    .serial
                    .clone()
                    .or_else(|| node.props.get("node.name").cloned())
                    .unwrap_or_default();
                props.push(("target.object", target));
            }
        }
        let owned: Vec<(CString, CString)> = props
            .into_iter()
            .filter_map(|(key, value)| Some((CString::new(key).ok()?, CString::new(value).ok()?)))
            .collect();
        let items: Vec<SpaDictItem> = owned
            .iter()
            .map(|(key, value)| SpaDictItem {
                key: key.as_ptr(),
                value: value.as_ptr(),
            })
            .collect();
        let dict = SpaDict {
            flags: 0,
            n_items: items.len() as u32,
            items: items.as_ptr(),
        };
        let format = Pod::object(
            pod::OBJECT_FORMAT,
            PARAM_ENUM_FORMAT,
            vec![
                (pod::FORMAT_MEDIA_TYPE, Pod::Id(pod::MEDIA_TYPE_AUDIO)),
                (pod::FORMAT_MEDIA_SUBTYPE, Pod::Id(pod::MEDIA_SUBTYPE_RAW)),
                (pod::FORMAT_AUDIO_FORMAT, Pod::Id(native_f32())),
                (pod::FORMAT_AUDIO_CHANNELS, Pod::Int(2)),
                (
                    pod::FORMAT_AUDIO_POSITION,
                    Pod::ids(&[pod::AUDIO_CHANNEL_FL, pod::AUDIO_CHANNEL_FR]),
                ),
            ],
        )
        .encode();
        // SAFETY: the core is live; the dict and its strings outlive
        // `properties_new_dict`, which copies them; the monitor is boxed
        // before its address is handed over.
        unsafe {
            let properties = (self.pw.properties_new_dict)(&dict);
            let name = c"morf-level";
            let stream = (self.pw.stream_new)(self.core, name.as_ptr(), properties);
            if stream.is_null() {
                self.events.send(Update::Error(
                    "level monitor: cannot create a stream".into(),
                ));
                return;
            }
            let mut monitor = Box::new(Monitor {
                pw: Arc::clone(&self.pw),
                events: self.events.clone(),
                id,
                stream,
                hook: SpaHook::zeroed(),
                meter: Meter::new(rate_hz, bands).with_beats(beat),
            });
            let data = (&raw mut *monitor).cast::<c_void>();
            (self.pw.stream_add_listener)(stream, &raw mut monitor.hook, &STREAM_EVENTS, data);
            let mut params = [format.as_ptr()];
            let result = (self.pw.stream_connect)(
                stream,
                DIRECTION_INPUT,
                PW_ID_ANY,
                STREAM_FLAG_AUTOCONNECT | STREAM_FLAG_MAP_BUFFERS,
                params.as_mut_ptr(),
                1,
            );
            if result < 0 {
                (self.pw.stream_destroy)(stream);
                self.events.send(Update::Error(format!(
                    "level monitor: cannot connect ({result})"
                )));
                return;
            }
            self.monitors.insert(id, monitor);
        }
    }
}

/// Sets one parameter on a node or device proxy.
///
/// # Safety
/// `proxy` must be a live proxy whose methods are `ParamMethods<E>`.
unsafe fn set_param<E>(proxy: *mut c_void, id: u32, value: &pod::Encoded) {
    // SAFETY: promised by the caller.
    unsafe {
        if let Some((methods, object)) = methods::<ParamMethods<E>>(proxy)
            && let Some(set_param) = methods.set_param
        {
            set_param(object, id, 0, value.as_ptr());
        }
    }
}

fn native_f32() -> u32 {
    if cfg!(target_endian = "little") {
        pod::AUDIO_FORMAT_F32_LE
    } else {
        pod::AUDIO_FORMAT_F32_LE + 1
    }
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
