//! Colour management, the buffer sink's half: telling the compositor how
//! the surface's buffers are encoded, on the sink's own queue.
//!
//! One parametric image description per encoding asked for -- PQ over
//! BT.2020, or extended linear over sRGB primaries -- set on the surface's
//! `wp_color_management_surface_v1` and taken with its next commit, the
//! frame drawn for it.

use morf_value::present::Encoding;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_management_surface_v1::WpColorManagementSurfaceV1,
    wp_color_manager_v1::{self, Primaries, RenderIntent, TransferFunction, WpColorManagerV1},
    wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1,
    wp_image_description_v1::{self, WpImageDescriptionV1},
};

use super::buffer_sink::{SinkState, WaylandBufferSink};
use super::color::note_support;

/// The reference white PQ implies when none is set, in cd/m².
pub(crate) const PQ_REFERENCE_WHITE: u32 = 203;

impl Dispatch<WpColorManagerV1, ()> for SinkState {
    fn event(
        state: &mut Self,
        _: &WpColorManagerV1,
        event: wp_color_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        note_support(&mut state.color, event);
    }
}

impl Dispatch<WpImageDescriptionV1, ()> for SinkState {
    fn event(
        state: &mut Self,
        _: &WpImageDescriptionV1,
        event: wp_image_description_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wp_image_description_v1::Event;
        match event {
            Event::Ready { .. } | Event::Ready2 { .. } => state.described = Some(Ok(())),
            Event::Failed { msg, .. } => state.described = Some(Err(msg)),
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(SinkState: ignore WpColorManagementSurfaceV1);
wayland_client::delegate_noop!(SinkState: ignore WpImageDescriptionCreatorParamsV1);

impl WaylandBufferSink {
    /// Describes the surface's buffers as `encoding` from the next commit.
    pub(super) fn describe(&mut self, encoding: Encoding) -> Result<(), String> {
        if encoding == Encoding::Srgb {
            if let Some(surface) = &self.color_surface {
                surface.unset_image_description();
            }
            return Ok(());
        }
        let manager = self
            .color_manager
            .clone()
            .ok_or("the compositor has no wp_color_manager_v1")?;
        let support = self.state.color;
        let qh = self.queue.handle();
        let creator = match encoding {
            Encoding::Pq { reference_white } => {
                if !support.can_pq() {
                    return Err("the compositor cannot describe PQ over BT.2020".to_owned());
                }
                let moved = reference_white != PQ_REFERENCE_WHITE;
                if moved && !support.luminances {
                    return Err("the compositor takes no reference white".to_owned());
                }
                let creator = manager.create_parametric_creator(&qh, ());
                creator.set_tf_named(TransferFunction::St2084Pq);
                creator.set_primaries_named(Primaries::Bt2020);
                if moved {
                    // 0.005 cd/m² as PQ implies; the peak is ignored for PQ.
                    creator.set_luminances(50, 10_000, reference_white);
                }
                creator
            }
            _ => {
                if !support.can_scrgb() {
                    return Err("the compositor cannot describe extended linear sRGB".to_owned());
                }
                // sRGB's default luminances make 1.0 its reference white.
                let creator = manager.create_parametric_creator(&qh, ());
                creator.set_tf_named(TransferFunction::ExtLinear);
                creator.set_primaries_named(Primaries::Srgb);
                creator
            }
        };
        let description = creator.create(&qh, ());
        // `ready` may come after a later roundtrip than the first; a few are
        // waited for, never forever.
        self.state.described = None;
        for _ in 0..8 {
            if self.state.described.is_some() {
                break;
            }
            if let Err(error) = self.queue.roundtrip(&mut self.state) {
                description.destroy();
                return Err(format!("the compositor did not answer: {error}"));
            }
        }
        let described = self
            .state
            .described
            .take()
            .unwrap_or_else(|| Err("no answer".to_owned()));
        if described.is_ok() {
            let surface = match &self.color_surface {
                Some(surface) => surface.clone(),
                None => {
                    let surface = manager.get_surface(&self.surface, &qh, ());
                    self.color_surface = Some(surface.clone());
                    surface
                }
            };
            surface.set_image_description(&description, RenderIntent::Perceptual);
        }
        description.destroy();
        let _ = self.connection.flush();
        described.map_err(|why| format!("the compositor refused the description: {why}"))
    }

    /// Lets go of the colour objects, before the sink's surface goes.
    pub(super) fn drop_color(&mut self) {
        if let Some(surface) = self.color_surface.take()
            && surface.is_alive()
        {
            surface.destroy();
        }
        if let Some(manager) = self.color_manager.take() {
            manager.destroy();
        }
    }
}
