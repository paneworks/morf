//! How a `Path` node's outline and paint are read, a data channel's series
//! included.

use super::*;

/// Reads a `Path` node's outline and how it is drawn.
pub(super) fn path_paint(scene: &Scene, node: NodeHandle) -> Result<crate::path::PathPaint, RenderError> {
    let word = |property: &str| scene.string_value(node, property);
    let invalid = |message: String| RenderError::Scene(format!("Path: {message}"));
    let view_box = morf_scene::PathViewBox::parse(scene.current(node, "view_box")?).map_err(invalid)?;
    let d = match series_d(scene, node, view_box) {
        Some(d) => d,
        None => word("d")?.to_owned(),
    };
    Ok(crate::path::PathPaint {
        d,
        morph_to: word("morph_to")?.to_owned(),
        morph_progress: scene.number(node, "morph_progress")?,
        fill_color: scene.color_value(node, "fill_color")?,
        fill_rule: morf_scene::FillRule::parse(word("fill_rule")?)
            .ok_or_else(|| invalid("unknown fill_rule".to_owned()))?,
        stroke_color: scene.color_value(node, "stroke_color")?,
        stroke_width: scene.number(node, "stroke_width")?.max(0.0),
        stroke_cap: morf_scene::StrokeCap::parse(word("stroke_cap")?)
            .ok_or_else(|| invalid("unknown stroke_cap".to_owned()))?,
        stroke_join: morf_scene::StrokeJoin::parse(word("stroke_join")?)
            .ok_or_else(|| invalid("unknown stroke_join".to_owned()))?,
        miter_limit: scene.number(node, "miter_limit")?,
        dash: morf_scene::path_dash(scene.current(node, "dash")?).map_err(invalid)?,
        dash_offset: scene.number(node, "dash_offset")?,
        trim_start: scene.number(node, "trim_start")?,
        trim_end: scene.number(node, "trim_end")?,
        view_box,
        fill_mode: image_fill_mode(word("fill_mode")?)?,
    })
}

/// A channel's id, given as a number or as a handle with an `id`.
fn channel_id(value: &morf_scene::Value) -> Option<u64> {
    match value {
        morf_scene::Value::Number(n) if *n >= 1.0 => Some(*n as u64),
        morf_scene::Value::Map(fields) => match fields.get("id") {
            Some(morf_scene::Value::Number(n)) if *n >= 1.0 => Some(*n as u64),
            _ => None,
        },
        _ => None,
    }
}

/// The outline of a `Path` that draws a data channel (`series`), made from
/// the channel's numbers now as its `plot` says; `None` when it draws `d`.
fn series_d(scene: &Scene, node: NodeHandle, view_box: Option<morf_scene::PathViewBox>) -> Option<String> {
    use morf_vector::series::{Plot, path};
    use morf_scene::Value;
    let id = channel_id(scene.current(node, "series").ok()?)?;
    let Some(channel) = morf_scene::channel_by_id(id) else { return Some("M0 0".into()) };
    let empty = std::collections::BTreeMap::new();
    let fields = match scene.current(node, "plot") {
        Ok(Value::Map(fields)) => fields,
        _ => &empty,
    };
    let number = |key: &str| match fields.get(key) {
        Some(Value::Number(n)) if n.is_finite() => Some(*n),
        _ => None,
    };
    let flag = |key: &str| matches!(fields.get(key), Some(Value::Bool(true)));
    let word = |key: &str| match fields.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    };
    let top = match fields.get("top") {
        Some(Value::Number(n)) if n.is_finite() => Some(Some(*n)),
        Some(_) => Some(None),
        // No top: a ring (a history) scales to its peak, a frame to 0..1.
        None if channel.is_ring() => Some(None),
        None => None,
    };
    let size = view_box.map_or((100.0, 100.0), |v| (v.width, v.height));
    let samples = if channel.is_ring() { channel.capacity() } else { 0 };
    let plot = Plot::from_fields(number, flag, word, top, size, samples);
    let (values, _) = channel.snapshot();
    let others = fields
        .get("with")
        .and_then(channel_id)
        .and_then(morf_scene::channel_by_id)
        .map(|other| other.snapshot().0)
        .unwrap_or_default();
    Some(path(&values, &others, &plot))
}
