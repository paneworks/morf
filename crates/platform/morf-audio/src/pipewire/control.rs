//! Carrying out commands: volumes, mutes, defaults, stream targets, and
//! the level monitors.

use std::collections::VecDeque;
use std::ffi::{CString, c_void};
use std::ptr;
use std::sync::Arc;
use std::time::Duration;

use crate::dsp::Meter;
use crate::{Command, MonitorDelay, ObjectId, Update};

use super::ffi::*;
use super::pod::{self, Pod};
use super::{Class, Monitor, OWN_STREAM, STREAM_EVENTS, Session, json_quote};

impl Session {
    pub(super) fn command(&mut self, command: Command) {
        match command {
            Command::SetVolumes { id, volumes } => {
                self.set_props(id, vec![(pod::PROP_CHANNEL_VOLUMES, Pod::floats(&volumes))]);
            }
            Command::SetMute { id, muted } => {
                self.set_props(id, vec![(pod::PROP_MUTE, Pod::Bool(muted))]);
            }
            Command::SetDefault { id } => {
                let Some(node) = self.nodes.get(&id) else {
                    return;
                };
                let key = match node.class {
                    Class::Sink => "default.configured.audio.sink",
                    Class::Source => "default.configured.audio.source",
                    _ => return,
                };
                let Some(name) = node.props.get("node.name") else {
                    return;
                };
                let value = format!("{{ \"name\": {} }}", json_quote(name));
                self.set_metadata(PW_ID_CORE, key, Some(("Spa:String:JSON", &value)));
            }
            Command::MoveStream { stream, device } => {
                let Some(serial) = self.nodes.get(&device).and_then(|node| node.serial.clone())
                else {
                    return;
                };
                if !self.nodes.contains_key(&stream) {
                    return;
                }
                self.set_metadata(stream, "target.node", None);
                self.set_metadata(stream, "target.object", Some(("Spa:Id", &serial)));
            }
            Command::StartMonitor {
                monitor,
                device,
                rate_hz,
                bands,
                beat,
                delay,
            } => {
                self.start_monitor(monitor, device, rate_hz, bands, beat);
                if let Some(started) = self.monitors.get_mut(&monitor) {
                    started.device = device;
                    started.delay = delay;
                }
                self.refresh_monitor_delays();
            }
            Command::StopMonitor { monitor } => {
                if let Some(monitor) = self.monitors.remove(&monitor) {
                    // SAFETY: the stream is ours; destroying it unhooks the
                    // listener before the box goes.
                    unsafe { (self.pw.stream_destroy)(monitor.stream) };
                }
            }
        }
    }

    /// Sets properties of a device or stream: on the device's active route
    /// when it has one, on the node otherwise.
    fn set_props(&mut self, id: ObjectId, properties: Vec<(u32, Pod)>) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        let props = Pod::object(pod::OBJECT_PROPS, PARAM_PROPS, properties);
        if node.class.is_device()
            && let Some(device_id) = node
                .props
                .get("device.id")
                .and_then(|id| id.parse::<u32>().ok())
            && let Some(card_device) = node
                .props
                .get("card.profile.device")
                .and_then(|device| device.parse::<i32>().ok())
            && let Some(device) = self.devices.get(&device_id)
        {
            let direction = if node.class == Class::Sink {
                DIRECTION_OUTPUT
            } else {
                DIRECTION_INPUT
            };
            if let Some(route) = device
                .routes
                .iter()
                .find(|route| route.device == card_device && route.direction == direction)
            {
                let route = Pod::object(
                    pod::OBJECT_ROUTE,
                    PARAM_ROUTE,
                    vec![
                        (pod::ROUTE_INDEX, Pod::Int(route.index)),
                        (pod::ROUTE_DEVICE, Pod::Int(card_device)),
                        (pod::ROUTE_PROPS, props),
                        (pod::ROUTE_SAVE, Pod::Bool(true)),
                    ],
                )
                .encode();
                // SAFETY: the device proxy is live and a device.
                unsafe { set_param::<DeviceEvents>(device.proxy, PARAM_ROUTE, &route) };
                return;
            }
        }
        let props = props.encode();
        // SAFETY: the node proxy is live and a node.
        unsafe { set_param::<NodeEvents>(node.proxy, PARAM_PROPS, &props) };
    }

    fn set_metadata(&mut self, subject: u32, key: &str, value: Option<(&str, &str)>) {
        let Some(metadata) = &self.metadata else {
            return;
        };
        let Ok(key) = CString::new(key) else {
            return;
        };
        let value = value
            .and_then(|(kind, value)| Some((CString::new(kind).ok()?, CString::new(value).ok()?)));
        // SAFETY: the metadata proxy is live; the strings outlive the call.
        unsafe {
            if let Some((methods, object)) = methods::<MetadataMethods>(metadata.proxy)
                && let Some(set_property) = methods.set_property
            {
                let (kind, value) = value
                    .as_ref()
                    .map_or((ptr::null(), ptr::null()), |(kind, value)| {
                        (kind.as_ptr(), value.as_ptr())
                    });
                set_property(object, subject, key.as_ptr(), kind, value);
            }
        }
    }

    fn start_monitor(
        &mut self,
        id: u64,
        device: Option<ObjectId>,
        rate_hz: f32,
        bands: usize,
        beat: bool,
    ) {
        let mut props = vec![
            ("media.type", "Audio".to_owned()),
            ("media.category", "Capture".to_owned()),
            ("media.role", "DSP".to_owned()),
            ("node.name", "morf-level".to_owned()),
            ("node.description", "morf level meter".to_owned()),
            ("application.name", "morf".to_owned()),
            ("node.passive", "true".to_owned()),
            ("node.dont-reconnect", "false".to_owned()),
            (OWN_STREAM, "true".to_owned()),
        ];
        match device {
            None => props.push(("stream.capture.sink", "true".to_owned())),
            Some(device) => {
                let Some(node) = self
                    .nodes
                    .get(&device)
                    .filter(|node| node.class.is_device())
                else {
                    self.events
                        .send(Update::Error(format!("level monitor: no device {device}")));
                    return;
                };
                if node.class == Class::Sink {
                    props.push(("stream.capture.sink", "true".to_owned()));
                }
                let target = node
                    .serial
                    .clone()
                    .or_else(|| node.props.get("node.name").cloned())
                    .unwrap_or_default();
                props.push(("target.object", target));
            }
        }
        let owned: Vec<(CString, CString)> = props
            .into_iter()
            .filter_map(|(key, value)| Some((CString::new(key).ok()?, CString::new(value).ok()?)))
            .collect();
        let items: Vec<SpaDictItem> = owned
            .iter()
            .map(|(key, value)| SpaDictItem {
                key: key.as_ptr(),
                value: value.as_ptr(),
            })
            .collect();
        let dict = SpaDict {
            flags: 0,
            n_items: items.len() as u32,
            items: items.as_ptr(),
        };
        let format = Pod::object(
            pod::OBJECT_FORMAT,
            PARAM_ENUM_FORMAT,
            vec![
                (pod::FORMAT_MEDIA_TYPE, Pod::Id(pod::MEDIA_TYPE_AUDIO)),
                (pod::FORMAT_MEDIA_SUBTYPE, Pod::Id(pod::MEDIA_SUBTYPE_RAW)),
                (pod::FORMAT_AUDIO_FORMAT, Pod::Id(native_f32())),
                (pod::FORMAT_AUDIO_CHANNELS, Pod::Int(2)),
                (
                    pod::FORMAT_AUDIO_POSITION,
                    Pod::ids(&[pod::AUDIO_CHANNEL_FL, pod::AUDIO_CHANNEL_FR]),
                ),
            ],
        )
        .encode();
        // SAFETY: the core is live; the dict and its strings outlive
        // `properties_new_dict`, which copies them; the monitor is boxed
        // before its address is handed over.
        unsafe {
            let properties = (self.pw.properties_new_dict)(&dict);
            let name = c"morf-level";
            let stream = (self.pw.stream_new)(self.core, name.as_ptr(), properties);
            if stream.is_null() {
                self.events.send(Update::Error(
                    "level monitor: cannot create a stream".into(),
                ));
                return;
            }
            let mut monitor = Box::new(Monitor {
                pw: Arc::clone(&self.pw),
                events: self.events.clone(),
                id,
                stream,
                hook: SpaHook::zeroed(),
                meter: Meter::new(rate_hz, bands).with_beats(beat),
                device: None,
                delay: MonitorDelay::None,
                hold: Duration::ZERO,
                held: VecDeque::new(),
            });
            let data = (&raw mut *monitor).cast::<c_void>();
            (self.pw.stream_add_listener)(stream, &raw mut monitor.hook, &STREAM_EVENTS, data);
            let mut params = [format.as_ptr()];
            let result = (self.pw.stream_connect)(
                stream,
                DIRECTION_INPUT,
                PW_ID_ANY,
                STREAM_FLAG_AUTOCONNECT | STREAM_FLAG_MAP_BUFFERS,
                params.as_mut_ptr(),
                1,
            );
            if result < 0 {
                (self.pw.stream_destroy)(stream);
                self.events.send(Update::Error(format!(
                    "level monitor: cannot connect ({result})"
                )));
                return;
            }
            self.monitors.insert(id, monitor);
        }
    }
}

/// Sets one parameter on a node or device proxy.
///
/// # Safety
/// `proxy` must be a live proxy whose methods are `ParamMethods<E>`.
unsafe fn set_param<E>(proxy: *mut c_void, id: u32, value: &pod::Encoded) {
    // SAFETY: promised by the caller.
    unsafe {
        if let Some((methods, object)) = methods::<ParamMethods<E>>(proxy)
            && let Some(set_param) = methods.set_param
        {
            set_param(object, id, 0, value.as_ptr());
        }
    }
}

fn native_f32() -> u32 {
    if cfg!(target_endian = "little") {
        pod::AUDIO_FORMAT_F32_LE
    } else {
        pod::AUDIO_FORMAT_F32_LE + 1
    }
}
