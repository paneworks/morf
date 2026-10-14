//! Input a spec hands the configuration under test: clicks, buttons, motion,
//! the wheel, touch and keys, through the shell's own pointer and key paths.

use morf_app::Event;
use morf_value::IpcValue;

use super::TestHost;
use super::values::{list, number, optional_text, text};
use morf_host::headless::Headless;
use morf_host::headless::input::{button, keysym, modifiers};

mod touch;
pub(crate) use touch::touch;

/// Where keys go: the surface named, else the one last clicked (a
/// compositor hands the keyboard to the window pressed on), else the
/// primary.
fn key_role(subject: &Headless, surface: Option<&IpcValue>) -> Result<morf_app::WindowId, String> {
    if optional_text(surface).is_none()
        && let Some(clicked) = subject.seat().keyboard
        && subject
            .surfaces
            .iter()
            .any(|candidate| candidate.role == clicked)
    {
        return Ok(clicked);
    }
    role(subject, surface)
}

fn role(subject: &Headless, surface: Option<&IpcValue>) -> Result<morf_app::WindowId, String> {
    let wanted = optional_text(surface);
    let index = subject.surface_index(wanted.as_deref())?;
    Ok(subject.surfaces[index].role)
}

pub(crate) fn click(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let x = number(arguments.first(), "x")?;
    let y = number(arguments.get(1), "y")?;
    let pressed = button(&optional_text(arguments.get(2)).unwrap_or_else(|| "left".to_owned()))?;
    let subject = host.subject()?;
    let surface = role(subject, arguments.get(3))?;
    let held =
        morf_host::headless::input::pointer_modifiers(optional_text(arguments.get(4)).as_deref())?;
    subject.click(surface, (x, y), pressed, held)?;
    Ok(Vec::new())
}

pub(crate) fn press(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let x = number(arguments.first(), "x")?;
    let y = number(arguments.get(1), "y")?;
    let code = button(&optional_text(arguments.get(2)).unwrap_or_else(|| "left".to_owned()))?;
    let pressed = matches!(arguments.get(3), Some(IpcValue::Boolean(true)));
    let subject = host.subject()?;
    let surface = role(subject, arguments.get(4))?;
    let modifiers =
        morf_host::headless::input::pointer_modifiers(optional_text(arguments.get(5)).as_deref())?;
    subject.pointer(Event::PointerButton {
        surface,
        button: code,
        pressed,
        x,
        y,
        modifiers,
    })?;
    Ok(Vec::new())
}

pub(crate) fn motion(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let x = number(arguments.first(), "x")?;
    let y = number(arguments.get(1), "y")?;
    let subject = host.subject()?;
    let surface = role(subject, arguments.get(2))?;
    subject.pointer(Event::PointerMotion { surface, x, y })?;
    Ok(Vec::new())
}

/// The pointer leaving a surface -- the one named, else the one it is on --
/// as a compositor says so when it moves off the surface's input region.
pub(crate) fn leave(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let subject = host.subject()?;
    let surface = match (subject.seat().pointer, optional_text(arguments.first())) {
        (Some((surface, _, _)), None) => surface,
        _ => role(subject, arguments.first())?,
    };
    subject.pointer(Event::PointerLeave { surface })?;
    Ok(Vec::new())
}

pub(crate) fn wheel(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let horizontal = number(arguments.first(), "dx")?;
    let vertical = number(arguments.get(1), "dy")?;
    let subject = host.subject()?;
    let named = arguments.get(4).and_then(text).is_some();
    let (surface, x, y) = match (subject.seat().pointer, named) {
        (Some((surface, x, y)), false) => (surface, x, y),
        _ => (role(subject, arguments.get(4))?, 0.0, 0.0),
    };
    let x = number(arguments.get(2), "x").unwrap_or(x);
    let y = number(arguments.get(3), "y").unwrap_or(y);
    subject.pointer(Event::PointerAxis {
        surface,
        x,
        y,
        horizontal,
        vertical,
        horizontal_steps: steps(horizontal),
        vertical_steps: steps(vertical),
        modifiers: morf_host::headless::input::pointer_modifiers(
            optional_text(arguments.get(5)).as_deref(),
        )?,
    })?;
    Ok(Vec::new())
}

/// The notches a turn of `amount` is: none for none (`signum` says 1 for 0).
fn steps(amount: f64) -> i32 {
    if amount == 0.0 {
        0
    } else {
        amount.signum() as i32
    }
}

pub(crate) fn key(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let name = optional_text(arguments.first()).ok_or("test.key wants a key name")?;
    let (code, typed) = keysym(&name).ok_or_else(|| format!("unknown key `{name}`"))?;
    let held = list(arguments.get(1))
        .iter()
        .filter_map(text)
        .collect::<Vec<_>>();
    let held = modifiers(&held)?;
    // A key typed with Ctrl or Alt held is a shortcut, not text.
    let typed = typed.filter(|_| !held.ctrl && !held.alt && !held.logo);
    let subject = host.subject()?;
    let surface = key_role(subject, arguments.get(2))?;
    // `phase`: "down" presses it, "up" lets it go; both by default.
    let (press, release) = match optional_text(arguments.get(3)).as_deref() {
        Some("down") => (true, false),
        Some("up") => (false, true),
        _ => (true, true),
    };
    subject.key_phase(surface, code, typed.as_deref(), held, press, release)?;
    Ok(Vec::new())
}

pub(crate) fn type_text(
    host: &mut TestHost,
    arguments: &[IpcValue],
) -> Result<Vec<IpcValue>, String> {
    let typed = optional_text(arguments.first()).unwrap_or_default();
    let subject = host.subject()?;
    let surface = key_role(subject, arguments.get(1))?;
    for character in typed.chars() {
        let name = match character {
            '\n' => "Return".to_owned(),
            '\t' => "Tab".to_owned(),
            other => other.to_string(),
        };
        let (code, text) = keysym(&name).ok_or_else(|| format!("cannot type `{name}`"))?;
        subject.key(surface, code, text.as_deref(), Default::default())?;
    }
    Ok(Vec::new())
}
