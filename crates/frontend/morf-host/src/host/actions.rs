use morf_app::Backend;
use morf_app::Edge;
use morf_layout::{Layout, ReparentTransition, Size};
use morf_lua::{Runtime, WindowSurfaceAction};
use morf_render::{RenderEngine, WgpuBackend};

use crate::host::windows::{Kind, Windows};
use crate::surfaces::*;

use morf_app::WindowId;

pub fn apply_window_surface_actions(
    runtime: &mut Runtime,
    client: &dyn Backend,
    windows: &Windows,
) {
    for action in runtime.take_window_surface_actions() {
        match action {
            WindowSurfaceAction::Move { id } if windows.contains(Kind::Toplevel, id) => {
                client.start_move(WindowId::Toplevel(id));
            }
            WindowSurfaceAction::Resize { id, edge } if windows.contains(Kind::Toplevel, id) => {
                let edge = match edge.as_str() {
                    "top" => Edge::Top,
                    "bottom" => Edge::Bottom,
                    "left" => Edge::Left,
                    "right" => Edge::Right,
                    "top_left" => Edge::TopLeft,
                    "top_right" => Edge::TopRight,
                    "bottom_left" => Edge::BottomLeft,
                    "bottom_right" => Edge::BottomRight,
                    _ => continue,
                };
                client.start_resize(WindowId::Toplevel(id), edge);
            }
            WindowSurfaceAction::Move { .. } | WindowSurfaceAction::Resize { .. } => {}
        }
    }
}

pub fn apply_parent_transitions(
    runtime: &mut Runtime,
    renderer: &mut RenderEngine<WgpuBackend>,
    client: &dyn Backend,
) -> Result<(), String> {
    let transitions = runtime.take_parent_transitions();
    if transitions.is_empty() {
        return Ok(());
    }
    let root = primary_surface_root(runtime)?;
    let (width, height) = client.primary_logical_size();
    let available = Size {
        width: width as f64,
        height: height as f64,
    };
    for transition in transitions {
        Layout::transition_reparent(
            &mut runtime.scene_mut(),
            renderer.backend_mut(),
            ReparentTransition {
                root,
                node: transition.node,
                new_parent: transition.parent,
                anchors: transition.anchors,
                available,
                behavior: transition.behavior,
            },
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}
