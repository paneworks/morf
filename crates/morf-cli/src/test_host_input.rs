//! The input and query half of `morf.test`'s host functions: clicks, the
//! wheel and keys, handed to the configuration under test through the
//! shell's own pointer and key paths, and the nodes it laid out.
//!
//! Split from `test_host` at the line gate.

use std::sync::Arc;

use morf_lua::{IpcTable, IpcValue};
use morf_wayland::LayerEvent;

use crate::headless::Headless;
use crate::headless_input::{button, keysym, modifiers};
use crate::test_host::{TestHost, list, map, number, optional_text, string, text};

/// Where keys go: the surface named, else the one last clicked (a
/// compositor hands the keyboard to the window pressed on), else the
/// primary.
fn key_role(
    subject: &Headless,
    surface: Option<&IpcValue>,
) -> Result<morf_wayland::SurfaceRole, String> {
    if optional_text(surface).is_none()
        && let Some(clicked) = subject.keyboard
        && subject
            .surfaces
            .iter()
            .any(|candidate| candidate.role == clicked)
    {
        return Ok(clicked);
    }
    role(subject, surface)
}

fn role(
    subject: &Headless,
    surface: Option<&IpcValue>,
) -> Result<morf_wayland::SurfaceRole, String> {
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
    subject.click(surface, (x, y), pressed)?;
    Ok(Vec::new())
}

pub(crate) fn press(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let x = number(arguments.first(), "x")?;
    let y = number(arguments.get(1), "y")?;
    let code = button(&optional_text(arguments.get(2)).unwrap_or_else(|| "left".to_owned()))?;
    let pressed = matches!(arguments.get(3), Some(IpcValue::Boolean(true)));
    let subject = host.subject()?;
    let surface = role(subject, arguments.get(4))?;
    subject.pointer(LayerEvent::PointerButton {
        surface,
        button: code,
        pressed,
        x,
        y,
    })?;
    Ok(Vec::new())
}

pub(crate) fn motion(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let x = number(arguments.first(), "x")?;
    let y = number(arguments.get(1), "y")?;
    let subject = host.subject()?;
    let surface = role(subject, arguments.get(2))?;
    subject.pointer(LayerEvent::PointerMotion { surface, x, y })?;
    Ok(Vec::new())
}

pub(crate) fn wheel(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let horizontal = number(arguments.first(), "dx")?;
    let vertical = number(arguments.get(1), "dy")?;
    let subject = host.subject()?;
    let named = arguments.get(4).and_then(text).is_some();
    let (surface, x, y) = match (subject.pointer, named) {
        (Some((surface, x, y)), false) => (surface, x, y),
        _ => (role(subject, arguments.get(4))?, 0.0, 0.0),
    };
    let x = number(arguments.get(2), "x").unwrap_or(x);
    let y = number(arguments.get(3), "y").unwrap_or(y);
    subject.pointer(LayerEvent::PointerAxis {
        surface,
        x,
        y,
        horizontal,
        vertical,
        horizontal_steps: horizontal.signum() as i32,
        vertical_steps: vertical.signum() as i32,
    })?;
    Ok(Vec::new())
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
    subject.key(surface, code, typed.as_deref(), held)?;
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

// ------------------------------------------------------------------ nodes

/// Every node on every surface, in paint order, with where it was laid out.
pub(crate) fn nodes(host: &mut TestHost) -> Result<Vec<IpcValue>, String> {
    let subject = host.subject()?;
    let mut found = Vec::new();
    {
        let scene = subject.runtime.scene();
        for surface in &subject.surfaces {
            let Some(layout) = &surface.layout else {
                continue;
            };
            // (node, depth, parent, visible so far)
            let mut pending = vec![(surface.root, 0i64, None, surface.visible)];
            while let Some((node, depth, parent, shown)) = pending.pop() {
                let visible = shown
                    && scene.bool_value(node, "visible").unwrap_or(true)
                    && scene.number(node, "opacity").unwrap_or(1.0) > 0.0;
                let rect = layout.surface_rect(&scene, node).unwrap_or_default();
                let read = |name: &str| {
                    scene
                        .has_property(node, name)
                        .unwrap_or(false)
                        .then(|| scene.string_value(node, name).ok())
                        .flatten()
                };
                found.push((
                    node,
                    parent,
                    vec![
                        (
                            "element",
                            string(format!(
                                "{:?}",
                                scene.element(node).map_err(|error| error.to_string())?
                            )),
                        ),
                        ("id", string(read("id").unwrap_or_default())),
                        ("text", read("text").map_or(IpcValue::Nil, string)),
                        ("x", IpcValue::Number(rect.x)),
                        ("y", IpcValue::Number(rect.y)),
                        ("width", IpcValue::Number(rect.width)),
                        ("height", IpcValue::Number(rect.height)),
                        ("visible", IpcValue::Boolean(visible)),
                        (
                            "opacity",
                            IpcValue::Number(scene.number(node, "opacity").unwrap_or(1.0)),
                        ),
                        ("exiting", IpcValue::Boolean(scene.is_exiting(node))),
                        ("depth", IpcValue::Integer(depth)),
                        ("surface", string(surface.label())),
                        ("surface_kind", string(surface.kind)),
                    ],
                ));
                if let Ok(children) = scene.children(node) {
                    for child in children.iter().rev() {
                        pending.push((*child, depth + 1, Some(node), visible));
                    }
                }
            }
        }
    }
    let rows = found
        .into_iter()
        .map(|(node, parent, mut fields)| {
            fields.push(("handle", IpcValue::Integer(host.number(node))));
            if let Some(parent) = parent {
                fields.push(("parent", IpcValue::Integer(host.number(parent))));
            }
            map(fields)
        })
        .collect();
    Ok(vec![IpcValue::Table(Arc::new(IpcTable::List(rows)))])
}

/// A node's text and its descendants', in order, joined by spaces.
pub(crate) fn text_of(
    host: &mut TestHost,
    arguments: &[IpcValue],
) -> Result<Vec<IpcValue>, String> {
    let node = host.handle(number(arguments.first(), "node")? as i64)?;
    let subject = host.subject()?;
    let scene = subject.runtime.scene();
    let mut parts = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if scene.has_property(node, "text").unwrap_or(false)
            && let Ok(text) = scene.string_value(node, "text")
            && !text.is_empty()
        {
            parts.push(text);
        }
        if let Ok(children) = scene.children(node) {
            pending.extend(children.iter().rev().copied());
        }
    }
    Ok(vec![string(parts.join(" "))])
}
