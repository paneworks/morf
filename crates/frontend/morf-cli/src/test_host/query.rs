//! What a spec reads off the configuration under test: the nodes it laid
//! out, their text, and the accessible tree a screen reader is given.

use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use super::TestHost;
use super::values::{map, number, string};

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
            // The surface's tree, then the engine's band painted over it.
            let band = (surface.kind == "primary")
                .then(|| subject.reload_band())
                .flatten()
                .map(|band| (band.root, &band.layout));
            for (tree, layout) in std::iter::once((surface.root, layout)).chain(band) {
                // (node, depth, parent, visible so far)
                let mut pending = vec![(tree, 0i64, None, surface.visible)];
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
                                IpcValue::Boolean(
                                    scene.bool_value(node, "focused").unwrap_or(false),
                                ),
                            ),
                            (
                                "visual_focus",
                                IpcValue::Boolean(
                                    scene.bool_value(node, "visual_focus").unwrap_or(false),
                                ),
                            ),
                            (
                                "contains_pointer",
                                IpcValue::Boolean(pointer.is_some_and(|(x, y)| {
                                    layout.contains_point(&scene, node, x, y)
                                })),
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
fn shown_glyph(scene: &morf_scene::Scene, node: morf_scene::NodeHandle) -> Option<String> {
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
/// given it (`morf_scene::Scene::accessible_tree`): rows root first, each
/// with its role, name, value and states, and its accessible parent.
pub(crate) fn accessible(host: &mut TestHost) -> Result<Vec<IpcValue>, String> {
    use morf_scene::{AccessibleValue, Checked};
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
