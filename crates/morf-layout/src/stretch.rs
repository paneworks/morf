//! Where stretching nodes were drawn, for their springs.
//!
//! The scene keeps the springs (see `morf_scene::Stretch`); only a layout
//! knows where a node actually is. After a frame is laid out and before it is
//! painted, every stretching node in it reports its centre, and its spring
//! steps from the motion between that and the last report. Its deformation is
//! then part of its transform for the paint that follows.

use morf_scene::Scene;

use crate::geometry::Transform2D;
use crate::helpers::LayoutError;
use crate::layout::Layout;
use crate::transform::node_transform_unstretched;

/// Reports every stretching node laid out in `layout` to its spring.
/// Returns whether any deformation changed.
///
/// Free when nothing stretches: one emptiness check.
pub fn observe_stretch(scene: &mut Scene, layout: &Layout) -> Result<bool, LayoutError> {
    if !scene.has_stretch() {
        return Ok(false);
    }
    let mut changed = false;
    for node in scene.stretch_nodes() {
        let Some(geometry) = layout.geometry(node) else {
            continue;
        };
        let parent = match scene.parent(node)? {
            Some(parent) => layout.chain_transform(scene, parent)?,
            None => Transform2D::IDENTITY,
        };
        let own = node_transform_unstretched(scene, node, geometry)?;
        let centre = parent.then(own).point(
            geometry.x + geometry.width / 2.0,
            geometry.y + geometry.height / 2.0,
        );
        // The deformation is applied in the parent's frame, so the velocity
        // is turned into it: a stretch inside a rotated panel still runs
        // along the way the node is moving on screen.
        let to_local = parent.linear_inverse().unwrap_or([1.0, 0.0, 0.0, 1.0]);
        changed |= scene.observe_stretch(node, [centre.0, centre.1], to_local);
    }
    Ok(changed)
}
