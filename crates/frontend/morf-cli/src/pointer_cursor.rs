//! The pointer takes the shape of the area it is over.

use morf_lua::Runtime;
use morf_scene::NodeHandle;
use morf_wayland::{LayerClient, SurfaceRole};

/// Whatever takes the pointer's shape: the compositor connection, or in a
/// test a recorder, so the input path runs without a display.
pub(crate) trait CursorShapes {
    fn set_cursor_shape(&mut self, shape: &str);
}

impl CursorShapes for LayerClient {
    fn set_cursor_shape(&mut self, shape: &str) {
        LayerClient::set_cursor_shape(self, shape);
    }
}

/// Re-asks for the cursor when the pointer moves from one node to another.
pub(crate) fn hover_changed(
    runtime: &Runtime,
    client: &mut dyn CursorShapes,
    entered: Option<(SurfaceRole, NodeHandle)>,
    left: Option<(SurfaceRole, NodeHandle)>,
) {
    if entered != left {
        follow_hovered_cursor(runtime, client, entered.map(|(_, node)| node));
    }
}

/// Asks for the hovered area's `cursor`, or the default when nothing under
/// the pointer names one.
pub(crate) fn follow_hovered_cursor(
    runtime: &Runtime,
    client: &mut dyn CursorShapes,
    hovered: Option<NodeHandle>,
) {
    let shape = hovered
        .and_then(|node| {
            runtime
                .scene()
                .string_value(node, "cursor")
                .ok()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "default".to_owned());
    client.set_cursor_shape(&shape);
}
