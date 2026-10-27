//! The desktop protocols, beside a window client on its connection: what
//! they send the configuration, and what it queued for them.

use morf_app::Backend;
use morf_desktop::{Desktop, DesktopEvent, ToplevelAction};
use morf_lua::{Runtime, Toplevel};
use morf_paint::RenderEngine;
use morf_render::WgpuBackend;

use crate::capture::{
    OfferedCapture, answer_capture_offer, apply_screencopy_requests, dispatch_screencopy,
};
use crate::process::services::{
    apply_clipboard_requests, apply_input_method_requests, apply_output_power_requests,
    apply_text_input_requests, apply_virtual_keyboard_requests,
};

/// The desktop protocols on `client`'s connection; a request naming no
/// output is for the one `client`'s surface sits on.
pub fn desktop_for(client: &dyn Backend) -> Result<Box<dyn Desktop>, String> {
    let link = client
        .desktop_link()
        .ok_or_else(|| "the desktop protocols need a compositor".to_owned())?;
    let mut desktop = morf_desktop::connect(link)?;
    // A selection read finishing on its thread rings this loop, so it wakes
    // for its answer rather than sleeping past it.
    desktop.set_waker(loop_wakeup());
    Ok(desktop)
}

/// Rings the loop on this thread, and no other: what a backend hands the
/// threads its transfers finish on.
pub fn loop_wakeup() -> morf_app::transfer::Wakeup {
    let waker = morf_io::waker();
    std::sync::Arc::new(move || waker.wake())
}

/// Hears what the compositor sent the desktop protocols and hands it to the
/// configuration. Returns whether anything needs drawing again.
///
/// `renderer` is the loop's own, when it has exactly one: a capture is
/// published as an image there, and a capture on the GPU is answered with a
/// buffer it exports (without one, captures go through shared memory).
pub fn dispatch_desktop(
    runtime: &mut Runtime,
    desktop: &mut dyn Desktop,
    mut renderer: Option<&mut RenderEngine<WgpuBackend>>,
) -> Result<bool, String> {
    desktop.dispatch_pending()?;
    let mut repaint = false;
    while let Some(event) = desktop.next_event() {
        match event {
            DesktopEvent::Screencopy { request_id, result } => {
                repaint |=
                    dispatch_screencopy(runtime, renderer.as_deref_mut(), request_id, result);
            }
            DesktopEvent::CaptureOffer {
                request_id,
                width,
                height,
                device,
                formats,
            } => {
                let offer = OfferedCapture {
                    request_id,
                    width,
                    height,
                    device,
                    formats,
                };
                repaint |= answer_capture_offer(runtime, renderer.as_deref_mut(), desktop, offer);
            }
            DesktopEvent::Idle {
                timeout_ms,
                input_only,
                idle,
            } => repaint |= runtime.dispatch_idle(timeout_ms, input_only, idle),
            DesktopEvent::Selection { primary, offer } => {
                let offer = offer.map(|offer| morf_lua::OfferDescription {
                    id: offer.id,
                    mime_types: offer.mime_types,
                    ..morf_lua::OfferDescription::default()
                });
                repaint |= runtime.dispatch_selection(primary, offer);
            }
            DesktopEvent::OfferRead { request_id, result } => {
                repaint |= runtime.dispatch_offer_read(request_id, result);
            }
        }
    }
    Ok(repaint)
}

/// Hands every request a configuration has queued to the compositor.
///
/// One list, called at each of the three points that need it. It used to be
/// written out six times in three different subsets, and the subsets did not
/// agree: the startup copies left out clipboard requests, and the copies that
/// run *after* the event loop — the ones that exist so a request made by an
/// input handler reaches the compositor in the same frame — left out output
/// power. So a key handler that turned a display off was a frame late, every
/// time, for no reason anybody chose.
pub fn apply_service_requests(
    runtime: &mut Runtime,
    client: &mut dyn Backend,
    desktop: &mut dyn Desktop,
) {
    apply_clipboard_requests(runtime, client, desktop);
    crate::input::drag::apply_offer_reads(runtime, client, desktop);
    crate::input::drag::apply_drag_requests(runtime, client);
    apply_virtual_keyboard_requests(runtime, client);
    apply_input_method_requests(runtime, client);
    apply_text_input_requests(runtime, client);
    // The rest is the desktop protocols'. Each is taken from the queue
    // whatever the desktop can do, and handed on only where it can.
    apply_output_power_requests(runtime, desktop);
    apply_screencopy_requests(runtime, desktop);
    publish_windows(runtime, desktop);
    apply_toplevel_requests(runtime, desktop);
}

/// Hands the compositor's window list to the configuration, when it changed.
///
/// Only when it changed. The list is rebuilt from scratch each time — cheap for
/// a dozen windows, and cheaper than a diff that would have to decide what
/// identity means for a renamed window — but doing it every frame would rebuild
/// a dozen Lua tables sixty times a second to say nothing new.
fn publish_windows(runtime: &mut Runtime, desktop: &mut dyn Desktop) {
    if !desktop.capabilities().toplevels || !desktop.take_toplevels_changed() {
        return;
    }
    let windows: Vec<Toplevel> = desktop
        .toplevels()
        .into_iter()
        .map(|window| Toplevel {
            identifier: window.identifier,
            title: window.title,
            app_id: window.app_id,
            activated: window.activated,
            maximized: window.maximized,
            minimized: window.minimized,
            fullscreen: window.fullscreen,
            controllable: window.controllable,
            outputs: window.outputs,
            parent: window.parent,
        })
        .collect();
    runtime.set_windows(&windows);
}

/// Acts on other windows, if the configuration asked.
fn apply_toplevel_requests(runtime: &mut Runtime, desktop: &mut dyn Desktop) {
    let requests = runtime.take_toplevel_requests();
    if !desktop.capabilities().toplevel_control {
        return;
    }
    for request in requests {
        let action = match request.action.as_str() {
            "activate" => ToplevelAction::Activate,
            "close" => ToplevelAction::Close,
            "set_maximized" => ToplevelAction::Maximized(request.value),
            "set_minimized" => ToplevelAction::Minimized(request.value),
            "set_fullscreen" => ToplevelAction::Fullscreen(request.value),
            "set_minimize_target" => match request.rect {
                Some((x, y, width, height)) => ToplevelAction::MinimizeTarget {
                    x,
                    y,
                    width,
                    height,
                },
                None => continue,
            },
            _ => continue,
        };
        desktop.control_toplevel(&request.identifier, action);
    }
}
