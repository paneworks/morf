//! The client's side of drags: reading what one carries, accepting it,
//! starting one. (The selection, watched and set without focus, is
//! `morf-desktop`'s data control.)

use std::sync::Arc;
use wayland_client::protocol::wl_data_device_manager::DndAction;

use crate::mime::{parse_uri_list, resolve_mime};
use crate::backend::wayland::{state_types::*, surface_types::*};

/// What a finished read was for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReadTag {
    /// A configuration asked for it, under this request id.
    Request(u64),
    /// Fetched ahead of a drop so the drop can carry it: the URI list.
    DropUris(u64),
    /// Fetched ahead of a drop so the drop can carry it: the text.
    DropText(u64),
}

impl LayerClient {
    /// A function a transfer thread calls when it finishes, so a loop asleep
    /// in [`Self::dispatch_timeout_or`] wakes for the result at once.
    pub fn set_waker(&mut self, waker: fn()) {
        self.state.waker = Some(waker);
    }

    /// Whether drags can come in and go out: a `wl_data_device` exists.
    pub fn supports_drag_and_drop(&self) -> bool {
        self.supports_clipboard()
    }

    /// Reads one type from an announced offer, off the loop.
    ///
    /// `mime` may be a shorthand — `text`, `image`, `uris` — resolved against
    /// what the offer lists. The answer arrives as [`Event::OfferRead`]
    /// with the same `request_id`, failures included, so a caller has exactly
    /// one place to hear back.
    pub fn read_offer(&mut self, request_id: u64, offer_id: u64, mime: &str) {
        let result = self.start_offer_read(request_id, offer_id, mime);
        if let Err(error) = result {
            self.state.events.push_back(Event::OfferRead {
                request_id,
                result: Err(error),
            });
        }
    }

    fn start_offer_read(
        &mut self,
        request_id: u64,
        offer_id: u64,
        mime: &str,
    ) -> Result<(), String> {
        let state = &mut self.state;
        let Some(drag) = state.drag.as_ref().filter(|drag| drag.id == offer_id) else {
            return Err("the offer is gone".to_owned());
        };
        if drag.finished {
            return Err("the drop is already finished".to_owned());
        }
        let mime = resolve_mime(mime, &drag.mime_types)
            .ok_or_else(|| format!("the drag has no `{mime}`"))?;
        let offer = state
            .any_drag_offer()
            .ok_or_else(|| "the drag is gone".to_owned())?;
        state.read_drag(&offer, &mime, ReadTag::Request(request_id))?;
        let _ = self.connection.flush();
        Ok(())
    }

    /// Says which type, if any, the target under the drag would take.
    ///
    /// Sent only when the answer changes. A drag that is refused everywhere is
    /// cancelled by the compositor when it is let go, which is right: nothing
    /// here wanted it.
    pub fn accept_drag(&mut self, mime: Option<&str>) {
        let Some(drag) = &mut self.state.drag else {
            return;
        };
        if drag.dropped || drag.accepted.as_deref() == mime {
            return;
        }
        drag.accepted = mime.map(str::to_owned);
        let serial = drag.serial;
        let Some(offer) = self.state.any_drag_offer() else {
            return;
        };
        offer.accept_mime_type(serial, mime.map(str::to_owned));
        // Copy only. A move would let the source delete what it dragged once
        // the drop is finished, and nothing here keeps it.
        if mime.is_some() {
            offer.set_actions(DndAction::Copy, DndAction::Copy);
        } else {
            offer.set_actions(DndAction::empty(), DndAction::empty());
        }
    }

    /// Tells the source the drop is done with; nothing more may be read.
    pub fn finish_drop(&mut self) {
        let Some(drag) = &mut self.state.drag else {
            return;
        };
        if !drag.dropped || drag.finished {
            return;
        }
        drag.finished = true;
        if let Some(offer) = self.state.any_drag_offer() {
            offer.finish();
        }
        self.state.drag = None;
    }

    /// Starts a drag out of this client, offering each type with its bytes.
    ///
    /// Only from inside a pointer press: the compositor wants the serial of
    /// the button that began it and the surface it began on, and refuses a
    /// drag it cannot tie to one.
    pub fn start_drag(&mut self, data: Vec<(String, Arc<Vec<u8>>)>) -> bool {
        let Some(manager) = &self.state.data_device_manager else {
            return false;
        };
        let Some(device) = self.state.data_devices.first() else {
            return false;
        };
        let (Some(surface), Some(serial)) = (
            self.state.pressed_surface.clone(),
            self.state.latest_input_serial,
        ) else {
            return false;
        };
        if data.is_empty() {
            return false;
        }
        let source = manager.create_drag_and_drop_source(
            &self.queue.handle(),
            data.iter().map(|(mime, _)| mime.clone()),
            DndAction::Copy,
        );
        source.start_drag(device, &surface, None, serial);
        self.state.drag_source = Some(OwnedDrag { source, data });
        true
    }
}

impl LayerState {
    /// Turns finished reads into events, and completes a waiting drop.
    pub(crate) fn drain_reads(&mut self) {
        while let Ok(done) = self.read_rx.try_recv() {
            match done.tag {
                ReadTag::Request(request_id) => {
                    self.events.push_back(Event::OfferRead {
                        request_id,
                        result: done.result,
                    });
                }
                ReadTag::DropUris(id) | ReadTag::DropText(id) => {
                    let Some(drag) = self.drag.as_mut().filter(|drag| drag.id == id) else {
                        continue;
                    };
                    if let Ok(bytes) = &done.result {
                        if matches!(done.tag, ReadTag::DropUris(_)) {
                            drag.uris = parse_uri_list(bytes);
                        } else {
                            drag.text = Some(String::from_utf8_lossy(bytes).into_owned());
                        }
                    }
                    drag.awaiting = drag.awaiting.saturating_sub(1);
                    self.announce_drop_if_ready();
                }
            }
        }
    }
}
