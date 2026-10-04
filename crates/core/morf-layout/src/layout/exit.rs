//! Nodes on their way out: the box each keeps, and placing it there.

use morf_scene::{NodeHandle, Scene};

use crate::custom::CustomLayout;
use crate::geometry::{Geometry, TextMeasurer};
use crate::helpers::LayoutError;
use crate::incremental::Local;

use super::Layout;

impl Layout {
    pub(crate) fn capture_exit_frames(&self, scene: &Scene) {
        for node in scene.exiting_nodes() {
            if scene.exit_frame(node).is_some() {
                continue;
            }
            let Some(parent) = scene.parent(node).ok().flatten() else {
                continue;
            };
            let (Some(own), Some(around)) = (self.geometry.get(&node), self.geometry.get(&parent))
            else {
                continue;
            };
            scene.fix_exit_frame(
                node,
                [own.x - around.x, own.y - around.y, own.width, own.height],
            );
        }
    }

    /// Places a node on its way out: in the box it keeps relative to its
    /// parent, whatever its parent's rules would do with it, taking no room
    /// from its siblings. See [`morf_scene::Scene::begin_exit`].
    pub(crate) fn place_exiting(
        &mut self,
        scene: &Scene,
        parent: NodeHandle,
        child: NodeHandle,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let frame = match scene.exit_frame(child) {
            Some(frame) => frame,
            None => {
                // Never placed before it started to leave: where its own
                // `x` and `y` put it, at the size it asks for.
                let size = self.requested.get(&child).copied().unwrap_or_default();
                scene.fix_exit_frame(
                    child,
                    [
                        scene.number(child, "x")?,
                        scene.number(child, "y")?,
                        size.width,
                        size.height,
                    ],
                );
                scene.exit_frame(child).unwrap_or_default()
            }
        };
        let local = Local::Placed {
            x: frame[0],
            y: frame[1],
            inset: None,
            scrolled: None,
            transition: (0.0, 0.0),
        };
        let (x, y) = local.position(self.geometry[&parent]);
        self.local.insert(child, local);
        let geometry = Geometry {
            x,
            y,
            width: frame[2],
            height: frame[3],
        };
        self.place(scene, child, geometry, text, host)
    }
}
