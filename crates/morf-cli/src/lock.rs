use morf_io::IpcIncoming;
use morf_lua::{IpcValue, Runtime, SessionLockState};
use morf_render::{RenderEngine, WgpuBackend};
use morf_wayland::{LayerClient, LayerEvent, ScreenInfo};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::{
    capture::*, lock_outputs::*, paint::*, services::apply_idle_timeouts, surface_keys::*,
    surface_layers::*, surface_pointer::*, surfaces::*, wake_plan::*,
};

pub(crate) struct Worker {
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) commands: WorkerSender,
    pub(crate) join: JoinHandle<()>,
    pub(crate) screen: ScreenInfo,
}

/// The way into an output thread: a channel whose every message rings the
/// loop's alarm, because the thread sleeps until something does -- a command
/// that only sat in the channel waited for the next unrelated wake, and its
/// sender, blocked on the answer, timed out first.
pub(crate) struct WorkerSender(mpsc::Sender<WorkerCommand>);

impl WorkerSender {
    pub(crate) fn new(sender: mpsc::Sender<WorkerCommand>) -> Self {
        Self(sender)
    }

    pub(crate) fn send(
        &self,
        command: WorkerCommand,
    ) -> Result<(), mpsc::SendError<WorkerCommand>> {
        let sent = self.0.send(command);
        morf_io::wake_all();
        sent
    }
}

impl Worker {
    /// Tells the output thread to stop, and wakes it to hear it.
    pub(crate) fn request_stop(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        morf_io::wake_all();
    }
}

pub(crate) enum WorkerCommand {
    Call {
        target: String,
        args: Vec<IpcValue>,
        reply: mpsc::SyncSender<Result<Vec<IpcValue>, String>>,
    },
    /// The compositor's output list, as the supervisor last recorded it.
    Screens(Vec<ScreenInfo>),
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
}

pub(crate) enum SupervisorMessage {
    Worker(WorkerMessage),
    Ipc(IpcIncoming),
    Reload {
        hard: bool,
    },
    WatchFiles(bool),
    /// The configuration asked the shell to stop.
    Quit,
}

pub(crate) enum WorkerMessage {
    /// A worker ran the configuration. `session_lock` is what it asked to
    /// be: a worker that hears `true` stops there, and the supervisor runs
    /// the file as a lock instead.
    Loaded {
        output: String,
        session_lock: bool,
    },
    Screens {
        output: String,
        screens: Vec<ScreenInfo>,
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
pub(crate) fn run_lock(mut runtime: Runtime, path: &std::path::Path) -> Result<(), String> {
    // One tree for every output, or one built for each: see lock_outputs.rs.
    let trees = LockTrees::of(&runtime)?;
    // Before the lock is asked for, so `morf --lock ipc call` reaches it
    // from the first moment; beside the shell's socket, not over it.
    let ipc = crate::lock_ipc::LockIpc::bind(path)?;
    let mut client = LayerClient::connect_lock().map_err(|error| error.to_string())?;
    client.set_idle_timeouts(&runtime.idle_timeouts());
    client
        .begin_session_lock()
        .map_err(|error| error.to_string())?;
    // Asked for, not yet granted: the compositor says `locked` once every
    // output shows a locked frame, and only then is the session hidden.
    runtime.set_session_lock_state(SessionLockState::Pending);
    apply_service_requests(&mut runtime, &mut client);
    let mut outputs: Vec<LockOutput> = Vec::new();
    let mut last_frame = None;
    // The pointer, the fingers and which node each surface's keys go to: the
    // same state, and the same routing, a layer surface has.
    let mut input = PointerInput::default();
    let mut locked = false;
    let mut unlock_pending = false;
    let mut clock = clock_text();
    runtime
        .update_clock(&clock)
        .map_err(|error| error.to_string())?;
    let wake = morf_io::Wake::new().map_err(|error| error.to_string())?;
    client.set_waker(morf_io::wake_all);
    // As on an output: one more turn at once after a turn that handled
    // events, for what their handlers left behind.
    let mut follow_up = false;
    let mut pending_streak = 0;
    loop {
        let sleep = Sleep::plan_with(
            &runtime,
            std::mem::take(&mut follow_up) || client.has_queued_events(),
            None,
            &mut pending_streak,
        );
        let slept = Instant::now();
        let woke = client
            .wait_for(sleep.timeout(), Some(wake.as_fd()))
            .map_err(|error| error.to_string())?;
        wake.drain();
        log_wake("lock", woke, &sleep, slept);
        let mut repaint = runtime.poll_services();
        repaint |= ipc.serve(&mut runtime);
        apply_service_requests(&mut runtime, &mut client);
        apply_idle_timeouts(&mut runtime, &mut client);
        unlock_pending |= runtime.take_session_unlock_request();
        // The file lifts the lock the way it asked for it: by clearing
        // `morf.surface.session_lock`. Which door was opened — a password, a
        // finger, a face — is the file's business, not this loop's.
        unlock_pending |= !runtime.layer_surface_config().session_lock;
        if locked && unlock_pending {
            client.unlock_session().map_err(|error| error.to_string())?;
            runtime.set_session_lock_state(SessionLockState::Unlocked);
            return Ok(());
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
            let event = match handle_pointer_event(
                &mut runtime,
                &mut client,
                &mut input,
                &LockLayouts(&outputs),
                event,
            ) {
                Ok(Ok(painted)) => {
                    repaint |= painted;
                    continue;
                }
                Ok(Err(event)) => event,
                // A scene mid-change fails a hit test the way it fails a
                // layout; the event is dropped and the lock stays up.
                Err(error) => {
                    runtime.warn(format!("lock input: {error}"));
                    continue;
                }
            };
            match event {
                // A lock client has only lock surfaces, and those are not
                // popups or floating windows.
                LayerEvent::AuxScale { .. }
                | LayerEvent::ShortcutsInhibited { .. }
                | LayerEvent::KeyboardFocus { .. }
                | LayerEvent::SurfaceKeyboard { .. }
                | LayerEvent::SurfacePointer { .. } => {}
                LayerEvent::SessionLocked => {
                    locked = true;
                    repaint |= runtime.set_session_lock_state(SessionLockState::Locked);
                }
                LayerEvent::Screens(_) => {}
                LayerEvent::SessionLockConfigure {
                    index,
                    width: logical_width,
                    height: logical_height,
                } => {
                    if outputs.len() <= index {
                        outputs.resize_with(index + 1, LockOutput::default);
                    }
                    // Before the primer, which is this tree's colour.
                    ensure_lock_tree(
                        &mut runtime,
                        trees,
                        &mut outputs[index],
                        index,
                        client.lock_screen(index),
                        (logical_width, logical_height),
                    )?;
                    let root = trees
                        .root(&outputs, index)
                        .ok_or_else(|| "lock surface has no tree to draw".to_owned())?;
                    let (width, height) = client
                        .lock_physical_size(index)
                        .ok_or_else(|| "configured lock surface disappeared".to_owned())?;
                    if let Some(renderer) = &mut outputs[index].renderer {
                        renderer.resize(width, height);
                    } else {
                        // The root's colour, up before the GPU is looked for,
                        // so the compositor has a locked frame within
                        // milliseconds rather than after three devices and
                        // three 4K frames.
                        client.prime_lock(index, root_color_bytes(&runtime, root)?);
                        let target = client
                            .lock_window_target(index)
                            .ok_or_else(|| "configured lock surface disappeared".to_owned())?;
                        let backend =
                            pollster::block_on(WgpuBackend::new_surface(target, width, height))
                                .map_err(|error| error.to_string())?;
                        outputs[index].renderer = Some(RenderEngine::new(backend));
                    }
                    repaint = true;
                }
                LayerEvent::SessionLockSurfaceRemoved { index } => {
                    if index < outputs.len() {
                        let mut gone = outputs.remove(index);
                        release_lock_tree(&mut runtime, &mut gone);
                    }
                    // The surfaces after it moved down one, and so did every
                    // role the input state names.
                    input.reset();
                }
                LayerEvent::SessionLockFrame { time_ms, .. } => {
                    let frame = runtime
                        .tick_animations(animation_delta(last_frame, time_ms))
                        .map_err(|error| error.to_string())?;
                    last_frame = frame.active.then_some(time_ms);
                    repaint |= frame.active || frame.changed > 0;
                }
                LayerEvent::Key {
                    surface,
                    pressed,
                    repeat,
                    keysym,
                    text,
                    modifiers,
                    ..
                } => {
                    // The same routing every other surface gets. This used to
                    // send every key to the first focusable node in the tree,
                    // with no Tab traversal and no memory of what had focus —
                    // so a lock screen with more than one field had one that
                    // could not be reached.
                    let Some(root) = trees.key_root(&outputs, surface) else {
                        continue;
                    };
                    // Remembered per surface, as a click set it.
                    let mut focused = input.focused.get(&surface).copied();
                    repaint |= dispatch_key_in_subtree(
                        &mut runtime,
                        root,
                        &mut focused,
                        KeyAction::of(pressed, repeat),
                        keysym,
                        text.as_deref(),
                        key_modifiers(modifiers),
                    );
                    match focused {
                        Some(node) => input.focused.insert(surface, node),
                        None => input.focused.remove(&surface),
                    };
                }
                LayerEvent::SessionLockFinished => {
                    // Told before the process goes, so a configuration can
                    // say why: refused outright, or ended from outside.
                    let (state, error) = if locked {
                        (SessionLockState::Unlocked, "compositor ended the session lock")
                    } else {
                        (SessionLockState::Failed, "compositor refused the session lock")
                    };
                    runtime.set_session_lock_state(state);
                    return Err(error.to_owned());
                }
                LayerEvent::Idle {
                    timeout_ms,
                    input_only,
                    idle,
                } => {
                    repaint |= runtime.dispatch_idle(timeout_ms, input_only, idle);
                }
                LayerEvent::Clipboard { text } => {
                    repaint |= runtime.dispatch_clipboard(text);
                }
                LayerEvent::Selection { primary, offer } => {
                    repaint |= runtime.dispatch_selection(
                        primary,
                        offer.map(|offer| morf_lua::OfferDescription {
                            id: offer.id,
                            mime_types: offer.mime_types,
                            ..morf_lua::OfferDescription::default()
                        }),
                    );
                }
                LayerEvent::OfferRead { request_id, result } => {
                    repaint |= runtime.dispatch_offer_read(request_id, result);
                }
                LayerEvent::Screencopy { request_id, result } => {
                    repaint |= dispatch_screencopy(&mut runtime, None, request_id, result);
                }
                LayerEvent::CaptureOffer {
                    request_id,
                    width,
                    height,
                    device,
                    formats,
                } => {
                    // A lock screen draws one renderer per output, none of
                    // them the one a capture would name: shared memory.
                    repaint |= answer_capture_offer(
                        &mut runtime,
                        None,
                        &mut client,
                        OfferedCapture {
                            request_id,
                            width,
                            height,
                            device,
                            formats,
                        },
                    );
                }
                LayerEvent::InputMethod(state) => {
                    repaint |= runtime.dispatch_input_method(
                        state.active,
                        state.surrounding_text,
                        state.cursor,
                        state.anchor,
                        state.serial,
                    );
                }
                LayerEvent::TextInput(state) => {
                    repaint |= runtime.dispatch_text_input(
                        state.focused,
                        state.preedit,
                        state.preedit_begin,
                        state.preedit_end,
                        state.commit,
                        state.delete_before,
                        state.delete_after,
                        state.serial,
                    );
                }
                LayerEvent::Configure { .. }
                | LayerEvent::Scale { .. }
                | LayerEvent::Frame { .. }
                // Taken above, by the pointer path every surface shares.
                | LayerEvent::PointerMotion { .. }
                | LayerEvent::PointerLeave { .. }
                | LayerEvent::PointerButton { .. }
                | LayerEvent::PointerAxis { .. }
                | LayerEvent::TouchDown { .. }
                | LayerEvent::TouchMotion { .. }
                | LayerEvent::TouchUp { .. }
                | LayerEvent::TouchCancel
                | LayerEvent::PopupConfigure { .. }
                | LayerEvent::PopupFrame { .. }
                | LayerEvent::PopupDone { .. }
                | LayerEvent::FloatingConfigure { .. }
                | LayerEvent::FloatingFrame { .. }
                | LayerEvent::FloatingClose { .. }
                // A lock screen takes no drags, in or out.
                | LayerEvent::DragEnter { .. }
                | LayerEvent::DragMotion { .. }
                | LayerEvent::DragLeave { .. }
                | LayerEvent::Drop { .. }
                | LayerEvent::DragSourceEnded { .. }
                | LayerEvent::Closed { .. } => {}
            }
        }
        apply_service_requests(&mut runtime, &mut client);
        if repaint {
            // A tree taken down leaves shaped text and textures in whichever
            // renderer drew it, keyed on nodes that are gone.
            let removed = runtime.take_removed_nodes();
            if !removed.is_empty() {
                for renderer in outputs
                    .iter_mut()
                    .filter_map(|output| output.renderer.as_mut())
                {
                    renderer.backend_mut().forget_nodes(&removed);
                }
            }
            for index in 0..outputs.len() {
                let Some(root) = trees.root(&outputs, index) else {
                    continue;
                };
                let output = &mut outputs[index];
                if let Some(renderer) = &mut output.renderer {
                    output.layout = Some(paint_lock(&mut runtime, renderer, &client, index, root)?);
                    client.release_lock_primer(index);
                }
            }
        }
    }
}
