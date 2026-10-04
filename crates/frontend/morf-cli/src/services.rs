use morf_io::IpcValue as WireValue;
use morf_lua::{InputMethodRequest, Runtime, TextInputRequest, VirtualKeyboardRequest};
use morf_value::IpcValue;
use morf_app::{InputRect, LayerClient};
use morf_desktop::{Desktop, OutputPowerMode};
use std::collections::BTreeMap;

use crate::lock::*;

pub(crate) fn lua_ipc_value(value: &WireValue) -> IpcValue {
    match value {
        WireValue::Nil => IpcValue::Nil,
        WireValue::Boolean(value) => IpcValue::Boolean(*value),
        WireValue::Integer(value) => IpcValue::Integer(*value),
        WireValue::Number(value) => IpcValue::Number(*value),
        WireValue::String(value) => IpcValue::String(value.clone()),
    }
}

pub(crate) fn wire_ipc_value(value: &IpcValue) -> WireValue {
    match value {
        IpcValue::Nil => WireValue::Nil,
        IpcValue::Boolean(value) => WireValue::Boolean(*value),
        IpcValue::Integer(value) => WireValue::Integer(*value),
        IpcValue::Number(value) => WireValue::Number(*value),
        IpcValue::String(value) => WireValue::String(value.clone()),
        // A colour crosses the wire as its hex, which is what a caller can
        // print and what a shell can parse back.
        IpcValue::Color(color) => WireValue::String(color.to_pastel().to_rgb_hex_string(true)),
        // A table a signal holds crosses the scalar wire as its JSON text.
        IpcValue::Table(_) => WireValue::String(value.to_json().to_string()),
    }
}

pub(crate) fn apply_idle_inhibit(runtime: &mut Runtime, client: &mut LayerClient) {
    if let Some(inhibited) = runtime.take_idle_inhibit_change() {
        client.set_idle_inhibited(inhibited);
    }
}

/// A threshold subscribed, or cancelled, since the last frame reaches the
/// compositor now, not at the next reload.
pub(crate) fn apply_idle_timeouts(runtime: &mut Runtime, client: &mut LayerClient) {
    if let Some(timeouts) = runtime.take_idle_timeouts_change() {
        client.update_idle_timeouts(&timeouts);
    }
}

pub(crate) fn apply_shortcuts_inhibit(runtime: &mut Runtime, client: &mut LayerClient) {
    if let Some(inhibited) = runtime.take_shortcuts_inhibit_change() {
        client.set_shortcuts_inhibited(inhibited);
    }
}

pub(crate) fn apply_output_power_requests(runtime: &mut Runtime, desktop: &mut Desktop) {
    for on in runtime.take_output_power_requests() {
        desktop.set_output_power(if on {
            OutputPowerMode::On
        } else {
            OutputPowerMode::Off
        });
    }
}

pub(crate) fn apply_gamma_requests(runtime: &mut Runtime, desktop: &mut Desktop) {
    for request in runtime.take_gamma_requests() {
        let output = request.output.as_deref();
        let result = match request.set {
            Some((temperature, brightness, gamma)) => desktop.set_gamma(
                output,
                morf_desktop::GammaSettings {
                    temperature,
                    brightness,
                    gamma,
                },
            ),
            None => desktop.reset_gamma(output),
        };
        if let Err(error) = result {
            runtime.warn(format!("morf.gamma: {error}"));
        }
    }
    for output in desktop.take_gamma_failures() {
        runtime.warn(format!(
            "morf.gamma: the compositor refused gamma control of `{output}` (another client may hold it)"
        ));
    }
}

pub(crate) fn apply_clipboard_requests(runtime: &mut Runtime, client: &mut LayerClient) {
    // Data control first: no focus, no serial, any type. Without it only text
    // can be set, and only once an input serial exists to set it with, so the
    // requests wait for one.
    if client.supports_data_control() {
        for request in runtime.take_clipboard_requests() {
            let primary = request.primary;
            client.set_selection(crate::surface_drag::clipboard_payload(request), primary);
        }
        return;
    }
    if !client.can_set_clipboard() {
        return;
    }
    for request in runtime.take_clipboard_requests() {
        if request.mime.is_none()
            && !request.primary
            && let Ok(text) = String::from_utf8(request.data)
        {
            client.set_clipboard(text);
        }
    }
}

pub(crate) fn apply_virtual_keyboard_requests(runtime: &mut Runtime, client: &mut LayerClient) {
    for request in runtime.take_virtual_keyboard_requests() {
        match request {
            VirtualKeyboardRequest::Key { keycode, pressed } => {
                client.send_virtual_key(keycode, pressed);
            }
            VirtualKeyboardRequest::Modifiers {
                depressed,
                latched,
                locked,
                group,
            } => {
                client.send_virtual_modifiers(depressed, latched, locked, group);
            }
        }
    }
}

pub(crate) fn apply_input_method_requests(runtime: &mut Runtime, client: &mut LayerClient) {
    if runtime.take_input_method_enable_request() {
        client.enable_input_method();
    }
    for request in runtime.take_input_method_requests() {
        match request {
            InputMethodRequest::Commit(text) => {
                client.input_method_commit(&text);
            }
            InputMethodRequest::Preedit { text, begin, end } => {
                client.input_method_preedit(&text, begin, end);
            }
            InputMethodRequest::Delete { before, after } => {
                client.input_method_delete(before, after);
            }
        }
    }
}

pub(crate) fn apply_text_input_requests(runtime: &mut Runtime, client: &mut LayerClient) {
    if runtime.take_text_input_enable_request() {
        client.enable_text_input();
    }
    for request in runtime.take_text_input_requests() {
        match request {
            TextInputRequest::Disable => {
                client.disable_text_input();
            }
            TextInputRequest::Surrounding {
                text,
                cursor,
                anchor,
            } => {
                client.set_text_input_surrounding(&text, cursor, anchor);
            }
            TextInputRequest::ContentType { hints, purpose } => {
                client.set_text_input_content_type(hints, purpose);
            }
            TextInputRequest::CursorRect {
                x,
                y,
                width,
                height,
            } => {
                client.set_text_input_cursor_rect(InputRect {
                    x,
                    y,
                    width,
                    height,
                });
            }
        }
    }
}

pub(crate) fn stop_workers(workers: BTreeMap<String, Worker>) {
    for worker in workers.values() {
        worker.request_stop();
    }
    for (_, worker) in workers {
        let _ = worker.join.join();
    }
}
