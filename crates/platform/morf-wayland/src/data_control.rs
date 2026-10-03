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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

use crate::offer_io::{spawn_write, take_slot};
use crate::{state_types::*, surface_types::*, types::*};

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
    pub(crate) fn ensure_device(&mut self, seat: &WlSeat, qh: &QueueHandle<LayerState>) {
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
        qh: &QueueHandle<LayerState>,
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

impl LayerState {
    /// Announces a new selection offer, or its absence.
    fn data_control_selection(
        &mut self,
        offer: Option<DcOffer>,
        object: Option<ObjectId>,
        primary: bool,
    ) {
        let Some(control) = &mut self.data_control else {
            return;
        };
        let announced = match (offer, object) {
            (Some(offer), Some(object)) => {
                let mime_types = control.pending.remove(&object).unwrap_or_default();
                self.next_offer_id += 1;
                let id = self.next_offer_id;
                control.remember(id, offer, mime_types.clone());
                Some(OfferInfo { id, mime_types })
            }
            _ => None,
        };
        self.events.push_back(LayerEvent::Selection {
            primary,
            offer: announced,
        });
    }

    fn data_control_send(&mut self, source: ObjectId, mime: String, fd: std::os::fd::OwnedFd) {
        let Some(control) = &self.data_control else {
            return;
        };
        let Some(bytes) = control.source_data(&source, &mime) else {
            return;
        };
        if take_slot(&self.clipboard_writes) {
            spawn_write(fd, bytes, Arc::clone(&self.clipboard_writes));
        }
    }
}

/// Implements the four interfaces for one spelling of the protocol.
macro_rules! data_control_dispatch {
    (
        $manager:ty,
        $device:ty, $device_mod:ident,
        $offer:ty, $offer_mod:ident,
        $source:ty, $source_mod:ident,
        $variant:ident
    ) => {
        impl Dispatch<$manager, ()> for LayerState {
            fn event(
                _state: &mut Self,
                _proxy: &$manager,
                _event: <$manager as Proxy>::Event,
                _data: &(),
                _connection: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl Dispatch<$device, ()> for LayerState {
            fn event(
                state: &mut Self,
                _proxy: &$device,
                event: $device_mod::Event,
                _data: &(),
                _connection: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                match event {
                    $device_mod::Event::DataOffer { id } => {
                        if let Some(control) = &mut state.data_control {
                            control.pending.insert(id.id(), Vec::new());
                        }
                    }
                    $device_mod::Event::Selection { id } => {
                        let object = id.as_ref().map(Proxy::id);
                        state.data_control_selection(id.map(DcOffer::$variant), object, false);
                    }
                    $device_mod::Event::PrimarySelection { id } => {
                        let object = id.as_ref().map(Proxy::id);
                        state.data_control_selection(id.map(DcOffer::$variant), object, true);
                    }
                    $device_mod::Event::Finished => {
                        if let Some(control) = &mut state.data_control {
                            control.finished();
                        }
                    }
                    _ => {}
                }
            }

            wayland_client::event_created_child!(LayerState, $device, [
                $device_mod::EVT_DATA_OFFER_OPCODE => ($offer, ())
            ]);
        }

        impl Dispatch<$offer, ()> for LayerState {
            fn event(
                state: &mut Self,
                proxy: &$offer,
                event: $offer_mod::Event,
                _data: &(),
                _connection: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                if let $offer_mod::Event::Offer { mime_type } = event
                    && let Some(control) = &mut state.data_control
                {
                    control.offer_listed(proxy.id(), mime_type);
                }
            }
        }

        impl Dispatch<$source, ()> for LayerState {
            fn event(
                state: &mut Self,
                proxy: &$source,
                event: $source_mod::Event,
                _data: &(),
                _connection: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                match event {
                    $source_mod::Event::Send { mime_type, fd } => {
                        state.data_control_send(proxy.id(), mime_type, fd);
                    }
                    $source_mod::Event::Cancelled => {
                        if let Some(control) = &mut state.data_control {
                            control.cancel_source(&proxy.id());
                        }
                    }
                    _ => {}
                }
            }
        }
    };
}

data_control_dispatch!(
    ExtDataControlManagerV1,
    ExtDataControlDeviceV1,
    ext_data_control_device_v1,
    ExtDataControlOfferV1,
    ext_data_control_offer_v1,
    ExtDataControlSourceV1,
    ext_data_control_source_v1,
    Ext
);

data_control_dispatch!(
    ZwlrDataControlManagerV1,
    ZwlrDataControlDeviceV1,
    zwlr_data_control_device_v1,
    ZwlrDataControlOfferV1,
    zwlr_data_control_offer_v1,
    ZwlrDataControlSourceV1,
    zwlr_data_control_source_v1,
    Wlr
);
