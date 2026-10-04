//! The event tables handed to libpipewire, and the callbacks behind them.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void};

use crate::beat::BeatEvent;
use crate::{Beat, Level, Tempo, Update};

use super::ffi::*;
use super::pod::{self, Pod};
use super::{Listener, Message, Monitor, Outcome, Session};

pub(super) static CORE_EVENTS: CoreEvents = CoreEvents {
    version: 0,
    info: None,
    done: Some(on_core_done),
    ping: None,
    error: Some(on_core_error),
    remove_id: None,
    bound_id: None,
    add_mem: None,
    remove_mem: None,
};

pub(super) static REGISTRY_EVENTS: RegistryEvents = RegistryEvents {
    version: 0,
    global: Some(on_global),
    global_remove: Some(on_global_remove),
};

pub(super) static NODE_EVENTS: NodeEvents = NodeEvents {
    version: 0,
    info: Some(on_node_info),
    param: Some(on_node_param),
};

pub(super) static DEVICE_EVENTS: DeviceEvents = DeviceEvents {
    version: 0,
    info: None,
    param: Some(on_device_param),
};

pub(super) static METADATA_EVENTS: MetadataEvents = MetadataEvents {
    version: 0,
    property: Some(on_metadata_property),
};

pub(super) static STREAM_EVENTS: StreamEvents = StreamEvents {
    version: 0,
    destroy: None,
    state_changed: Some(on_stream_state),
    control_info: None,
    io_changed: None,
    param_changed: Some(on_stream_param),
    add_buffer: None,
    remove_buffer: None,
    process: Some(on_stream_process),
    drained: None,
};

// The callbacks. Each finds the session through its data pointer and hands
// over to a method; the library calls them only from `pw_main_loop_run`,
// on this backend's thread, while nothing else holds the session.

unsafe extern "C" fn on_core_done(data: *mut c_void, id: u32, seq: c_int) {
    // SAFETY: registered with the session as data.
    let session = unsafe { &mut *data.cast::<Session>() };
    if id == PW_ID_CORE {
        session.done(seq);
    }
}

unsafe extern "C" fn on_core_error(
    data: *mut c_void,
    id: u32,
    _seq: c_int,
    res: c_int,
    message: *const c_char,
) {
    // SAFETY: registered with the session as data; the message is a C string.
    let (session, message) = unsafe { (&mut *data.cast::<Session>(), string(message)) };
    if id != PW_ID_CORE {
        return;
    }
    if res == -libc::EPIPE {
        session.outcome = Outcome::Lost;
        // SAFETY: the loop is running on this thread.
        unsafe { (session.pw.main_loop_quit)(session.main_loop) };
    } else {
        session.events.send(Update::Error(format!(
            "PipeWire: {}",
            message.unwrap_or_else(|| format!("error {res}"))
        )));
    }
}

unsafe extern "C" fn on_global(
    data: *mut c_void,
    id: u32,
    _permissions: u32,
    kind: *const c_char,
    _version: u32,
    props: *const SpaDict,
) {
    // SAFETY: registered with the session as data; the strings and dict are
    // the library's, valid for this call.
    unsafe {
        let session = data.cast::<Session>();
        let (Some(kind), props) = (string(kind), dict_entries(props)) else {
            return;
        };
        let props: HashMap<String, String> = props.into_iter().collect();
        Session::global(session, id, &kind, props);
    }
}

unsafe extern "C" fn on_global_remove(data: *mut c_void, id: u32) {
    // SAFETY: registered with the session as data.
    unsafe { (*data.cast::<Session>()).global_remove(id) };
}

unsafe extern "C" fn on_node_info(data: *mut c_void, info: *const NodeInfo) {
    // SAFETY: registered with a listener as data; the info is the library's.
    unsafe {
        let listener = data.cast::<Listener>();
        let (session, id) = ((*listener).session, (*listener).id);
        if info.is_null() {
            return;
        }
        let props = dict_entries((*info).props);
        (*session).node_info(id, props);
    }
}

unsafe extern "C" fn on_node_param(
    data: *mut c_void,
    _seq: c_int,
    param: u32,
    _index: u32,
    _next: u32,
    value: *const SpaPod,
) {
    // SAFETY: as for `on_node_info`; the POD is the library's.
    unsafe {
        let listener = data.cast::<Listener>();
        let (session, id) = ((*listener).session, (*listener).id);
        if param == PARAM_PROPS
            && let Some(value) = pod::read(value)
        {
            (*session).node_props(id, &value);
        } else if param == PARAM_LATENCY
            && let Some(value) = pod::read(value)
        {
            (*session).node_latency(id, &value);
        }
    }
}

unsafe extern "C" fn on_device_param(
    data: *mut c_void,
    _seq: c_int,
    param: u32,
    _index: u32,
    _next: u32,
    value: *const SpaPod,
) {
    // SAFETY: as for `on_node_param`.
    unsafe {
        let listener = data.cast::<Listener>();
        let (session, id) = ((*listener).session, (*listener).id);
        if param == PARAM_ROUTE
            && let Some(value) = pod::read(value)
        {
            (*session).device_route(id, &value);
        }
    }
}

unsafe extern "C" fn on_metadata_property(
    data: *mut c_void,
    subject: u32,
    key: *const c_char,
    _kind: *const c_char,
    value: *const c_char,
) -> c_int {
    // SAFETY: as for `on_node_info`; the strings are the library's.
    unsafe {
        let listener = data.cast::<Listener>();
        let session = (*listener).session;
        (*session).metadata_property(subject, string(key), string(value));
    }
    0
}

pub(super) unsafe extern "C" fn on_command(data: *mut c_void, _count: u64) {
    // SAFETY: registered with the session as data.
    let session = unsafe { &mut *data.cast::<Session>() };
    while let Ok(message) = session.receiver.try_recv() {
        match message {
            Message::Quit => {
                session.outcome = Outcome::Quit;
                // SAFETY: the loop is running on this thread.
                unsafe { (session.pw.main_loop_quit)(session.main_loop) };
                return;
            }
            Message::Command(command) => session.command(command),
        }
    }
}

unsafe extern "C" fn on_stream_state(
    data: *mut c_void,
    old: c_int,
    state: c_int,
    error: *const c_char,
) {
    // SAFETY: registered with the monitor as data.
    let monitor = unsafe { &mut *data.cast::<Monitor>() };
    // A device with nothing playing goes idle and the stream is paused: no
    // more samples, so no more readings. Say silence once, or a meter would
    // hold whatever it last showed.
    if old == STREAM_STATE_STREAMING && state != STREAM_STATE_STREAMING {
        let reading = monitor.meter.silence();
        monitor.events.send(Update::Level(Level {
            monitor: monitor.id,
            left: reading.left,
            right: reading.right,
            bands: reading.bands,
        }));
    }
    if state == STREAM_STATE_ERROR {
        // SAFETY: a C string or null.
        let error = unsafe { string(error) }.unwrap_or_default();
        monitor
            .events
            .send(Update::Error(format!("level monitor: {error}")));
    }
}

unsafe extern "C" fn on_stream_param(data: *mut c_void, id: u32, value: *const SpaPod) {
    // SAFETY: registered with the monitor as data; the POD is the library's.
    let (monitor, value) = unsafe { (&mut *data.cast::<Monitor>(), pod::read(value)) };
    if id != PARAM_FORMAT {
        return;
    }
    let Some(format) = value else {
        return;
    };
    let rate = format
        .property(pod::FORMAT_AUDIO_RATE)
        .and_then(Pod::as_int)
        .unwrap_or(48_000);
    let channels = format
        .property(pod::FORMAT_AUDIO_CHANNELS)
        .and_then(Pod::as_int)
        .unwrap_or(2);
    monitor
        .meter
        .set_format(rate.max(1) as u32, channels.max(1) as u32);
}

/// Reports what the meter's beat tracker heard, if it has one.
fn send_beats(monitor: &mut Monitor) {
    let id = monitor.id;
    let events: Vec<BeatEvent> = monitor.meter.beats().collect();
    for event in events {
        monitor.emit(match event {
            BeatEvent::Beat { strength } => Update::Beat(Beat {
                monitor: id,
                strength,
            }),
            BeatEvent::Tempo { bpm, confidence } => Update::Tempo(Tempo {
                monitor: id,
                bpm,
                confidence,
            }),
        });
    }
}

unsafe extern "C" fn on_stream_process(data: *mut c_void) {
    // SAFETY: registered with the monitor as data; the buffer and its memory
    // are the stream's until queued back, and mapped (MAP_BUFFERS).
    unsafe {
        let monitor = &mut *data.cast::<Monitor>();
        let buffer = (monitor.pw.stream_dequeue_buffer)(monitor.stream);
        if buffer.is_null() {
            return;
        }
        let spa = (*buffer).buffer;
        if !spa.is_null() && (*spa).n_datas > 0 && !(*spa).datas.is_null() {
            let first = &*(*spa).datas;
            if !first.data.is_null() && !first.chunk.is_null() && first.maxsize > 0 {
                let chunk = &*first.chunk;
                let offset = chunk.offset % first.maxsize;
                let size = chunk.size.min(first.maxsize - offset);
                let start = first.data.cast::<u8>().add(offset as usize);
                if start.align_offset(std::mem::align_of::<f32>()) == 0 {
                    let samples =
                        std::slice::from_raw_parts(start.cast::<f32>(), size as usize / 4);
                    if let Some(reading) = monitor.meter.push(samples) {
                        let id = monitor.id;
                        monitor.emit(Update::Level(Level {
                            monitor: id,
                            left: reading.left,
                            right: reading.right,
                            bands: reading.bands,
                        }));
                    }
                    send_beats(monitor);
                    monitor.release();
                }
            }
        }
        (monitor.pw.stream_queue_buffer)(monitor.stream, buffer);
    }
}
