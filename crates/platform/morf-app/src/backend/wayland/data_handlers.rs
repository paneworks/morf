use smithay_client_toolkit::data_device_manager::WritePipe;
use smithay_client_toolkit::data_device_manager::data_device::DataDeviceHandler;
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::DataSourceHandler;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::{wl_data_device, wl_data_source, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::backend::wayland::mime::{URI_LIST_MIME, best_text_mime};
use crate::backend::wayland::offer_io::{ReadTag, pipe, spawn_read, spawn_write, take_slot};
use crate::backend::wayland::{state_types::*, surface_types::*};

impl LayerState {
    /// The drag offer SCTK is holding for one data device.
    pub(crate) fn drag_offer(
        &self,
        data_device: &wl_data_device::WlDataDevice,
    ) -> Option<DragOffer> {
        self.data_devices
            .iter()
            .find(|device| device.inner() == data_device)
            .and_then(|device| device.data().drag_offer())
    }

    /// Any live drag offer, for requests that do not come with a device.
    pub(crate) fn any_drag_offer(&self) -> Option<DragOffer> {
        self.data_devices
            .iter()
            .find_map(|device| device.data().drag_offer())
    }

    /// Starts one read of the drag offer into a pipe.
    pub(crate) fn read_drag(
        &self,
        offer: &DragOffer,
        mime: &str,
        tag: ReadTag,
    ) -> Result<(), String> {
        if !take_slot(&self.clipboard_reads) {
            return Err("too many reads in flight".to_owned());
        }
        let (reader, writer) = match pipe() {
            Ok(pair) => pair,
            Err(error) => {
                self.clipboard_reads.fetch_sub(1, Ordering::Relaxed);
                return Err(error);
            }
        };
        offer.inner().receive(mime.to_owned(), writer.as_fd());
        drop(writer);
        spawn_read(
            reader,
            tag,
            self.read_tx.clone(),
            self.waker,
            Arc::clone(&self.clipboard_reads),
        );
        Ok(())
    }

    /// Announces the drop once everything fetched ahead of it has arrived.
    pub(crate) fn announce_drop_if_ready(&mut self) {
        let Some(drag) = &self.drag else {
            return;
        };
        if !drag.dropped || drag.awaiting > 0 || drag.finished || drag.announced {
            return;
        }
        let drop = DropInfo {
            offer: OfferInfo {
                id: drag.id,
                mime_types: drag.mime_types.clone(),
            },
            accepted: drag.accepted.clone(),
            uris: drag.uris.clone(),
            text: drag.text.clone(),
        };
        let event = LayerEvent::Drop {
            surface: drag.surface,
            x: drag.x,
            y: drag.y,
            drop: Box::new(drop),
        };
        if let Some(drag) = &mut self.drag {
            drag.announced = true;
        }
        self.events.push_back(event);
    }
}

impl DataDeviceHandler for LayerState {
    fn enter(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
        surface: &wl_surface::WlSurface,
    ) {
        let Some(role) = self.surface_role(surface) else {
            return;
        };
        let Some(offer) = self.drag_offer(data_device) else {
            // A drag with no data: this client's own internal drag, if it
            // ever starts one. Nothing to offer a target.
            return;
        };
        let mime_types = offer.with_mime_types(<[String]>::to_vec);
        self.next_offer_id += 1;
        let id = self.next_offer_id;
        self.drag = Some(DragState {
            id,
            surface: role,
            mime_types: mime_types.clone(),
            x,
            y,
            serial: offer.serial,
            accepted: None,
            dropped: false,
            finished: false,
            announced: false,
            awaiting: 0,
            uris: Vec::new(),
            text: None,
        });
        // Refused until a target says otherwise: the host answers this event
        // with `accept_drag` once it has hit-tested the point.
        offer.accept_mime_type(offer.serial, None);
        self.events.push_back(LayerEvent::DragEnter {
            surface: role,
            x,
            y,
            offer: OfferInfo { id, mime_types },
        });
    }

    fn leave(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &wl_data_device::WlDataDevice,
    ) {
        let Some(drag) = &self.drag else {
            return;
        };
        // A drop is followed by a leave; the drop keeps its state until it is
        // finished, and a target has already been told about it.
        if drag.dropped {
            return;
        }
        let surface = drag.surface;
        self.drag = None;
        self.events.push_back(LayerEvent::DragLeave { surface });
    }

    fn motion(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
    ) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        drag.x = x;
        drag.y = y;
        let surface = drag.surface;
        self.events
            .push_back(LayerEvent::DragMotion { surface, x, y });
    }

    fn selection(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
    ) {
        let Some(offer) = self
            .data_devices
            .iter()
            .find(|device| device.inner() == data_device)
            .and_then(|device| device.data().selection_offer())
        else {
            self.events.push_back(LayerEvent::Clipboard { text: None });
            return;
        };
        let mime = offer.with_mime_types(|types| best_text_mime(types).map(str::to_owned));
        let Some(mime) = mime else {
            self.events.push_back(LayerEvent::Clipboard { text: None });
            return;
        };
        let Ok(pipe) = offer.receive(mime) else {
            self.events.push_back(LayerEvent::Clipboard { text: None });
            return;
        };
        if !take_slot(&self.clipboard_reads) {
            return;
        }
        let tx = self.clipboard_tx.clone();
        let active = Arc::clone(&self.clipboard_reads);
        let waker = self.waker;
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let text = pipe
                .take(1_048_577)
                .read_to_end(&mut bytes)
                .ok()
                .filter(|_| bytes.len() <= 1_048_576)
                .and_then(|_| String::from_utf8(bytes).ok());
            let _ = tx.send(text);
            active.fetch_sub(1, Ordering::Relaxed);
            if let Some(wake) = waker {
                wake();
            }
        });
    }

    fn drop_performed(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
    ) {
        let Some(offer) = self.drag_offer(data_device) else {
            return;
        };
        let Some(drag) = &mut self.drag else {
            return;
        };
        drag.dropped = true;
        let id = drag.id;
        let mime_types = drag.mime_types.clone();
        // Fetched now, while the source is certainly still there, so the drop
        // can hand a target its files and its text as plain values. Anything
        // else it reads for itself, inside the drop handler.
        let mut awaiting = 0;
        if mime_types.iter().any(|mime| mime == URI_LIST_MIME)
            && self
                .read_drag(&offer, URI_LIST_MIME, ReadTag::DropUris(id))
                .is_ok()
        {
            awaiting += 1;
        }
        if let Some(text) = best_text_mime(&mime_types)
            && self.read_drag(&offer, text, ReadTag::DropText(id)).is_ok()
        {
            awaiting += 1;
        }
        if let Some(drag) = &mut self.drag {
            drag.awaiting = awaiting;
        }
        self.announce_drop_if_ready();
    }
}

impl DataOfferHandler for LayerState {
    fn source_actions(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _actions: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _actions: DndAction,
    ) {
    }
}

impl DataSourceHandler for LayerState {
    fn accept_mime(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wl_data_source::WlDataSource,
        _mime: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
        mime: String,
        mut pipe: WritePipe,
    ) {
        if let Some(drag) = self
            .drag_source
            .as_ref()
            .filter(|drag| drag.source.inner() == source)
        {
            let Some(bytes) = drag
                .data
                .iter()
                .find(|(offered, _)| *offered == mime)
                .map(|(_, bytes)| Arc::clone(bytes))
            else {
                return;
            };
            // A duplicate of the descriptor, so the writer owns its own; the
            // original closes with `pipe` here.
            let Ok(fd) = pipe.as_fd().try_clone_to_owned() else {
                return;
            };
            if take_slot(&self.clipboard_writes) {
                spawn_write(fd, bytes, Arc::clone(&self.clipboard_writes));
            }
            return;
        }
        let Some(text) = self
            .clipboard_source
            .as_ref()
            .filter(|current| current.inner() == source)
            .map(|_| self.clipboard_text.clone())
        else {
            return;
        };
        if !take_slot(&self.clipboard_writes) {
            return;
        }
        let active = Arc::clone(&self.clipboard_writes);
        thread::spawn(move || {
            let _ = pipe.write_all(text.as_bytes());
            active.fetch_sub(1, Ordering::Relaxed);
        });
    }

    fn cancelled(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if self
            .drag_source
            .as_ref()
            .is_some_and(|drag| drag.source.inner() == source)
        {
            self.drag_source = None;
            self.events
                .push_back(LayerEvent::DragSourceEnded { dropped: false });
            return;
        }
        if self
            .clipboard_source
            .as_ref()
            .is_some_and(|current| current.inner() == source)
        {
            self.clipboard_source = None;
        }
    }

    fn dnd_dropped(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wl_data_source::WlDataSource,
    ) {
    }

    fn dnd_finished(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if self
            .drag_source
            .as_ref()
            .is_some_and(|drag| drag.source.inner() == source)
        {
            self.drag_source = None;
            self.events
                .push_back(LayerEvent::DragSourceEnded { dropped: true });
        }
    }

    fn action(
        &mut self,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wl_data_source::WlDataSource,
        _action: DndAction,
    ) {
    }
}
