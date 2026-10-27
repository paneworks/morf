//! Coverage of declared keyboard nodes. This registry is scoped to Morf's
//! primary scene; it cannot establish the absence of another program's OSK.
use super::*;

pub(super) fn keyboard(
    scene: &Scene,
    primary: NodeHandle,
    regions: &[Region],
) -> Result<Json, String> {
    let mut pending: Vec<_> = scene
        .roots()
        .into_iter()
        .map(|node| (node, false))
        .collect();
    let mut keyboards = Vec::new();
    let mut visited = 0;
    while let Some((node, inside)) = pending.pop() {
        visited += 1;
        if visited > 65_536 {
            return Err("native semantic registry exceeds node budget".into());
        }
        let inside = inside || node == primary;
        let declaration = semantic(
            scene
                .current(node, "input_context")
                .map_err(|e| e.to_string())?,
        )?;
        if let Some(declaration) = declaration
            && declaration["role"] == "keyboard"
        {
            keyboards.push((node, inside, declaration["layout"].clone()));
        }
        for &child in scene.children(node).map_err(|e| e.to_string())? {
            pending.push((child, inside));
        }
    }
    let mut result =
        json!({"scope":"registered_morf_primary","registered":keyboards.len(),"state":"unknown"});
    let [(node, true, layout)] = keyboards.as_slice() else {
        return Ok(result);
    };
    let id = scene.string_value(*node, "id").map_err(|e| e.to_string())?;
    if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
        return Ok(result);
    }
    let Some(hidden) = hidden(scene, *node, primary)? else {
        return Ok(result);
    };
    let region = regions.iter().find(|region| region.node == node.to_bits());
    let state = match (hidden, region) {
        (true, None) => "hidden",
        (false, Some(region))
            if region
                .semantic
                .as_ref()
                .is_some_and(|s| s["role"] == "keyboard") =>
        {
            "visible"
        }
        _ => return Ok(result),
    };
    result["state"] = json!(state);
    result["node"] = json!(node.to_bits().to_string());
    result["layout"] = layout.clone();
    Ok(result)
}
fn hidden(scene: &Scene, node: NodeHandle, primary: NodeHandle) -> Result<Option<bool>, String> {
    let mut at = Some(node);
    let mut hidden = false;
    while let Some(node) = at {
        if scene.is_exiting(node) {
            return Ok(None);
        }
        let opacity = scene.number(node, "opacity").map_err(|e| e.to_string())?;
        if !opacity.is_finite() {
            return Err("invalid registry opacity".into());
        }
        hidden |= !scene
            .bool_value(node, "visible")
            .map_err(|e| e.to_string())?
            || opacity <= 0.0;
        if node == primary {
            return Ok(Some(hidden));
        }
        at = scene.parent(node).map_err(|e| e.to_string())?;
    }
    Ok(None)
}
