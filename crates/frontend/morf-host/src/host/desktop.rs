//! The desktop protocols, beside a window client on its connection.

use morf_app::Backend;
use morf_desktop::{Desktop, DesktopEvent};
use morf_lua::Runtime;
use morf_render::{RenderEngine, WgpuBackend};

use crate::capture::{OfferedCapture, answer_capture_offer, dispatch_screencopy};

/// The desktop protocols on `client`'s connection; a request naming no
/// output is for the one `client`'s surface sits on.
pub fn desktop_for(client: &dyn Backend) -> Result<Desktop, String> {
    let client = client
        .as_wayland()
        .ok_or_else(|| "the desktop protocols need a compositor".to_owned())?;
    let mut desktop = Desktop::new(client.connection())?;
    // A selection read finishing on its thread rings every loop, so the
    // loop wakes for its answer rather than sleeping past it.
    desktop.set_waker(morf_io::wake_all);
    desktop.set_own_output(client.own_output().and_then(|output| output.name));
    desktop.set_shell_surface(client.primary_surface());
    Ok(desktop)
}

/// Hears what the compositor sent the desktop protocols and hands it to the
/// configuration. Returns whether anything needs drawing again.
///
/// `renderer` is the loop's own, when it has exactly one: a capture is
/// published as an image there, and a capture on the GPU is answered with a
/// buffer it exports (without one, captures go through shared memory).
pub fn dispatch_desktop(
    runtime: &mut Runtime,
    desktop: &mut Desktop,
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
