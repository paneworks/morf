//! What a frame tells a field: where its text landed, and the blink.

use super::*;

/// What the last frame made of a field: its box, its shaped text, and where
/// that sits in the box. Keeps the caret in view by scrolling, and publishes
/// how big the content is.
pub(crate) fn observe_shaped(
    state: &mut ReactiveState,
    node: NodeHandle,
    geometry: Geometry,
    shaped: &str,
    map: CaretMap,
    measured_height: f64,
) {
    if !pull(state, node) {
        return;
    }
    let shown = display(state, node);
    let multiline = state.scene.bool_value(node, "multiline").unwrap_or(false);
    let focused = state.scene.bool_value(node, "focus").unwrap_or(false);
    let caret_width = state
        .scene
        .number(node, "caret_width")
        .unwrap_or(0.0)
        .max(0.0);
    let offset_y = if multiline {
        0.0
    } else {
        let spare = (geometry.height - measured_height).max(0.0);
        match state
            .scene
            .string_value(node, "vertical_alignment")
            .unwrap_or("center")
        {
            "top" => 0.0,
            "bottom" => spare,
            _ => spare / 2.0,
        }
    };
    let (left, right, bottom) = map.extent();
    let text = state.scene.string_value(node, "text").unwrap_or_default();
    let Some(input) = state.text_inputs.get(&node) else {
        return;
    };
    let cursor = shown.to_display(text, input.buffer.cursor());
    let caret = if shown.placeholder {
        None
    } else {
        Some(map.caret(cursor))
    };
    let (content_width, content_height) = if shown.placeholder {
        (0.0, bottom as f64)
    } else {
        ((right - left.min(0.0)) as f64 + caret_width, bottom as f64)
    };
    let wraps = multiline && state.scene.bool_value(node, "wrap").unwrap_or(true);
    let mut scroll_x = state.scene.number(node, "scroll_x").unwrap_or(0.0);
    let mut scroll_y = state.scene.number(node, "scroll_y").unwrap_or(0.0);
    if wraps || shown.placeholder {
        scroll_x = 0.0;
    } else {
        if focused && let Some(caret) = caret {
            let caret_left = f64::from(caret.x) - caret_width / 2.0;
            let caret_right = f64::from(caret.x) + caret_width / 2.0;
            if caret_left < scroll_x {
                scroll_x = caret_left;
            } else if caret_right > scroll_x + geometry.width {
                scroll_x = caret_right - geometry.width;
            }
        }
        let least = f64::from(left.min(0.0));
        let most = (f64::from(right) + caret_width - geometry.width).max(least);
        scroll_x = scroll_x.clamp(least, most);
    }
    if multiline {
        if focused && let Some(caret) = caret {
            let top = f64::from(caret.y);
            let bottom = top + f64::from(caret.height);
            if top < scroll_y {
                scroll_y = top;
            } else if bottom > scroll_y + geometry.height {
                scroll_y = bottom - geometry.height;
            }
        }
        scroll_y = scroll_y.clamp(0.0, (content_height - geometry.height).max(0.0));
    } else {
        scroll_y = 0.0;
    }
    let ime_rect = caret.map(|caret| {
        (
            (geometry.x + f64::from(caret.x) - scroll_x).round() as i32,
            (geometry.y + offset_y + f64::from(caret.y) - scroll_y).round() as i32,
            caret_width.ceil().max(1.0) as i32,
            f64::from(caret.height).ceil() as i32,
        )
    });
    if let Some(input) = state.text_inputs.get_mut(&node) {
        input.map = Some((shaped.to_owned(), map));
        input.geometry = Some(geometry);
        input.offset_y = offset_y;
    }
    for (property, value) in [
        ("scroll_x", scroll_x),
        ("scroll_y", scroll_y),
        ("content_width", content_width),
        ("content_height", content_height),
    ] {
        // Compared against the target, so an animated scroll is not asked
        // to start over every frame it is still on its way.
        let same = matches!(
            state.scene.target(node, property),
            Ok(SceneValue::Number(now)) if (now - value).abs() < 0.01
        );
        if !same {
            let _ = assign_scene_property(state, node, property, SceneValue::Number(value));
        }
    }
    if focused
        && let Some(rect) = ime_rect
        && state
            .text_inputs
            .get(&node)
            .is_some_and(|input| input.ime_rect != Some(rect))
    {
        if let Some(input) = state.text_inputs.get_mut(&node) {
            input.ime_rect = Some(rect);
        }
        state
            .text_input_requests
            .push(TextInputRequest::CursorRect {
                x: rect.0,
                y: rect.1,
                width: rect.2,
                height: rect.3,
            });
    }
}

/// Moves the focused field's blink on. Returns whether its caret changed.
/// When the focused field's caret next turns on or off, if it blinks: the
/// loop sleeps until then rather than waking to ask.
pub(crate) fn next_blink(state: &ReactiveState) -> Option<Instant> {
    let node = state.focused_input?;
    let input = state.text_inputs.get(&node)?;
    let interval = state
        .scene
        .number(node, "caret_blink_interval")
        .unwrap_or(0.0);
    if !(interval.is_finite() && interval > 0.0) {
        return None;
    }
    let half = std::time::Duration::from_secs_f64(interval / 1000.0);
    let since = Instant::now().saturating_duration_since(input.blink_start);
    let halves = (since.as_secs_f64() / half.as_secs_f64()) as u32;
    Some(input.blink_start + half * (halves + 1))
}

pub(crate) fn blink(state: &mut ReactiveState, now: Instant) -> bool {
    let Some(node) = state.focused_input else {
        return false;
    };
    let Some(input) = state.text_inputs.get(&node) else {
        return false;
    };
    let interval = state
        .scene
        .number(node, "caret_blink_interval")
        .unwrap_or(0.0);
    let visible = if interval > 0.0 {
        let halves = now.duration_since(input.blink_start).as_secs_f64() * 1000.0 / interval;
        (halves as u64).is_multiple_of(2)
    } else {
        true
    };
    if state
        .scene
        .bool_value(node, "caret_visible")
        .unwrap_or(true)
        == visible
    {
        return false;
    }
    let _ = assign_scene_property(state, node, "caret_visible", SceneValue::Bool(visible));
    true
}
