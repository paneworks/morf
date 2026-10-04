//! The protocol's events: one macro implements the four interfaces, once for
//! each spelling, and hands selections and send requests to the state.

use std::sync::Arc;

use morf_app::OfferInfo;
use morf_app::transfer::{spawn_write, take_slot};
use wayland_client::backend::ObjectId;
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

use crate::{DesktopEvent, DesktopState};

use super::DcOffer;

impl DesktopState {
    /// Announces a new selection offer, or its absence.
    fn data_control_selection(
        &mut self,
        offer: Option<DcOffer>,
        object: Option<ObjectId>,
        primary: bool,
    ) {
        let Some(control) = &mut self.clipboard.control else {
            return;
        };
        let announced = match (offer, object) {
            (Some(offer), Some(object)) => {
                let mime_types = control.pending.remove(&object).unwrap_or_default();
                self.clipboard.next_offer_id += 1;
                let id = self.clipboard.next_offer_id;
                control.remember(id, offer, mime_types.clone());
                Some(OfferInfo { id, mime_types })
            }
            _ => None,
        };
        self.events.push_back(DesktopEvent::Selection {
            primary,
            offer: announced,
        });
    }

    fn data_control_send(&mut self, source: ObjectId, mime: String, fd: std::os::fd::OwnedFd) {
        let Some(control) = &self.clipboard.control else {
            return;
        };
        let Some(bytes) = control.source_data(&source, &mime) else {
            return;
        };
        if take_slot(&self.clipboard.writes) {
            spawn_write(fd, bytes, Arc::clone(&self.clipboard.writes));
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
        impl Dispatch<$manager, ()> for DesktopState {
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

        impl Dispatch<$device, ()> for DesktopState {
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
                        if let Some(control) = &mut state.clipboard.control {
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
                        if let Some(control) = &mut state.clipboard.control {
                            control.finished();
                        }
                    }
                    _ => {}
                }
            }

            wayland_client::event_created_child!(DesktopState, $device, [
                $device_mod::EVT_DATA_OFFER_OPCODE => ($offer, ())
            ]);
        }

        impl Dispatch<$offer, ()> for DesktopState {
            fn event(
                state: &mut Self,
                proxy: &$offer,
                event: $offer_mod::Event,
                _data: &(),
                _connection: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                if let $offer_mod::Event::Offer { mime_type } = event
                    && let Some(control) = &mut state.clipboard.control
                {
                    control.offer_listed(proxy.id(), mime_type);
                }
            }
        }

        impl Dispatch<$source, ()> for DesktopState {
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
                        if let Some(control) = &mut state.clipboard.control {
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
