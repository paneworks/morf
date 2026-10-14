use morf_app::Backend as _;
use morf_app::{LayerClient, Output, WindowId};
use morf_io::IpcIncoming;
use morf_lua::Runtime;
use morf_render::RenderEngine;
use morf_value::IpcValue;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::host::desktop::apply_service_requests;
use crate::host::desktop::{desktop_for, dispatch_desktop};
use crate::lock::session::{LockSession, LockStep};
use crate::paint::render_target::surface_backend;
use crate::{
    input::pointer::*, lock::outputs::*, paint::wake_plan::*, paint::*,
    process::services::apply_idle_timeouts, process::services::refresh_idle_timeouts,
};

pub struct Worker {
    pub stop: Arc<AtomicBool>,
    pub commands: WorkerSender,
    pub join: JoinHandle<()>,
    pub screen: Output,
}

/// The way into an output thread: a channel whose every message rings the
/// loop's alarm, because the thread sleeps until something does -- a command
/// that only sat in the channel waited for the next unrelated wake, and its
/// sender, blocked on the answer, timed out first.
pub struct WorkerSender {
    sender: mpsc::Sender<WorkerCommand>,
    alarm: WorkerAlarm,
}

/// One output thread's alarm, claimed by the thread as it starts: until
/// then it has no loop to ring, and reads its channel before it first waits.
#[derive(Clone, Default)]
pub struct WorkerAlarm(Arc<std::sync::OnceLock<morf_io::Waker>>);

impl WorkerAlarm {
    /// Makes this thread's loop the one rung.
    pub fn claim(&self) {
        let _ = self.0.set(morf_io::waker());
    }

    /// Rings that thread's loop and no other.
    pub fn ring(&self) {
        if let Some(waker) = self.0.get() {
            waker.wake();
        }
    }
}

impl WorkerSender {
    pub fn new(sender: mpsc::Sender<WorkerCommand>, alarm: WorkerAlarm) -> Self {
        Self { sender, alarm }
    }

    pub fn send(&self, command: WorkerCommand) -> Result<(), mpsc::SendError<WorkerCommand>> {
        let sent = self.sender.send(command);
        self.alarm.ring();
        sent
    }
}

impl Worker {
    /// Tells the output thread to stop, and wakes it to hear it.
    pub fn request_stop(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        self.commands.alarm.ring();
    }
}

pub enum WorkerCommand {
    Call {
        target: String,
        args: Vec<IpcValue>,
        reply: mpsc::SyncSender<Result<Vec<IpcValue>, String>>,
    },
    /// The compositor's output list, as the supervisor last recorded it.
    Screens(Vec<Output>),
    Verbs(mpsc::SyncSender<Vec<String>>),
    Logs(mpsc::SyncSender<Vec<String>>),
    Capabilities(mpsc::SyncSender<Vec<String>>),
    Bindings(mpsc::SyncSender<Vec<String>>),
    Reload {
        path: Arc<PathBuf>,
        source: Arc<[u8]>,
        hard: bool,
        reply: mpsc::SyncSender<Result<(), String>>,
    },
    /// The configuration asked to lock the session: this worker takes the
    /// runtime it ran it with and becomes the lock.
    BecomeLock,
    /// This worker's runtime is now the primary one, or no longer is
    /// (`morf.primary()`).
    Primary(bool),
}

pub enum SupervisorMessage {
    Worker(WorkerMessage),
    Ipc(IpcIncoming),
    Reload {
        hard: bool,
    },
    WatchFiles(bool),
    /// The configuration asked the shell to stop.
    Quit,
    /// Ask the compositor which outputs it has, and run a worker on each:
    /// after an output's surface was closed under it, and again every
    /// second while there is none.
    Probe,
}

pub enum WorkerMessage {
    /// A worker ran the configuration. `session_lock` is what it asked to
    /// be: a worker that hears `true` stops there, and the supervisor runs
    /// the file as a lock instead.
    Loaded {
        output: String,
        session_lock: bool,
        /// Whether it asked to keep running with no output
        /// (`morf.surface.outputless`).
        outputless: bool,
    },
    /// A reload changed `morf.surface.outputless`.
    Outputless {
        output: String,
        wanted: bool,
    },
    Screens {
        output: String,
        screens: Vec<Output>,
    },
    Failed {
        output: String,
        error: String,
    },
}

/// Holds the session locked with a configuration that asked to.
///
/// Entered from the ordinary run, once the file has said
/// `morf.surface.session_lock = true`: the same file that would have been a
/// layer is instead one lock surface per output, until it says otherwise.
pub fn run_lock(mut runtime: Runtime, path: &std::path::Path) -> Result<(), String> {
    // One tree for every output, or one built for each: see lock_outputs.rs.
    let mut lock = LockSession::new(&runtime)?;
    // Before the lock is asked for, so `morf --lock ipc call` reaches it
    // from the first moment; beside the shell's socket, not over it.
    let ipc = crate::lock::ipc::LockIpc::bind(path)?;
    let client = LayerClient::connect_lock().map_err(|error| error.to_string())?;
    let mut desktop = desktop_for(&client)?;
    // In morf's pixels, as the shell's own surfaces are.
    let mut client = morf_app::Dense::new(Box::new(client), runtime.density());
    refresh_idle_timeouts(&runtime, &mut *desktop);
    LockSession::request(&mut runtime, &mut client)?;
    apply_service_requests(&mut runtime, &mut client, &mut *desktop);
    let mut clock = clock_text();
    runtime
        .update_clock(&clock)
        .map_err(|error| error.to_string())?;
    let wake = morf_io::Wake::new().map_err(|error| error.to_string())?;
    client.set_waker(crate::host::desktop::loop_wakeup());
    // As on an output: one more turn at once after a turn that handled
    // events, for what their handlers left behind.
    let mut follow_up = false;
    let mut repaint_next = false;
    let mut pending_streak = 0;
    loop {
        let sleep = Sleep::plan_with(
            &runtime,
            std::mem::take(&mut follow_up) || client.has_queued_events(),
            None,
            &mut pending_streak,
        );
        let slept = Instant::now();
        let woke = client.wait(sleep.timeout(), Some(wake.as_fd()))?;
        let desktop_repaint = dispatch_desktop(&mut runtime, &mut *desktop, None)?;
        wake.drain();
        log_wake("lock", woke, &sleep, slept);
        let mut repaint =
            std::mem::take(&mut repaint_next) | desktop_repaint | runtime.poll_services();
        repaint |= ipc.serve(&mut runtime);
        apply_service_requests(&mut runtime, &mut client, &mut *desktop);
        apply_idle_timeouts(&mut runtime, &mut *desktop);
        if lock.wants_unlock(&mut runtime) {
            return lock.unlock(&mut runtime, &mut client);
        }
        let next_clock = clock_text();
        if next_clock != clock {
            clock = next_clock;
            repaint |= runtime
                .update_clock(&clock)
                .map_err(|error| error.to_string())?;
        }
        while let Some(event) = client.next_event() {
            follow_up = true;
            match lock.handle(&mut runtime, &mut client, event)? {
                LockStep::Handled(painted) => repaint |= painted,
                LockStep::Finished(error) => return Err(error),
                LockStep::Configure {
                    index,
                    width: logical_width,
                    height: logical_height,
                } => {
                    // Before the primer, which is this tree's colour.
                    let root = lock.configure(
                        &mut runtime,
                        index,
                        client.inner().lock_screen(index),
                        (logical_width, logical_height),
                    )?;
                    let (width, height) = client
                        .lock_physical_size(index)
                        .ok_or_else(|| "configured lock surface disappeared".to_owned())?;
                    let window = lock
                        .window_mut(index)
                        .ok_or_else(|| "lock surface has no window".to_owned())?;
                    if let Some(renderer) = &mut window.renderer {
                        renderer.resize(width, height);
                    } else {
                        // The root's colour, up before the GPU is looked for,
                        // so the compositor has a locked frame within
                        // milliseconds rather than after three devices and
                        // three 4K frames.
                        client
                            .inner_mut()
                            .prime_lock(index, root_color_bytes(&runtime, root)?);
                        let target = client
                            .render_target(WindowId::Lock(index))
                            .ok_or_else(|| "configured lock surface disappeared".to_owned())?;
                        let backend = surface_backend(target, width, height)
                            .map_err(|error| error.to_string())?;
                        if let Some(window) = lock.window_mut(index) {
                            window.renderer = Some(RenderEngine::new(backend));
                        }
                    }
                    repaint = true;
                }
            }
        }
        apply_service_requests(&mut runtime, &mut client, &mut *desktop);
        if repaint {
            // A tree taken down leaves shaped text and textures in whichever
            // renderer drew it, keyed on nodes that are gone.
            let removed = runtime.take_removed_nodes();
            if !removed.is_empty() {
                for renderer in lock
                    .outputs
                    .values_mut()
                    .filter_map(|output| output.renderer.as_mut())
                {
                    renderer.backend_mut().forget_nodes(&removed);
                }
            }
            for index in lock.indices() {
                let Some(root) = lock.root(index) else {
                    continue;
                };
                let Some(output) = lock.window_mut(index) else {
                    continue;
                };
                if let Some(renderer) = &mut output.renderer {
                    let layout = paint_lock(&mut runtime, renderer, &client, index, root)?;
                    output.layout = Some(CachedLayout::uncached(layout));
                    client.inner_mut().release_lock_primer(index);
                }
            }
            if answer_new_containment(&mut runtime, &lock.input, &lock.outputs) {
                repaint_next = true;
                follow_up = true;
            }
        }
    }
}
