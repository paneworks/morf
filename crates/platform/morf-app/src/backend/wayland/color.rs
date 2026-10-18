//! Colour management (`wp_color_manager_v1`), the connection's half: what
//! the compositor can be told, and what each output prefers.
//!
//! A surface's own description is set by its buffer sink, on the sink's
//! queue (`sink_color`); this side only reads, for `Capabilities` and the
//! outputs' [`OutputColor`].

use std::collections::HashMap;

use morf_value::present::ColorSupport;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_management_output_v1::{self, WpColorManagementOutputV1},
    wp_color_manager_v1::{self, Primaries, TransferFunction, WpColorManagerV1},
    wp_image_description_info_v1::{self, WpImageDescriptionInfoV1},
    wp_image_description_v1::{self, WpImageDescriptionV1},
};

use crate::OutputColor;
use crate::backend::wayland::state_types::LayerState;

/// The newest `wp_color_manager_v1` this speaks.
pub(crate) const MANAGER_VERSION: u32 = 3;

/// What the compositor said about colour, connection-wide and per output
/// (by the output's global name).
#[derive(Default)]
pub(crate) struct ColorState {
    pub(crate) manager: Option<WpColorManagerV1>,
    pub(crate) support: ColorSupport,
    pub(crate) outputs: HashMap<u32, OutputColor>,
    pending: HashMap<u32, OutputColor>,
    watched: HashMap<u32, WpColorManagementOutputV1>,
}

impl ColorState {
    pub(crate) fn new(manager: Option<WpColorManagerV1>) -> Self {
        Self {
            manager,
            ..Self::default()
        }
    }
}

/// Adds one `wp_color_manager_v1` capability event to `support`.
pub(crate) fn note_support(support: &mut ColorSupport, event: wp_color_manager_v1::Event) {
    use wp_color_manager_v1::{Event, Feature};
    match event {
        Event::SupportedFeature {
            feature: WEnum::Value(feature),
        } => match feature {
            Feature::Parametric => support.parametric = true,
            Feature::SetLuminances => support.luminances = true,
            _ => {}
        },
        Event::SupportedTfNamed {
            tf: WEnum::Value(tf),
        } => match tf {
            TransferFunction::St2084Pq => support.pq = true,
            TransferFunction::ExtLinear => support.ext_linear = true,
            _ => {}
        },
        Event::SupportedPrimariesNamed {
            primaries: WEnum::Value(primaries),
        } => match primaries {
            Primaries::Bt2020 => support.bt2020 = true,
            Primaries::Srgb => support.srgb_primaries = true,
            _ => {}
        },
        _ => {}
    }
}

fn transfer_name(tf: TransferFunction) -> &'static str {
    match tf {
        TransferFunction::St2084Pq => "pq",
        TransferFunction::ExtLinear => "ext_linear",
        TransferFunction::Gamma22 => "gamma22",
        TransferFunction::Gamma28 => "gamma28",
        TransferFunction::Srgb | TransferFunction::CompoundPower24 => "srgb",
        TransferFunction::ExtSrgb => "ext_srgb",
        TransferFunction::Hlg => "hlg",
        TransferFunction::Bt1886 => "bt1886",
        _ => "other",
    }
}

fn primaries_name(primaries: Primaries) -> &'static str {
    match primaries {
        Primaries::Srgb => "srgb",
        Primaries::Bt2020 => "bt2020",
        Primaries::DisplayP3 => "display_p3",
        Primaries::DciP3 => "dci_p3",
        Primaries::AdobeRgb => "adobe_rgb",
        _ => "other",
    }
}

impl LayerState {
    /// Starts reading what `output` prefers, and again whenever that changes.
    pub(crate) fn watch_output_color(&mut self, output: &WlOutput, qh: &QueueHandle<Self>) {
        let (Some(manager), Some(id)) = (
            self.color.manager.as_ref(),
            self.outputs.info(output).map(|info| info.id),
        ) else {
            return;
        };
        if self.color.watched.contains_key(&id) {
            return;
        }
        let watched = manager.get_output(output, qh, id);
        watched.get_image_description(qh, id);
        self.color.watched.insert(id, watched);
    }

    /// Forgets an output that is going away.
    pub(crate) fn forget_output_color(&mut self, output: &WlOutput) {
        let Some(id) = self.outputs.info(output).map(|info| info.id) else {
            return;
        };
        if let Some(watched) = self.color.watched.remove(&id) {
            watched.destroy();
        }
        self.color.outputs.remove(&id);
        self.color.pending.remove(&id);
    }
}

impl Dispatch<WpColorManagerV1, ()> for LayerState {
    fn event(
        state: &mut Self,
        _: &WpColorManagerV1,
        event: wp_color_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        note_support(&mut state.color.support, event);
    }
}

impl Dispatch<WpColorManagementOutputV1, u32> for LayerState {
    fn event(
        _: &mut Self,
        output: &WpColorManagementOutputV1,
        event: wp_color_management_output_v1::Event,
        id: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wp_color_management_output_v1::Event::ImageDescriptionChanged = event {
            output.get_image_description(qh, *id);
        }
    }
}

impl Dispatch<WpImageDescriptionV1, u32> for LayerState {
    fn event(
        state: &mut Self,
        description: &WpImageDescriptionV1,
        event: wp_image_description_v1::Event,
        id: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wp_image_description_v1::Event;
        match event {
            Event::Ready { .. } | Event::Ready2 { .. } => {
                state.color.pending.insert(*id, OutputColor::default());
                description.get_information(qh, *id);
                description.destroy();
            }
            Event::Failed { .. } => description.destroy(),
            _ => {}
        }
    }
}

impl Dispatch<WpImageDescriptionInfoV1, u32> for LayerState {
    fn event(
        state: &mut Self,
        _: &WpImageDescriptionInfoV1,
        event: wp_image_description_info_v1::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wp_image_description_info_v1::Event;
        if let Event::Done = event {
            if let Some(color) = state.color.pending.remove(id) {
                if std::env::var_os("MORF_HDR").is_some() {
                    eprintln!(
                        "morf: color: output {id} prefers {} over {}, {}-{} cd/m², SDR white {} cd/m²",
                        color.transfer,
                        color.primaries,
                        f64::from(color.min_luminance) / 10_000.0,
                        color.max_luminance,
                        color.reference_luminance
                    );
                }
                state.color.outputs.insert(*id, color);
            }
            return;
        }
        let Some(color) = state.color.pending.get_mut(id) else {
            return;
        };
        match event {
            Event::TfNamed {
                tf: WEnum::Value(tf),
            } => color.transfer = transfer_name(tf),
            Event::TfPower { .. } => color.transfer = "power",
            Event::PrimariesNamed {
                primaries: WEnum::Value(primaries),
            } => color.primaries = primaries_name(primaries),
            Event::Primaries { .. } if color.primaries.is_empty() => color.primaries = "custom",
            Event::Luminances {
                min_lum,
                max_lum,
                reference_lum,
            } => {
                color.min_luminance = min_lum;
                color.max_luminance = max_lum;
                color.reference_luminance = reference_lum;
            }
            // The display's own peak, sent after the encoding's, wins.
            Event::TargetLuminance { max_lum, .. } => color.max_luminance = max_lum,
            _ => {}
        }
    }
}
