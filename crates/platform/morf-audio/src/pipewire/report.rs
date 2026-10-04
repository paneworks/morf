//! What the session learns about nodes, devices and metadata, and the
//! updates it reports from them.

use std::time::Duration;

use crate::{Device, DeviceKind, Direction, MonitorDelay, Stream, Update};

use super::ffi::*;
use super::pod::{self, Pod};
use super::{ANALYSIS_LAG, Class, OWN_STREAM, Route, Session, json_name};

impl Session {
    pub(super) fn node_info(&mut self, id: u32, props: Vec<(String, String)>) {
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        if !props.is_empty() {
            node.props = props.into_iter().collect();
            node.own = node.props.contains_key(OWN_STREAM);
            if let Some(serial) = node.props.get("object.serial") {
                node.serial = Some(serial.clone());
            }
        }
        node.have_info = true;
        self.report(id);
    }

    /// A node's Latency param: the input side of a sink says how long what
    /// it is given takes to be heard.
    pub(super) fn node_latency(&mut self, id: u32, value: &Pod) {
        // SPA_PARAM_LATENCY_direction and _maxNs.
        const DIRECTION: u32 = 1;
        const MAX_NS: u32 = 7;
        if value.property(DIRECTION).and_then(Pod::as_id) != Some(DIRECTION_INPUT) {
            return;
        }
        let ns = match value.property(MAX_NS) {
            Some(Pod::Long(ns)) => (*ns).max(0) as u64,
            Some(Pod::Int(ns)) => (*ns).max(0) as u64,
            _ => return,
        };
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        let latency = Duration::from_nanos(ns);
        if node.latency != latency {
            node.latency = latency;
            self.refresh_monitor_delays();
        }
    }

    /// Works out again how long each monitor holds what it measured: a
    /// device's delay moves when it changes codec, and the default output
    /// moves when a headset connects.
    pub(super) fn refresh_monitor_delays(&mut self) {
        let default = self.default_sink.clone();
        let default_node = default.clone().and_then(|name| {
            self.nodes
                .iter()
                .find(|(_, node)| node.props.get("node.name") == Some(&name))
                .map(|(id, _)| *id)
        });
        for monitor in self.monitors.values_mut() {
            monitor.hold = match monitor.delay {
                MonitorDelay::None => Duration::ZERO,
                MonitorDelay::Fixed(ms) => Duration::from_secs_f32(ms.clamp(0.0, 5000.0) / 1000.0),
                MonitorDelay::Device => monitor
                    .device
                    .or(default_node)
                    .and_then(|id| self.nodes.get(&id))
                    .map_or(Duration::ZERO, |node| {
                        node.latency.saturating_sub(ANALYSIS_LAG)
                    }),
            };
            if std::env::var_os("MORF_AUDIO_LOG").is_some() {
                eprintln!(
                    "morf: audio: monitor {} holds {} ms (default {:?} = node {:?})",
                    monitor.id,
                    monitor.hold.as_millis(),
                    self.default_sink,
                    default_node
                );
            }
        }
    }

    pub(super) fn node_props(&mut self, id: u32, value: &Pod) {
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        let mut touched = false;
        if let Some(volumes) = value
            .property(pod::PROP_CHANNEL_VOLUMES)
            .and_then(Pod::as_floats)
            && !volumes.is_empty()
        {
            node.volumes = volumes;
            touched = true;
        }
        if let Some(muted) = value.property(pod::PROP_MUTE).and_then(Pod::as_bool) {
            node.muted = muted;
            touched = true;
        }
        if touched {
            node.settled = true;
            self.report(id);
        }
    }

    pub(super) fn device_route(&mut self, id: u32, value: &Pod) {
        let Some(device) = self.devices.get_mut(&id) else {
            return;
        };
        let (Some(index), Some(direction), Some(card_device)) = (
            value.property(pod::ROUTE_INDEX).and_then(Pod::as_int),
            value.property(pod::ROUTE_DIRECTION).and_then(Pod::as_id),
            value.property(pod::ROUTE_DEVICE).and_then(Pod::as_int),
        ) else {
            return;
        };
        device
            .routes
            .retain(|route| !(route.direction == direction && route.device == card_device));
        device.routes.push(Route {
            index,
            direction,
            device: card_device,
        });
    }

    pub(super) fn metadata_property(
        &mut self,
        subject: u32,
        key: Option<String>,
        value: Option<String>,
    ) {
        if subject != PW_ID_CORE {
            return;
        }
        let name = value.as_deref().and_then(json_name);
        match key.as_deref() {
            None => {
                self.default_sink = None;
                self.default_source = None;
            }
            Some("default.audio.sink") => self.default_sink = name,
            Some("default.audio.source") => self.default_source = name,
            _ => return,
        }
        self.report_defaults();
        self.refresh_monitor_delays();
    }

    pub(super) fn report_defaults(&mut self) {
        if !self.ready {
            return;
        }
        if self.reported_defaults.0 != self.default_sink {
            self.reported_defaults.0 = self.default_sink.clone();
            self.events
                .send(Update::DefaultSink(self.default_sink.clone()));
        }
        if self.reported_defaults.1 != self.default_source {
            self.reported_defaults.1 = self.default_source.clone();
            self.events
                .send(Update::DefaultSource(self.default_source.clone()));
        }
    }

    /// Reports a node as it now is, if that differs from the last report.
    pub(super) fn report(&mut self, id: u32) {
        if !self.ready {
            return;
        }
        let Some(update) = self.describe(id) else {
            return;
        };
        let node = self.nodes.get_mut(&id).expect("described nodes exist");
        if node.reported.as_ref() != Some(&update) {
            node.reported = Some(update.clone());
            self.events.send(update);
        }
    }

    fn describe(&self, id: u32) -> Option<Update> {
        let node = self.nodes.get(&id)?;
        if node.own || !node.have_info || !node.settled {
            return None;
        }
        let prop = |key: &str| {
            node.props
                .get(key)
                .filter(|value| !value.is_empty())
                .cloned()
        };
        if node.class.is_device() {
            let name = prop("node.name").unwrap_or_else(|| format!("node-{id}"));
            let icon_name = prop("device.icon-name").or_else(|| {
                let device = prop("device.id")?.parse::<u32>().ok()?;
                self.devices.get(&device)?.icon_name.clone()
            });
            return Some(Update::Device(Device {
                id,
                description: prop("node.description")
                    .or_else(|| prop("node.nick"))
                    .unwrap_or_else(|| name.clone()),
                name,
                kind: if node.class == Class::Sink {
                    DeviceKind::Sink
                } else {
                    DeviceKind::Source
                },
                channel_volumes: node.volumes.clone(),
                muted: node.muted,
                icon_name,
            }));
        }
        let direction = if node.class == Class::Playback {
            Direction::Playback
        } else {
            Direction::Record
        };
        let device = self.links.values().find_map(|&(output, input)| {
            let (this, peer) = match direction {
                Direction::Playback => (output, input),
                Direction::Record => (input, output),
            };
            (this == id
                && self
                    .nodes
                    .get(&peer)
                    .is_some_and(|peer| peer.class.is_device()))
            .then_some(peer)
        });
        Some(Update::Stream(Stream {
            id,
            app_name: prop("application.name")
                .or_else(|| prop("node.description"))
                .or_else(|| prop("node.name"))
                .unwrap_or_default(),
            app_id: prop("application.id").or_else(|| prop("pipewire.access.portal.app_id")),
            binary: prop("application.process.binary"),
            icon_name: prop("application.icon-name").or_else(|| prop("media.icon-name")),
            media_name: prop("media.name"),
            direction,
            device,
            channel_volumes: node.volumes.clone(),
            muted: node.muted,
            pid: prop("application.process.id").and_then(|pid| pid.parse().ok()),
        }))
    }
}
