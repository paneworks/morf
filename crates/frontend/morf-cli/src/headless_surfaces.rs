//! Which surfaces a headless configuration has, and how big each is.
//!
//! Split from `headless` at the line gate. A compositor decides a surface's
//! size from its anchors, its margins and the output under it; with no
//! compositor this file decides the same way.

use std::collections::{HashMap, HashSet};

use morf_lua::WindowSurfaceKind;
use morf_scene::NodeHandle;
use morf_app::backend::headless::{self, layer_extent as stretched, layer_position};
use morf_app::{LayerAnchors, PRIMARY_LAYER, ShellLayer, WindowId};

use crate::headless::{Headless, Surface};
use crate::surface_popups::window_surface_effectively_visible;
use crate::surfaces::primary_surface_root;

impl Headless {
    /// Works out the surfaces the configuration has asked for, and their
    /// sizes on this screen.
    pub(crate) fn refresh_surfaces(&mut self) {
        if self.outputless {
            // With no output nothing is mapped, whatever is declared.
            self.surfaces.clear();
            return;
        }
        let mut previous = std::mem::take(&mut self.surfaces)
            .into_iter()
            .map(|surface| (surface.role, surface))
            .collect::<HashMap<_, _>>();
        let mut surfaces = Vec::new();
        match primary_surface_root(&self.runtime) {
            Ok(root) => {
                let config = self.runtime.layer_surface_config();
                let fallback = self.root_size(root);
                let size = (
                    stretched(
                        config.anchors.left,
                        config.anchors.right,
                        config.width,
                        self.screen.0,
                        fallback.0,
                    ),
                    stretched(
                        config.anchors.top,
                        config.anchors.bottom,
                        config.height,
                        self.screen.1,
                        fallback.1,
                    ),
                );
                surfaces.push(Surface {
                    role: WindowId::Layer(PRIMARY_LAYER),
                    kind: "primary",
                    name: config.namespace.clone(),
                    id: None,
                    root,
                    size,
                    position: placed(&config, size, self.screen),
                    visible: true,
                    stack: layer_stack(&config.layer),
                    blend: config.blend.clone(),
                    layout: None,
                    laid: None,
                    stable: true,
                    label: String::new(),
                });
            }
            Err(error) => {
                if !self.problems.contains(&error) {
                    self.problems.push(error);
                }
            }
        }
        let configs = self.runtime.window_surface_configs();
        let by_id = configs
            .iter()
            .map(|surface| (surface.id, surface))
            .collect::<HashMap<_, _>>();
        for surface in &configs {
            let visible =
                window_surface_effectively_visible(surface.id, &by_id, &mut HashSet::new());
            let fallback = self.root_size(surface.root);
            let stack = match &surface.kind {
                WindowSurfaceKind::Layer(config) => layer_stack(&config.layer),
                // A popup or window is above every layer but the overlay.
                _ => 2,
            };
            let (role, kind, name, size, position, blend) = match &surface.kind {
                WindowSurfaceKind::Popup(config) => (
                    WindowId::Popup(surface.id),
                    "popup",
                    String::new(),
                    (
                        nonzero(config.width, fallback.0),
                        nonzero(config.height, fallback.1),
                    ),
                    (
                        config.anchor_x + config.offset_x,
                        config.anchor_y + config.offset_y,
                    ),
                    config.blend.clone(),
                ),
                WindowSurfaceKind::Toplevel(config) => (
                    WindowId::Toplevel(surface.id),
                    "toplevel",
                    config.title.clone(),
                    (
                        nonzero(config.width, fallback.0),
                        nonzero(config.height, fallback.1),
                    ),
                    (0, 0),
                    config.blend.clone(),
                ),
                WindowSurfaceKind::Layer(config) => {
                    let size = (
                        stretched(
                            config.anchors.left,
                            config.anchors.right,
                            config.width,
                            self.screen.0,
                            fallback.0,
                        ),
                        stretched(
                            config.anchors.top,
                            config.anchors.bottom,
                            config.height,
                            self.screen.1,
                            fallback.1,
                        ),
                    );
                    (
                        // The shell numbers its extra layers one past their id.
                        WindowId::Layer(surface.id + 1),
                        "layer",
                        config.namespace.clone(),
                        size,
                        placed(config, size, self.screen),
                        config.blend.clone(),
                    )
                }
            };
            surfaces.push(Surface {
                role,
                kind,
                name,
                id: Some(surface.id),
                root: surface.root,
                size,
                position,
                visible,
                stack,
                blend,
                layout: None,
                laid: None,
                stable: true,
                label: String::new(),
            });
        }
        // What the last frame laid out is kept for a surface that is still
        // the same tree, so an unchanged one is not laid out again.
        for surface in &mut surfaces {
            if let Some(old) = previous.remove(&surface.role)
                && old.root == surface.root
            {
                surface.layout = old.layout;
                surface.laid = old.laid;
                surface.stable = old.stable;
            }
        }
        // Labels, made unique: two layers may share a namespace.
        let plain = surfaces
            .iter()
            .map(Surface::plain_label)
            .collect::<Vec<_>>();
        for (index, surface) in surfaces.iter_mut().enumerate() {
            let shared = plain.iter().filter(|label| **label == plain[index]).count() > 1;
            surface.label = match (shared, surface.id) {
                (true, Some(id)) => format!("{}#{id}", plain[index]),
                _ => plain[index].clone(),
            };
        }
        self.surfaces = surfaces;
    }

    /// The size a root asks for itself, for a surface whose settings leave
    /// it to the content: its `width` and `height`, or the screen's.
    pub(crate) fn root_size(&self, root: NodeHandle) -> (u32, u32) {
        let scene = self.runtime.scene();
        let read = |name: &str, screen: u32| {
            scene
                .number(root, name)
                .ok()
                .filter(|value| value.is_finite() && *value >= 1.0)
                .map_or(screen, |value| value.round().min(16_384.0) as u32)
        };
        (read("width", self.screen.0), read("height", self.screen.1))
    }

    /// Where in [`Headless::surfaces`] the surface `wanted` names is: an
    /// index, `primary`, a kind, a name or a label; the first when nothing
    /// is named.
    pub(crate) fn surface_index(&self, wanted: Option<&str>) -> Result<usize, String> {
        let Some(wanted) = wanted else {
            return if self.surfaces.is_empty() {
                Err("the configuration has no surface".to_owned())
            } else {
                Ok(0)
            };
        };
        if let Ok(index) = wanted.parse::<usize>() {
            return (index < self.surfaces.len())
                .then_some(index)
                .ok_or_else(|| {
                    format!(
                        "there is no surface {index}; there are {}",
                        self.surfaces.len()
                    )
                });
        }
        self.surfaces
            .iter()
            .position(|surface| surface.is_named(wanted))
            .ok_or_else(|| {
                format!(
                    "no surface is called `{wanted}`; there are: {}",
                    self.surfaces
                        .iter()
                        .map(Surface::label)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
    }
}

fn nonzero(asked: u32, fallback: u32) -> u32 {
    if asked == 0 { fallback.max(1) } else { asked }
}

/// Where a layer surface of `size` sits on a screen (the headless backend's
/// arithmetic, from the configuration's anchors and margins).
fn placed(
    config: &morf_lua::LayerSurfaceConfig,
    size: (u32, u32),
    screen: (u32, u32),
) -> (i32, i32) {
    let anchors = LayerAnchors {
        top: config.anchors.top,
        right: config.anchors.right,
        bottom: config.anchors.bottom,
        left: config.anchors.left,
    };
    let margins = (
        config.margin_top,
        config.margin_right,
        config.margin_bottom,
        config.margin_left,
    );
    layer_position(anchors, margins, size, screen)
}

/// A layer-shell layer's place in the stack, bottom first; an unknown name
/// is `top`, the layer a surface gets when it names none.
pub(crate) fn layer_stack(layer: &str) -> u8 {
    headless::layer_stack(match layer {
        "background" => ShellLayer::Background,
        "bottom" => ShellLayer::Bottom,
        "overlay" => ShellLayer::Overlay,
        _ => ShellLayer::Top,
    })
}

#[cfg(test)]
mod tests {
    use super::layer_stack;

    #[test]
    fn surfaces_compose_bottom_layer_first_and_keep_their_order_within_one() {
        // Declared order: the shell's own surface (top), then a wallpaper
        // (background), a dock (top), a lock curtain (overlay), a desk (bottom).
        let declared = ["top", "background", "top", "overlay", "bottom"];
        let mut order = (0..declared.len()).collect::<Vec<_>>();
        order.sort_by_key(|index| layer_stack(declared[*index]));
        assert_eq!(order, vec![1, 4, 0, 2, 3]);
        assert_eq!(layer_stack(""), layer_stack("top"));
    }
}
