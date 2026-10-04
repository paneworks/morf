use super::*;

mod keys;
mod pointer;

pub(super) const BACKSPACE: u32 = 0xff08;
pub(super) const RETURN: u32 = 0xff0d;
pub(super) const ESCAPE: u32 = 0xff1b;
pub(super) const LEFT: u32 = 0xff51;
pub(super) const UP: u32 = 0xff52;
pub(super) const RIGHT: u32 = 0xff53;
pub(super) const DOWN: u32 = 0xff54;
pub(super) const HOME: u32 = 0xff50;
pub(super) const END: u32 = 0xff57;

pub(super) const CTRL: KeyModifiers = KeyModifiers {
    ctrl: true,
    shift: false,
    alt: false,
    logo: false,
};
pub(super) const SHIFT: KeyModifiers = KeyModifiers {
    ctrl: false,
    shift: true,
    alt: false,
    logo: false,
};
pub(super) const CTRL_SHIFT: KeyModifiers = KeyModifiers {
    ctrl: true,
    shift: true,
    alt: false,
    logo: false,
};
pub(super) const NONE: KeyModifiers = KeyModifiers {
    ctrl: false,
    shift: false,
    alt: false,
    logo: false,
};

/// A configuration whose root is the one text input `properties` describes,
/// with its callbacks recorded into `log`, which `morf.ipc["log"]` reads.
pub(super) fn field(properties: &str) -> (Runtime, NodeHandle) {
    let mut runtime = Runtime::default();
    let source = format!(
        r#"
            local ui = require("morf.ui")
            local log = {{}}
            local function record(name)
                return function(...)
                    local parts = {{ name }}
                    for _, value in ipairs({{ ... }}) do parts[#parts + 1] = tostring(value) end
                    log[#log + 1] = table.concat(parts, " ")
                end
            end
            morf.ipc["log"] = function()
                local joined = table.concat(log, "|")
                log = {{}}
                return joined
            end
            field = ui.TextInput {{
                width = 200,
                height = 30,
                font_size = 10,
                on_text_changed = record("changed"),
                on_accepted = record("accepted"),
                on_escape = record("escape"),
                on_focus_changed = record("focus"),
                {properties}
            }}
        "#
    );
    runtime.execute("field.lua", source.as_bytes()).unwrap();
    let node = runtime.scene().roots()[0];
    (runtime, node)
}

pub(super) fn log(runtime: &mut Runtime) -> String {
    match runtime.call_ipc("log", &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        other => panic!("log returned {other:?}"),
    }
}

pub(super) fn text(runtime: &Runtime, node: NodeHandle) -> String {
    runtime
        .scene()
        .string_value(node, "text")
        .unwrap()
        .to_owned()
}

pub(super) fn number(runtime: &Runtime, node: NodeHandle, property: &str) -> f64 {
    runtime.scene().number(node, property).unwrap()
}

pub(super) fn type_text(runtime: &mut Runtime, node: NodeHandle, typed: &str) {
    for character in typed.chars() {
        let keysym = character as u32;
        let text = character.to_string();
        runtime.dispatch_key(node, keysym, Some(&text), NONE);
    }
}

pub(super) fn press(runtime: &mut Runtime, node: NodeHandle, keysym: u32, modifiers: KeyModifiers) {
    runtime.dispatch_key(node, keysym, None, modifiers);
}

pub(super) fn letter(
    runtime: &mut Runtime,
    node: NodeHandle,
    letter: char,
    modifiers: KeyModifiers,
) {
    // Control and a letter: xkb reports the letter's keysym and a control
    // character as its text, which must not be typed.
    let control = char::from_u32(letter as u32 & 0x1f).unwrap().to_string();
    runtime.dispatch_key(node, letter as u32, Some(&control), modifiers);
}

pub(super) fn focused(properties: &str) -> (Runtime, NodeHandle) {
    let (mut runtime, node) = field(properties);
    assert!(runtime.set_key_focus(Some(node)));
    assert_eq!(log(&mut runtime), "focus true");
    (runtime, node)
}

/// The text the field put on the clipboard since this was last asked.
pub(super) fn copied(runtime: &mut Runtime) -> Vec<String> {
    runtime
        .take_clipboard_requests()
        .into_iter()
        .map(|request| {
            assert!(request.mime.is_none() && !request.primary);
            String::from_utf8(request.data).unwrap()
        })
        .collect()
}
