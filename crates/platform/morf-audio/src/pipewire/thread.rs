//! The backend's thread: finding libpipewire's plugins, connecting and
//! retrying, and one session's main loop.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Once};
use std::time::Instant;

use crate::{Events, Update};

use super::ffi::*;
use super::{
    CORE_EVENTS, Message, Outcome, REGISTRY_EVENTS, RETRY, Session, SharedWaker, Waker, on_command,
};

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

pub(super) fn run(
    pw: Arc<Pw>,
    events: Events,
    mut receiver: Receiver<Message>,
    waker: SharedWaker,
) {
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
