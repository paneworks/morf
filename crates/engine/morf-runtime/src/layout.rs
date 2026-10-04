//! What a frame's layout tells the runtime: which nodes a layout-reading
//! binding watches have moved, where a popup anchored to a node now opens,
//! and whether two nodes' relative transform changed.

use std::collections::HashMap;

use morf_layout::{Geometry, Layout, TransformTracker, TransformWatcher};
use morf_scene::{NodeHandle, Scene};

use crate::Handler;
use crate::windows::{PopupNodeAnchor, WindowSurfaceConfig, WindowSurfaceKind};

/// The pseudo-property a binding depends on when it reads `layout_x` or
/// `layout_y`.
pub const LAYOUT_POSITION: &str = "layout_position";
/// The pseudo-property a binding depends on when it reads `layout_width` or
/// `layout_height`.
///
/// Apart from the position because the two change apart. A label in a panel
/// that is centred in a growing island moves on every frame of the morph and
/// keeps its size; a binding that only sizes something to it (a dot under
/// it) has nothing to recompute.
pub const LAYOUT_SIZE: &str = "layout_size";

/// A layout that has stopped moving, and how many passes it took.
pub struct SettledLayout {
    pub layout: Layout,
    /// Layout passes run, one or more.
    pub passes: usize,
    /// Whether the last pass changed nothing a layout reads. False when the
    /// passes ran out first: a binding that feeds its own geometry back.
    pub stable: bool,
}

/// A layout coordinate as a whole pixel, clamped.
pub fn geometry_i32(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// Of the layout pseudo-properties bindings read (`(node, property)`),
/// those whose geometry in `layout` differs from what `tracker` last saw.
pub fn moved_nodes<'a>(
    read: impl Iterator<Item = (NodeHandle, &'a str)>,
    layout: &Layout,
    tracker: &TransformTracker,
) -> Vec<(NodeHandle, &'static str)> {
    read.filter_map(|(node, property)| {
        let size = match property {
            LAYOUT_SIZE => true,
            LAYOUT_POSITION => false,
            _ => return None,
        };
        let now = layout.geometry(node)?;
        let changed = match tracker.geometry(node) {
            None => true,
            Some(before) if size => before.width != now.width || before.height != now.height,
            Some(before) => before.x != now.x || before.y != now.y,
        };
        changed.then_some((node, if size { LAYOUT_SIZE } else { LAYOUT_POSITION }))
    })
    .collect()
}

impl PopupNodeAnchor {
    /// The anchor rectangle `(x, y, width, height)` against a node at
    /// `geometry`.
    pub fn resolve(&self, geometry: &Geometry) -> (i32, i32, i32, i32) {
        let node_width = geometry_i32(geometry.width).max(1);
        let node_height = geometry_i32(geometry.height).max(1);
        (
            geometry_i32(geometry.x)
                .saturating_add(self.x)
                .saturating_sub(self.margin_left),
            geometry_i32(geometry.y)
                .saturating_add(self.y)
                .saturating_sub(self.margin_top),
            self.width
                .unwrap_or(node_width)
                .saturating_add(self.margin_left)
                .saturating_add(self.margin_right)
                .max(1),
            self.height
                .unwrap_or(node_height)
                .saturating_add(self.margin_top)
                .saturating_add(self.margin_bottom)
                .max(1),
        )
    }
}

/// Moves every node-anchored popup to where its node now is. Returns
/// whether any popup's anchor changed (the window surfaces owe a sync).
pub fn place_popup_anchors(
    anchors: &HashMap<u64, PopupNodeAnchor>,
    tracker: &TransformTracker,
    surfaces: &mut HashMap<u64, WindowSurfaceConfig>,
) -> bool {
    let mut changed = false;
    for (id, anchor) in anchors {
        let Some(geometry) = tracker.geometry(anchor.node) else {
            continue;
        };
        let resolved = anchor.resolve(&geometry);
        if let Some(WindowSurfaceConfig {
            kind: WindowSurfaceKind::Popup(config),
            ..
        }) = surfaces.get_mut(id)
            && (
                config.anchor_x,
                config.anchor_y,
                config.anchor_width,
                config.anchor_height,
            ) != resolved
        {
            config.anchor_x = resolved.0;
            config.anchor_y = resolved.1;
            config.anchor_width = resolved.2;
            config.anchor_height = resolved.3;
            changed = true;
        }
    }
    changed
}

/// `ui.watch_transform(a, b, fn)`: two nodes whose relative transform is
/// watched, and who is told when it changes.
pub struct TransformWatch {
    pub a: NodeHandle,
    pub b: NodeHandle,
    pub watcher: TransformWatcher,
    pub callback: Option<Handler>,
    /// Moves on with every change.
    pub revision: u64,
    /// A change the callback has not heard yet.
    pub pending: bool,
}

/// Observes every watch against a frame. Returns whether any changed, and
/// what could not be observed.
pub fn observe_transform_watches(
    watches: &mut HashMap<u64, TransformWatch>,
    scene: &Scene,
    tracker: &TransformTracker,
) -> (bool, Vec<String>) {
    let mut changed = false;
    let mut errors = Vec::new();
    for watch in watches.values_mut() {
        match watch.watcher.observe(scene, tracker) {
            Ok(true) => {
                watch.revision = watch.revision.wrapping_add(1);
                watch.pending = true;
                changed = true;
            }
            Ok(false) => {}
            Err(error) => errors.push(format!("transform watcher: {error}")),
        }
    }
    (changed, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_popup_anchor_takes_the_node_s_size_and_its_margins() {
        let node = morf_scene::Scene::default().create(morf_scene::Element::Item);
        let anchor = PopupNodeAnchor {
            node,
            x: 4,
            y: -2,
            width: None,
            height: Some(10),
            margin_top: 1,
            margin_right: 2,
            margin_bottom: 3,
            margin_left: 5,
        };
        let geometry = Geometry {
            x: 100.4,
            y: 20.6,
            width: 30.0,
            height: 0.2,
            ..Geometry::default()
        };
        assert_eq!(anchor.resolve(&geometry), (99, 18, 37, 14));
    }

    #[test]
    fn a_coordinate_is_rounded_and_clamped() {
        assert_eq!(geometry_i32(1.5), 2);
        assert_eq!(geometry_i32(f64::MAX), i32::MAX);
        assert_eq!(geometry_i32(f64::MIN), i32::MIN);
    }
}
