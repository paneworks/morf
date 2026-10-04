//! The desktop protocols, beside a window client on its connection.

use morf_app::LayerClient;
use morf_desktop::{Desktop, DesktopEvent};
use morf_lua::Runtime;

/// The desktop protocols on `client`'s connection; a request naming no
/// output is for the one `client`'s surface sits on.
pub(crate) fn desktop_for(client: &LayerClient) -> Result<Desktop, String> {
    let mut desktop = Desktop::new(client.connection())?;
    // A selection read finishing on its thread rings every loop, so the
    // loop wakes for its answer rather than sleeping past it.
    desktop.set_waker(morf_io::wake_all);
    desktop.set_own_output(client.own_output().and_then(|output| output.name));
    Ok(desktop)
}

/// Hears what the compositor sent the desktop protocols and hands it to the
/// configuration. Returns whether anything needs drawing again.
pub(crate) fn dispatch_desktop(runtime: &mut Runtime, desktop: &mut Desktop) -> Result<bool, String> {
    desktop.dispatch_pending()?;
    let mut repaint = false;
    while let Some(event) = desktop.next_event() {
        match event {
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
