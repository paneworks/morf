//! Where an event goes: which nodes take keys and in what order, where a key
//! goes when the focused node does not take it, which buttons a pointer area
//! answers, whether a wheel stops at a node, and the `hovered` and `pressed`
//! a pointer area keeps.

use morf_scene::{Element, NodeHandle, Scene, Value as SceneValue};

use super::{Events, KeyModifiers, UiEvent};

const RETURN: u32 = 0xff0d;
const KP_ENTER: u32 = 0xff8d;
const SPACE: u32 = 0x20;

/// Whether `node` lies in the subtree rooted at `root` (`root` included).
pub fn node_in_subtree(scene: &Scene, root: NodeHandle, node: NodeHandle) -> bool {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate == root {
            return true;
        }
        current = scene.parent(candidate).ok().flatten();
    }
    false
}

/// Whether keys can go to a node: it handles them, or it is a text input or
/// a terminal, which take them themselves.
pub fn takes_keys(scene: &Scene, events: &Events, node: NodeHandle) -> bool {
    events.handles_keys(node)
        || matches!(
            scene.element(node).ok(),
            Some(Element::TextInput | Element::Terminal)
        )
}

/// Every node under `root` that takes keys and can hold focus -- shown,
/// enabled and staying, through its ancestors -- in tree order.
pub fn key_targets_in(scene: &Scene, events: &Events, root: NodeHandle) -> Vec<NodeHandle> {
    scene.focus_nodes(root, |node| takes_keys(scene, events, node))
}

/// The nodes a key nothing has focus for is offered to, in order: one with
/// `focus` set first, then the rest in tree order.
pub fn key_offer_order(scene: &Scene, events: &Events, root: NodeHandle) -> Vec<NodeHandle> {
    let mut targets = key_targets_in(scene, events, root);
    targets.sort_by_key(|node| !scene.bool_value(*node, "focus").unwrap_or(false));
    targets
}

/// The first key target in `root`: one with `focus` set, else the first.
pub fn first_key_target(scene: &Scene, events: &Events, root: NodeHandle) -> Option<NodeHandle> {
    let targets = key_targets_in(scene, events, root);
    targets
        .iter()
        .copied()
        .find(|node| scene.bool_value(*node, "focus").unwrap_or(false))
        .or_else(|| targets.first().copied())
}

/// The key target after `current` in `root`, round to the first.
pub fn next_key_target(
    scene: &Scene,
    events: &Events,
    root: NodeHandle,
    current: Option<NodeHandle>,
) -> Option<NodeHandle> {
    let targets = key_targets_in(scene, events, root);
    if targets.is_empty() {
        return None;
    }
    let next = current
        .and_then(|current| targets.iter().position(|node| *node == current))
        .map_or(0, |index| (index + 1) % targets.len());
    Some(targets[next])
}

/// The nearest enabled, visible ancestor of a hit-tested node (itself
/// included) that takes keys.
pub fn key_target_for_node(scene: &Scene, events: &Events, node: NodeHandle) -> Option<NodeHandle> {
    let mut current = Some(node);
    while let Some(node) = current {
        if takes_keys(scene, events, node)
            && scene.bool_value(node, "enabled").unwrap_or(false)
            && scene.bool_value(node, "visible").unwrap_or(false)
        {
            return Some(node);
        }
        current = scene.parent(node).ok().flatten();
    }
    None
}

/// The node a key pressed while `node` has focus goes to: itself when it
/// takes keys, else its nearest ancestor that does.
pub fn key_route(scene: &Scene, events: &Events, node: NodeHandle) -> Option<NodeHandle> {
    let mut current = Some(node);
    while let Some(node) = current {
        if takes_keys(scene, events, node) {
            return Some(node);
        }
        current = scene.parent(node).ok().flatten();
    }
    None
}

/// Whether a key is one that clicks `node`: Return, the keypad's Enter or
/// Space with no Ctrl, Alt or Super, on a pointer area with an `on_clicked`
/// and no keys of its own.
pub fn activates_by_key(
    scene: &Scene,
    events: &Events,
    node: NodeHandle,
    keysym: u32,
    modifiers: KeyModifiers,
) -> bool {
    let clickable = scene.element(node).ok() == Some(Element::MouseArea)
        && !events.handles_keys(node)
        && events.has(node, UiEvent::Clicked);
    let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    clickable && plain && matches!(keysym, RETURN | KP_ENTER | SPACE)
}

/// Whether a key at `node` is for its own handler alone, which may decline
/// it and pass it up: it handles keys and is not a text input or a
/// terminal, which have the last word on their keys.
pub fn bubbles_keys(scene: &Scene, events: &Events, node: NodeHandle) -> bool {
    events.handles_keys(node)
        && !matches!(
            scene.element(node).ok(),
            Some(Element::TextInput | Element::Terminal)
        )
}

/// Whether a pointer area (or text input, link or terminal) answers a Linux
/// input button code.
pub fn accepts_pointer_button(scene: &Scene, node: NodeHandle, button: u32) -> bool {
    match scene.element(node).ok() {
        // A text input places its caret with the primary button, and a
        // link takes the primary button.
        Some(Element::TextInput | Element::Text) => return button == 0x110,
        // A terminal passes all three on to a program that wants them.
        Some(Element::Terminal) => return matches!(button, 0x110..=0x112),
        _ => {}
    }
    let Ok(value) = scene.current(node, "accepted_buttons") else {
        return false;
    };
    let accepted = |value: &SceneValue| match value {
        SceneValue::String(name) => match name.as_str() {
            "all" => true,
            "left" => button == 0x110,
            "right" => button == 0x111,
            "middle" => button == 0x112,
            _ => false,
        },
        SceneValue::Number(code) => *code == f64::from(button),
        _ => false,
    };
    match value {
        SceneValue::List(values) => values.iter().any(accepted),
        value => accepted(value),
    }
}

/// Whether a wheel turn over `node` stops there: a node with an `on_wheel`
/// handler, or any Flickable or terminal. Everything else lets the wheel
/// bubble on to what is beneath it, so a button on a scrolling page does not
/// swallow the page's scroll.
pub fn takes_wheel(scene: &Scene, events: &Events, node: NodeHandle) -> bool {
    match scene.element(node) {
        Ok(Element::Flickable | Element::Terminal) => true,
        Ok(_) => events.has(node, UiEvent::Wheel),
        Err(_) => false,
    }
}

/// The `hovered` or `pressed` a pointer area takes from `event`, when it
/// changes: a binding follows hover without a signal per area.
pub fn pointer_state_change(
    scene: &Scene,
    node: NodeHandle,
    event: UiEvent,
) -> Option<(&'static str, bool)> {
    let (property, value) = match event {
        UiEvent::PointerEntered => ("hovered", true),
        UiEvent::PointerExited => ("hovered", false),
        UiEvent::Pressed => ("pressed", true),
        UiEvent::Released | UiEvent::TouchCanceled => ("pressed", false),
        _ => return None,
    };
    if scene.element(node).ok() != Some(Element::MouseArea)
        || scene.bool_value(node, property).ok() == Some(value)
    {
        return None;
    }
    Some((property, value))
}

/// Where a wheel's pixel delta moves a Flickable's `content_x` and
/// `content_y`, kept inside `0..=max` (how far the content can scroll on
/// each axis, which only the layout knows): each property that moves, with
/// its new value.
pub fn flickable_scroll(
    scene: &Scene,
    node: NodeHandle,
    pixels: (f64, f64),
    max: (f64, f64),
) -> Vec<(&'static str, f64)> {
    let mut moves = Vec::new();
    for (property, delta, limit) in [
        ("content_x", pixels.0, max.0),
        ("content_y", pixels.1, max.1),
    ] {
        if delta == 0.0 {
            continue;
        }
        let Ok(SceneValue::Number(current)) = scene.target(node, property) else {
            continue;
        };
        let next = (current + delta).clamp(0.0, limit.max(0.0));
        if next != *current {
            moves.push((property, next));
        }
    }
    moves
}
