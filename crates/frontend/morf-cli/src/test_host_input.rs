//! The input and query half of `morf.test`'s host functions: clicks, the
//! wheel and keys, handed to the configuration under test through the
//! shell's own pointer and key paths, and the nodes it laid out.
//!
//! Split from `test_host` at the line gate.

use std::sync::Arc;

use morf_host::morf_app::Event;
use morf_value::{IpcTable, IpcValue};

use crate::test_host::{TestHost, list, map, number, optional_text, string, text};
use morf_host::headless::Headless;
use morf_host::headless_input::{button, keysym, modifiers};

/// Where keys go: the surface named, else the one last clicked (a
/// compositor hands the keyboard to the window pressed on), else the
/// primary.
fn key_role(
    subject: &Headless,
    surface: Option<&IpcValue>,
) -> Result<morf_host::morf_app::WindowId, String> {
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

fn role(
    subject: &Headless,
    surface: Option<&IpcValue>,
) -> Result<morf_host::morf_app::WindowId, String> {
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
        morf_host::headless_input::pointer_modifiers(optional_text(arguments.get(4)).as_deref())?;
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
        morf_host::headless_input::pointer_modifiers(optional_text(arguments.get(5)).as_deref())?;
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
        modifiers: morf_host::headless_input::pointer_modifiers(
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

// ------------------------------------------------------------------ nodes

/// Every node on every surface, in paint order, with where it was laid out.
pub(crate) fn nodes(host: &mut TestHost) -> Result<Vec<IpcValue>, String> {
    let subject = host.subject()?;
    let mut found = Vec::new();
    {
        let scene = subject.runtime.scene();
        for surface in &subject.surfaces {
            let Some(layout) = subject.layout_of(surface) else {
                continue;
            };
            // Where the pointer is on this surface, if it is on it.
            let pointer = subject
                .input()
                .pointer
                .filter(|(role, _, _)| *role == surface.role)
                .map(|(_, x, y)| (x, y));
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
                        (
                            "text",
                            read("text")
                                .map(str::to_owned)
                                .or_else(|| shown_glyph(&scene, node))
                                .map_or(IpcValue::Nil, string),
                        ),
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
                        (
                            "focused",
                            IpcValue::Boolean(scene.bool_value(node, "focused").unwrap_or(false)),
                        ),
                        (
                            "visual_focus",
                            IpcValue::Boolean(
                                scene.bool_value(node, "visual_focus").unwrap_or(false),
                            ),
                        ),
                        (
                            "contains_pointer",
                            IpcValue::Boolean(
                                pointer.is_some_and(|(x, y)| {
                                    layout.contains_point(&scene, node, x, y)
                                }),
                            ),
                        ),
                        ("depth", IpcValue::Integer(depth)),
                        // Whether it clips what is under it, and turns: a
                        // check of what spills reads the visible box through
                        // both.
                        (
                            "clip",
                            IpcValue::Boolean(scene.bool_value(node, "clip").unwrap_or(false)),
                        ),
                        (
                            "rotation",
                            IpcValue::Number(scene.number(node, "rotation").unwrap_or(0.0)),
                        ),
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

/// A glyph shape's text: the glyph its morph is nearer (`glyph`, or
/// `glyph_morph_to` past halfway), so a number drawn as morphing glyphs reads
/// as text does.
fn shown_glyph(
    scene: &morf_host::morf_scene::Scene,
    node: morf_host::morf_scene::NodeHandle,
) -> Option<String> {
    if scene.string_value(node, "shape").ok()? != "glyph" {
        return None;
    }
    let late = scene.number(node, "morph_progress").unwrap_or(0.0) > 0.5
        && scene.string_value(node, "morph_to").ok() == Some("glyph");
    let name = if late { "glyph_morph_to" } else { "glyph" };
    scene.string_value(node, name).ok().map(str::to_owned)
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

/// Every visible surface's accessible tree, as a screen reader would be
/// given it (`morf_host::morf_scene::Scene::accessible_tree`): rows root first, each
/// with its role, name, value and states, and its accessible parent.
pub(crate) fn accessible(host: &mut TestHost) -> Result<Vec<IpcValue>, String> {
    use morf_host::morf_scene::{AccessibleValue, Checked};
    let subject = host.subject()?;
    let mut found = Vec::new();
    {
        let scene = subject.runtime.scene();
        for surface in subject.surfaces.iter().filter(|s| s.visible) {
            let Some(layout) = subject.layout_of(surface) else {
                continue;
            };
            let nodes = scene.accessible_tree(surface.root, "window", &surface.label(), &|node| {
                layout
                    .surface_rect(&scene, node)
                    .map(|g| (g.x, g.y, g.width, g.height))
            });
            let mut parents = std::collections::HashMap::new();
            for item in &nodes {
                for child in &item.children {
                    parents.insert(*child, item.node);
                }
            }
            for item in nodes {
                let (x, y, w, h) = item.bounds.unwrap_or_default();
                let opt = |b: Option<bool>| b.map_or(IpcValue::Nil, IpcValue::Boolean);
                let mut fields = vec![
                    ("role", string(item.role.clone())),
                    ("name", string(item.name.clone())),
                    ("description", string(item.description.clone())),
                    (
                        "value",
                        match &item.value {
                            Some(AccessibleValue::Number(n)) => IpcValue::Number(*n),
                            Some(AccessibleValue::Text(t)) => string(t.clone()),
                            None => IpcValue::Nil,
                        },
                    ),
                    (
                        "minimum",
                        item.minimum.map_or(IpcValue::Nil, IpcValue::Number),
                    ),
                    (
                        "maximum",
                        item.maximum.map_or(IpcValue::Nil, IpcValue::Number),
                    ),
                    (
                        "checked",
                        match item.checked {
                            Some(Checked::True) => IpcValue::Boolean(true),
                            Some(Checked::False) => IpcValue::Boolean(false),
                            Some(Checked::Mixed) => string("mixed"),
                            None => IpcValue::Nil,
                        },
                    ),
                    ("expanded", opt(item.expanded)),
                    ("selected", opt(item.selected)),
                    ("disabled", IpcValue::Boolean(item.disabled)),
                    ("focusable", IpcValue::Boolean(item.focusable)),
                    ("focused", IpcValue::Boolean(item.focused)),
                    ("x", IpcValue::Number(x)),
                    ("y", IpcValue::Number(y)),
                    ("width", IpcValue::Number(w)),
                    ("height", IpcValue::Number(h)),
                    ("children", IpcValue::Integer(item.children.len() as i64)),
                    ("surface", string(surface.label())),
                    (
                        "id",
                        string(scene.string_value(item.node, "id").unwrap_or("").to_owned()),
                    ),
                ];
                fields.retain(|(_, v)| *v != IpcValue::Nil);
                found.push((item.node, parents.get(&item.node).copied(), fields));
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

/// Does what a screen reader asks of a node: `(handle, action, value)`.
pub(crate) fn accessible_action(
    host: &mut TestHost,
    arguments: &[IpcValue],
) -> Result<Vec<IpcValue>, String> {
    let node = host.handle(number(arguments.first(), "node")? as i64)?;
    let action = match arguments.get(1) {
        Some(IpcValue::String(a)) => a.clone(),
        _ => return Err("accessible_action wants an action".into()),
    };
    let value = arguments.get(2).cloned().filter(|v| *v != IpcValue::Nil);
    let subject = host.subject()?;
    let root = {
        let scene = subject.runtime.scene();
        let mut root = node;
        while let Ok(Some(parent)) = scene.parent(root) {
            root = parent;
        }
        root
    };
    let ran = subject
        .runtime
        .accessible_action(root, node, &action, value);
    Ok(vec![IpcValue::Boolean(ran)])
}

/// Configures a window to a new size, as a compositor does when a person
/// resizes it or a phone fits it to the screen: `(surface label, w, h)`.
pub(crate) fn resize_window(
    host: &mut TestHost,
    arguments: &[IpcValue],
) -> Result<Vec<IpcValue>, String> {
    let label = arguments
        .first()
        .and_then(text)
        .ok_or("resize_window wants a surface")?;
    let width = number(arguments.get(1), "width")?.max(1.0) as u32;
    let height = number(arguments.get(2), "height")?.max(1.0) as u32;
    let subject = host.subject()?;
    let Some(surface) = subject
        .surfaces
        .iter_mut()
        .find(|s| s.label() == label || s.name == label)
    else {
        return Err(format!("no surface {label}"));
    };
    let Some(id) = surface.id else {
        return Err(format!("{label} is not a window"));
    };
    let role = surface.role;
    if !surface.open {
        surface.size = (width, height);
        let changed = subject.runtime.set_window_surface_size(id, width, height);
        return Ok(vec![IpcValue::Boolean(changed)]);
    }
    // An open window is resized as a compositor does it: a configure.
    let changed = surface.size != (width, height);
    if let Some(backend) = subject
        .host
        .as_mut()
        .and_then(|host| host.backend.as_headless_mut())
    {
        backend.resize(role, (width, height));
    }
    subject.frame(std::time::Duration::ZERO);
    Ok(vec![IpcValue::Boolean(changed)])
}
