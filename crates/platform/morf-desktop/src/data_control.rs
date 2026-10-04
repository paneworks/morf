//! Data control: the clipboard as it changes, without focus.
//!
//! `wl_data_device` tells a client about the selection only while one of its
//! surfaces holds the keyboard, which for a bar is almost never — so a
//! clipboard history built on it misses nearly everything. Data control is the
//! protocol clipboard managers use instead (`wl-paste --watch`, cliphist):
//! every change is announced to every bound client, focused or not, and the
//! client may also set the selection without an input serial.
//!
//! Two spellings of one protocol exist. `ext-data-control-v1` is the
//! standardised one; `zwlr-data-control-unstable-v1` is its wlroots ancestor
//! and still the only one many compositors carry. They are identical but for
//! names, so each is wrapped in the same small enums and handled by one macro.

mod dispatch;

use std::collections::{HashMap, VecDeque};
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use morf_app::mime::resolve_mime;
use morf_app::transfer::{ReadDone, pipe, spawn_read, take_slot};
use wayland_client::backend::ObjectId;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Proxy, QueueHandle};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::ExtDataControlDeviceV1,
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::ExtDataControlOfferV1,
    ext_data_control_source_v1::ExtDataControlSourceV1,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
};

use crate::{Desktop, DesktopEvent, DesktopState};

/// How many announced offers stay readable after a newer one replaced them.
///
/// A configuration reads in response to an announcement, and the clipboard can
/// change again before its read is issued; keeping the last few lets that read
/// still name its offer. Older ones are destroyed.
const LIVE_OFFERS: usize = 4;

/// How many types one offer may list before the rest are ignored.
const MAX_MIME_TYPES: usize = 64;

pub(crate) enum DcManager {
    Ext(ExtDataControlManagerV1),
    Wlr(ZwlrDataControlManagerV1),
}

pub(crate) enum DcDevice {
    Ext(ExtDataControlDeviceV1),
    Wlr(ZwlrDataControlDeviceV1),
}

#[derive(Clone)]
pub(crate) enum DcOffer {
    Ext(ExtDataControlOfferV1),
    Wlr(ZwlrDataControlOfferV1),
}

pub(crate) enum DcSource {
    Ext(ExtDataControlSourceV1),
    Wlr(ZwlrDataControlSourceV1),
}

impl DcOffer {
    pub(crate) fn receive(&self, mime: String, fd: std::os::fd::BorrowedFd<'_>) {
        match self {
            Self::Ext(offer) => offer.receive(mime, fd),
            Self::Wlr(offer) => offer.receive(mime, fd),
        }
    }

    fn destroy(&self) {
        match self {
            Self::Ext(offer) => offer.destroy(),
            Self::Wlr(offer) => offer.destroy(),
        }
    }
}

impl DcSource {
    fn id(&self) -> ObjectId {
        match self {
            Self::Ext(source) => source.id(),
            Self::Wlr(source) => source.id(),
        }
    }

    fn destroy(&self) {
        match self {
            Self::Ext(source) => source.destroy(),
            Self::Wlr(source) => source.destroy(),
        }
    }
}

/// One announced offer a configuration may still read.
pub(crate) struct LiveOffer {
    pub(crate) id: u64,
    pub(crate) offer: DcOffer,
    pub(crate) mime_types: Vec<String>,
}

/// A selection this client set, and the bytes it answers with.
pub(crate) struct OwnedSource {
    source: DcSource,
    /// Each type the source offered with the bytes it stands for.
    data: Vec<(String, Arc<Vec<u8>>)>,
}

/// Where the desktop's offer ids start: far from the window client's own
/// (a drag's), so a read names exactly one of them.
const OFFER_ID_BASE: u64 = 1 << 48;

/// The selection as data control sees it, and the reads of it in flight.
pub(crate) struct ClipboardState {
    control: Option<DataControl>,
    next_offer_id: u64,
    reads: Arc<AtomicUsize>,
    writes: Arc<AtomicUsize>,
    read_tx: mpsc::Sender<ReadDone<u64>>,
    read_rx: mpsc::Receiver<ReadDone<u64>>,
    /// Rung when a read finishes on its thread, so the loop wakes for it.
    waker: Option<fn()>,
}

impl ClipboardState {
    /// Data control: the standard spelling first, the wlroots one where it
    /// is all there is. Version 2 of the latter adds the primary selection.
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<DesktopState>) -> Self {
        let control = globals
            .bind::<ExtDataControlManagerV1, _, _>(qh, 1..=1, ())
            .map(DcManager::Ext)
            .or_else(|_| {
                globals
                    .bind::<ZwlrDataControlManagerV1, _, _>(qh, 1..=2, ())
                    .map(DcManager::Wlr)
            })
            .ok()
            .map(DataControl::new);
        let (read_tx, read_rx) = mpsc::channel();
        Self {
            control,
            next_offer_id: OFFER_ID_BASE,
            reads: Arc::default(),
            writes: Arc::default(),
            read_tx,
            read_rx,
            waker: None,
        }
    }

    /// Watches the selection on `seat` (the first: a second seat's clipboard
    /// is a second clipboard, and no shell has asked for two yet).
    pub(crate) fn seat_added(&mut self, seat: &WlSeat, qh: &QueueHandle<DesktopState>) {
        if let Some(control) = &mut self.control {
            control.ensure_device(seat, qh);
        }
    }

    /// Reads that finished, as events.
    pub(crate) fn drain_reads(&mut self, events: &mut VecDeque<DesktopEvent>) {
        while let Ok(done) = self.read_rx.try_recv() {
            events.push_back(DesktopEvent::OfferRead {
                request_id: done.tag,
                result: done.result,
            });
        }
    }

    fn live(&self, offer_id: u64) -> Option<&LiveOffer> {
        self.control
            .as_ref()
            .and_then(|control| control.live.iter().find(|live| live.id == offer_id))
    }
}

impl Desktop {
    /// Rings `waker` whenever a read finishes on its thread.
    pub fn set_waker(&mut self, waker: fn()) {
        self.state.clipboard.waker = Some(waker);
    }

    /// Whether the selection can be watched and set without focus.
    pub fn supports_data_control(&self) -> bool {
        self.state
            .clipboard
            .control
            .as_ref()
            .is_some_and(|control| control.device.is_some())
    }

    /// Which data-control protocol is bound, if any.
    pub fn data_control_protocol(&self) -> Option<&'static str> {
        self.state
            .clipboard
            .control
            .as_ref()
            .map(DataControl::protocol)
    }

    /// Whether the primary selection can be watched and set too.
    pub fn supports_primary_selection(&self) -> bool {
        self.supports_data_control()
            && self
                .state
                .clipboard
                .control
                .as_ref()
                .is_some_and(DataControl::supports_primary)
    }

    /// Owns the selection, offering each type with its bytes. No focus or
    /// input serial needed.
    pub fn set_selection(&mut self, data: Vec<(String, Arc<Vec<u8>>)>, primary: bool) -> bool {
        let qh = self.handle();
        self.state
            .clipboard
            .control
            .as_mut()
            .is_some_and(|control| control.set_selection(data, primary, &qh))
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self, primary: bool) -> bool {
        self.state
            .clipboard
            .control
            .as_mut()
            .is_some_and(|control| control.clear_selection(primary))
    }

    /// Whether `offer_id` is one of the selection offers announced here.
    pub fn owns_offer(&self, offer_id: u64) -> bool {
        offer_id > OFFER_ID_BASE
    }

    /// Reads one type from an announced offer, off the loop.
    ///
    /// `mime` may be a shorthand -- `text`, `image`, `uris` -- resolved
    /// against what the offer lists. The answer arrives as
    /// [`DesktopEvent::OfferRead`] with the same `request_id`, failures
    /// included, so a caller has exactly one place to hear back.
    pub fn read_offer(&mut self, request_id: u64, offer_id: u64, mime: &str) {
        if let Err(error) = self.start_offer_read(request_id, offer_id, mime) {
            self.state.events.push_back(DesktopEvent::OfferRead {
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
        let clipboard = &self.state.clipboard;
        let live = clipboard
            .live(offer_id)
            .ok_or_else(|| "the offer is gone".to_owned())?;
        let mime = resolve_mime(mime, &live.mime_types)
            .ok_or_else(|| format!("the offer has no `{mime}`"))?;
        if !take_slot(&clipboard.reads) {
            return Err("too many reads in flight".to_owned());
        }
        let (reader, writer) = match pipe() {
            Ok(pair) => pair,
            Err(error) => {
                clipboard.reads.fetch_sub(1, Ordering::Relaxed);
                return Err(error);
            }
        };
        live.offer.receive(mime, writer.as_fd());
        drop(writer);
        spawn_read(
            reader,
            request_id,
            clipboard.read_tx.clone(),
            clipboard.waker,
            Arc::clone(&clipboard.reads),
        );
        let _ = self.connection.flush();
        Ok(())
    }
}

/// Everything data control needs to remember between events.
pub(crate) struct DataControl {
    pub(crate) manager: DcManager,
    pub(crate) device: Option<DcDevice>,
    /// Types each offer has listed so far, before it is announced.
    pending: HashMap<ObjectId, Vec<String>>,
    /// Announced offers, oldest first.
    pub(crate) live: VecDeque<LiveOffer>,
    /// Selections this client currently owns.
    sources: Vec<OwnedSource>,
}

impl DataControl {
    pub(crate) fn new(manager: DcManager) -> Self {
        Self {
            manager,
            device: None,
            pending: HashMap::new(),
            live: VecDeque::new(),
            sources: Vec::new(),
        }
    }

    /// Whether the primary selection (middle-click paste) can be watched and set.
    pub(crate) fn supports_primary(&self) -> bool {
        match &self.manager {
            DcManager::Ext(_) => true,
            DcManager::Wlr(manager) => manager.version() >= 2,
        }
    }

    /// The protocol in use, for a configuration that wants to know.
    pub(crate) fn protocol(&self) -> &'static str {
        match self.manager {
            DcManager::Ext(_) => "ext-data-control-v1",
            DcManager::Wlr(_) => "zwlr-data-control-v1",
        }
    }

    /// Binds the device for a seat, once.
    pub(crate) fn ensure_device(&mut self, seat: &WlSeat, qh: &QueueHandle<DesktopState>) {
        if self.device.is_some() {
            return;
        }
        self.device = Some(match &self.manager {
            DcManager::Ext(manager) => DcDevice::Ext(manager.get_data_device(seat, qh, ())),
            DcManager::Wlr(manager) => DcDevice::Wlr(manager.get_data_device(seat, qh, ())),
        });
    }

    /// Makes this client the owner of the selection, offering `data`.
    pub(crate) fn set_selection(
        &mut self,
        data: Vec<(String, Arc<Vec<u8>>)>,
        primary: bool,
        qh: &QueueHandle<DesktopState>,
    ) -> bool {
        if primary && !self.supports_primary() {
            return false;
        }
        let Some(device) = &self.device else {
            return false;
        };
        let source = match &self.manager {
            DcManager::Ext(manager) => DcSource::Ext(manager.create_data_source(qh, ())),
            DcManager::Wlr(manager) => DcSource::Wlr(manager.create_data_source(qh, ())),
        };
        for (mime, _) in &data {
            match &source {
                DcSource::Ext(source) => source.offer(mime.clone()),
                DcSource::Wlr(source) => source.offer(mime.clone()),
            }
        }
        match (device, &source) {
            (DcDevice::Ext(device), DcSource::Ext(source)) if primary => {
                device.set_primary_selection(Some(source));
            }
            (DcDevice::Ext(device), DcSource::Ext(source)) => device.set_selection(Some(source)),
            (DcDevice::Wlr(device), DcSource::Wlr(source)) if primary => {
                device.set_primary_selection(Some(source));
            }
            (DcDevice::Wlr(device), DcSource::Wlr(source)) => device.set_selection(Some(source)),
            _ => unreachable!("a device and its sources share one protocol"),
        }
        // The compositor cancels the previous source as this one replaces it,
        // so the list stays at one or two entries.
        self.sources.push(OwnedSource { source, data });
        true
    }

    /// Clears the selection, when this client may.
    pub(crate) fn clear_selection(&mut self, primary: bool) -> bool {
        if primary && !self.supports_primary() {
            return false;
        }
        match &self.device {
            Some(DcDevice::Ext(device)) if primary => device.set_primary_selection(None),
            Some(DcDevice::Ext(device)) => device.set_selection(None),
            Some(DcDevice::Wlr(device)) if primary => device.set_primary_selection(None),
            Some(DcDevice::Wlr(device)) => device.set_selection(None),
            None => return false,
        }
        true
    }

    fn offer_listed(&mut self, offer: ObjectId, mime: String) {
        let types = self.pending.entry(offer).or_default();
        if types.len() < MAX_MIME_TYPES && !types.contains(&mime) {
            types.push(mime);
        }
    }

    fn remember(&mut self, id: u64, offer: DcOffer, mime_types: Vec<String>) {
        self.live.push_back(LiveOffer {
            id,
            offer,
            mime_types,
        });
        while self.live.len() > LIVE_OFFERS {
            if let Some(old) = self.live.pop_front() {
                old.offer.destroy();
            }
        }
    }

    fn source_data(&self, source: &ObjectId, mime: &str) -> Option<Arc<Vec<u8>>> {
        self.sources
            .iter()
            .find(|owned| owned.source.id() == *source)
            .and_then(|owned| {
                owned
                    .data
                    .iter()
                    .find(|(offered, _)| offered == mime)
                    .map(|(_, bytes)| Arc::clone(bytes))
            })
    }

    fn cancel_source(&mut self, source: &ObjectId) {
        self.sources.retain(|owned| {
            let keep = owned.source.id() != *source;
            if !keep {
                owned.source.destroy();
            }
            keep
        });
    }

    fn finished(&mut self) {
        match self.device.take() {
            Some(DcDevice::Ext(device)) => device.destroy(),
            Some(DcDevice::Wlr(device)) => device.destroy(),
            None => {}
        }
    }
}
