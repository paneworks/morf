use morf_io::IpcIncoming;
use morf_layout::Size;
use morf_lua::{IpcValue, Runtime};
use morf_render::{RenderEngine, WgpuBackend};
use morf_scene::Element;
use morf_wayland::{LayerClient, LayerEvent, ScreenInfo};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::{
    capture::*, paint::*, surface_keys::*, surface_layers::*, surface_pointer::*, surfaces::*,
};

/// One output's lock surface: what draws it, and what it last drew.
#[derive(Default)]
pub(crate) struct LockOutput {
    pub(crate) renderer: Option<RenderEngine<WgpuBackend>>,
    /// The layout of the last frame, which is what a point on this surface
    /// is hit-tested against.
    pub(crate) layout: Option<morf_layout::Layout>,
}

/// The lock's surfaces, as the input path looks them up.
pub(crate) struct LockLayouts<'a>(pub(crate) &'a [LockOutput]);

impl SurfaceLayouts for LockLayouts<'_> {
    fn layout_of(&self, surface: morf_wayland::SurfaceRole) -> Option<&morf_layout::Layout> {
        match surface {
            morf_wayland::SurfaceRole::Lock(index) => self.0.get(index)?.layout.as_ref(),
            _ => None,
        }
    }
}

pub(crate) struct Worker {
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) commands: mpsc::Sender<WorkerCommand>,
    pub(crate) join: JoinHandle<()>,
    pub(crate) screen: ScreenInfo,
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
pub(crate) fn run_lock(mut runtime: Runtime) -> Result<(), String> {
    if runtime.scene().roots().len() != 1 {
        return Err("lock configuration must create exactly one root item".to_owned());
    }
    let root = runtime.scene().roots()[0];
    if runtime
        .scene()
        .element(root)
        .map_err(|error| error.to_string())?
        != Element::Rect
    {
        return Err("lock configuration root must be an opaque Rect".to_owned());
    }
    let mut client = LayerClient::connect_lock().map_err(|error| error.to_string())?;
    client.set_idle_timeouts(&runtime.idle_timeouts());
    client
        .begin_session_lock()
        .map_err(|error| error.to_string())?;
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
    loop {
        client
            .dispatch_timeout_or(
                until_next_second().min(Duration::from_millis(100)),
                Some(wake.as_fd()),
            )
            .map_err(|error| error.to_string())?;
        wake.drain();
        let mut repaint = runtime.poll_services();
        apply_service_requests(&mut runtime, &mut client);
        unlock_pending |= runtime.take_session_unlock_request();
        // The file lifts the lock the way it asked for it: by clearing
        // `morf.surface.session_lock`. Which door was opened — a password, a
        // finger, a face — is the file's business, not this loop's.
        unlock_pending |= !runtime.layer_surface_config().session_lock;
        if locked && unlock_pending {
            client.unlock_session().map_err(|error| error.to_string())?;
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
                | LayerEvent::KeyboardFocus { .. } => {}
                LayerEvent::SessionLocked => locked = true,
                LayerEvent::Screens(_) => {}
                LayerEvent::SessionLockConfigure { index, .. } => {
                    if outputs.len() <= index {
                        outputs.resize_with(index + 1, LockOutput::default);
                    }
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
                        client.prime_lock(index, root_color_bytes(&runtime)?);
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
                        outputs.remove(index);
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
                    pressed: true,
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
                    let Some(root) = runtime.scene().roots().first().copied() else {
                        continue;
                    };
                    // Remembered per surface, as a click set it.
                    let mut focused = input.focused.get(&surface).copied();
                    repaint |= dispatch_key_in_subtree(
                        &mut runtime,
                        root,
                        &mut focused,
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
                    return Err("compositor ended the session lock".to_owned());
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
                LayerEvent::Key { pressed: false, .. }
                | LayerEvent::Configure { .. }
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
            for (index, output) in outputs.iter_mut().enumerate() {
                if let Some(renderer) = &mut output.renderer {
                    output.layout = Some(paint_lock(&mut runtime, renderer, &client, index)?);
                    client.release_lock_primer(index);
                }
            }
        }
    }
}

/// The lock root's colour as bytes, for the first frame.
fn root_color_bytes(runtime: &Runtime) -> Result<[u8; 4], String> {
    let root = primary_surface_root(runtime)?;
    let color = runtime
        .scene()
        .color_value(root, "color")
        .map_err(|error| error.to_string())?;
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    Ok([
        byte(color.red),
        byte(color.green),
        byte(color.blue),
        byte(color.alpha),
    ])
}

pub(crate) fn paint_lock(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &LayerClient,
    index: usize,
) -> Result<morf_layout::Layout, String> {
    let (width, height) = client
        .lock_size(index)
        .ok_or_else(|| "lock surface disappeared while painting".to_owned())?;
    let root = primary_surface_root(runtime)?;
    {
        let mut scene = runtime.scene_mut();
        scene
            .assign(root, "x", 0.0)
            .map_err(|error| error.to_string())?;
        scene
            .assign(root, "y", 0.0)
            .map_err(|error| error.to_string())?;
        scene
            .assign(root, "width", width as f64)
            .map_err(|error| error.to_string())?;
        scene
            .assign(root, "height", height as f64)
            .map_err(|error| error.to_string())?;
    }
    let scene = runtime.scene();
    let color = scene
        .color_value(root, "color")
        .map_err(|error| error.to_string())?;
    if color.alpha < 1.0
        || scene
            .number(root, "opacity")
            .map_err(|error| error.to_string())?
            < 1.0
    {
        return Err("lock configuration root must stay opaque".to_owned());
    }
    if scene
        .number(root, "width")
        .map_err(|error| error.to_string())?
        < width as f64
        || scene
            .number(root, "height")
            .map_err(|error| error.to_string())?
            < height as f64
    {
        return Err("lock configuration root must cover the output".to_owned());
    }
    drop(scene);
    let layout = runtime.compute_layout(
        root,
        Size {
            width: width as f64,
            height: height as f64,
        },
        renderer.backend_mut(),
    )?;
    runtime.sync_text_inputs(&layout, renderer.backend_mut().text_system());
    let scene = runtime.scene();
    client.request_lock_frame(index);
    let scale = client.lock_scale_120(index).unwrap_or(120);
    let damage = renderer
        .render(&scene, &layout, scale, |_| {})
        .map_err(|error| error.to_string())?;
    if damage.is_empty() {
        client.commit_lock(index);
    }
    drop(scene);
    runtime.observe_layout(&layout);
    Ok(layout)
}
