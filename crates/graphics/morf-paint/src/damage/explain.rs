//! `MORF_DAMAGE_LOG=1`: what made the frame's large damage areas, for a
//! frame log whose rectangles say where but not why.

use morf_layout::Geometry;
use morf_scene::NodeHandle;

use crate::commands::*;

/// `MORF_DAMAGE_LOG=1`: says on stderr which command or layer made a damage
/// area of a megapixel or more, and what about it changed -- the one thing a
/// frame log's rectangle cannot say.
pub(super) fn damage_log_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| {
        std::env::var_os("MORF_DAMAGE_LOG").is_some_and(|value| !value.is_empty() && value != "0")
    })
}

pub(super) fn explain(
    what: &str,
    node: NodeHandle,
    bounds: Geometry,
    detail: impl FnOnce() -> String,
) {
    if !damage_log_wanted() || bounds.width * bounds.height < 1_000_000.0 {
        return;
    }
    eprintln!(
        "damage: {what} {node:?} {:.0}x{:.0}+{:.0}+{:.0}: {}",
        bounds.width,
        bounds.height,
        bounds.x,
        bounds.y,
        detail()
    );
}

pub(super) fn kind(command: &DrawCommand) -> &'static str {
    match command {
        DrawCommand::Quad { .. } => "quad",
        DrawCommand::Text { .. } => "text",
        DrawCommand::Texture { .. } => "texture",
        DrawCommand::Path { .. } => "path",
        DrawCommand::Field { .. } => "field",
        DrawCommand::Backdrop { .. } => "backdrop",
        DrawCommand::Terminal { .. } => "terminal",
    }
}

pub(super) fn command_change(old: &DrawCommand, new: &DrawCommand, reordered: bool) -> String {
    let mut out = kind(new).to_owned();
    if reordered {
        out.push_str(", paint order moved");
    }
    if let (
        DrawCommand::Field {
            layers: old_layers,
            shader: old_shader,
            ..
        },
        DrawCommand::Field { layers, shader, .. },
    ) = (old, new)
    {
        out.push_str(&format!(
            ", layers {} -> {}",
            old_layers.len(),
            layers.len()
        ));
        if shader.is_some() || old_shader.is_some() {
            out.push_str(", shader");
        }
        let mut same = old.clone();
        if let DrawCommand::Field {
            layers: same_layers,
            ..
        } = &mut same
        {
            same_layers.clone_from(layers);
        }
        if same != *new {
            out.push_str(", more than its layers changed");
        }
    }
    out
}

pub(super) fn layer_change(old: &Layer, new: &Layer) -> String {
    let mut parts = Vec::new();
    if old.opacity != new.opacity {
        parts.push(format!("opacity {} -> {}", old.opacity, new.opacity));
    }
    if old.bounds != new.bounds {
        parts.push("bounds".to_owned());
    }
    if old.commands.len() != new.commands.len() {
        parts.push(format!(
            "commands {} -> {}",
            old.commands.len(),
            new.commands.len()
        ));
    }
    if old.blur != new.blur || old.shadow_blur != new.shadow_blur {
        parts.push("blur".to_owned());
    }
    if parts.is_empty() {
        parts.push("composition".to_owned());
    }
    parts.join(", ")
}
