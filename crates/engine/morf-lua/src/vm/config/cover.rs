//! The engine-owned cover: a picture of what a surface showed, laid over
//! it while the surface turns or is rebuilt, in a tree of the engine's own
//! that no configuration can reach.

use super::*;
use morf_layout::{Layout, Size, TextMeasurer};
use morf_scene::{Element, Value};

/// How the cover's picture stands on the surface, centred on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverPose {
    /// Its size in logical pixels, before `scale`.
    pub size: (f64, f64),
    /// Clockwise, in degrees, about its centre.
    pub rotation: f64,
    pub scale: f64,
    pub opacity: f64,
}

/// The cover as a surface laid it out.
#[derive(Clone, Debug)]
pub struct CoverLayout {
    pub root: NodeHandle,
    pub layout: Layout,
}

/// The cover's nodes: a surface-sized root and the picture on it.
pub(crate) struct Cover {
    root: NodeHandle,
    picture: NodeHandle,
}

impl Runtime {
    /// Lays the picture `source` over a surface `surface` logical pixels
    /// big, posed as `pose`, and returns the cover laid out. A cover that
    /// cannot be laid out is taken down and its error logged.
    pub fn pose_cover(
        &mut self,
        source: &str,
        pose: CoverPose,
        surface: (u32, u32),
        text: &mut impl TextMeasurer,
    ) -> Option<CoverLayout> {
        let (root, picture) = self.cover_nodes();
        let (width, height) = (f64::from(surface.0), f64::from(surface.1));
        let settings = [
            (root, "width", Value::Number(width)),
            (root, "height", Value::Number(height)),
            (picture, "source", Value::String(source.to_owned())),
            (picture, "x", Value::Number((width - pose.size.0) / 2.0)),
            (picture, "y", Value::Number((height - pose.size.1) / 2.0)),
            (picture, "width", Value::Number(pose.size.0)),
            (picture, "height", Value::Number(pose.size.1)),
            (picture, "rotation", Value::Number(pose.rotation)),
            (picture, "scale", Value::Number(pose.scale)),
            (
                picture,
                "opacity",
                Value::Number(pose.opacity.clamp(0.0, 1.0)),
            ),
        ];
        let mut state = self.reactive.borrow_mut();
        for (node, property, value) in settings {
            if let Err(message) =
                crate::api::ui::scene::assign_scene_property(&mut state, node, property, value)
            {
                state.log(LogLevel::Warn, format!("cover: {property}: {message}"));
            }
        }
        drop(state);
        let available = Size { width, height };
        match self.compute_layout(root, available, text) {
            Ok(layout) => Some(CoverLayout { root, layout }),
            Err(message) => {
                self.clear_cover();
                self.warn(format!("the cover could not be laid out: {message}"));
                None
            }
        }
    }

    /// Takes the cover down, if it is up.
    pub fn clear_cover(&mut self) {
        let cover = self.cover.borrow_mut().take();
        if let Some(cover) = cover
            && self.scene().contains(cover.root)
        {
            let _ = self.scene_mut().remove(cover.root);
        }
    }

    /// The cover's root while it is up.
    pub fn cover_root(&self) -> Option<NodeHandle> {
        let root = self.cover.borrow().as_ref()?.root;
        self.scene().contains(root).then_some(root)
    }

    fn cover_nodes(&mut self) -> (NodeHandle, NodeHandle) {
        if let Some(cover) = self.cover.borrow().as_ref()
            && self.scene().contains(cover.root)
        {
            return (cover.root, cover.picture);
        }
        let state = &self.reactive;
        let create = |element| crate::api::ui::scene::create_node(state, element);
        let (root, picture) = (create(Element::Item), create(Element::Image));
        let mut state = state.borrow_mut();
        for (node, id) in [(root, "morf-cover"), (picture, "morf-cover-picture")] {
            let id = Value::String(id.to_owned());
            if let Err(message) =
                crate::api::ui::scene::assign_scene_property(&mut state, node, "id", id)
            {
                state.log(LogLevel::Warn, format!("cover: {message}"));
            }
        }
        if let Err(message) = state.scene_mut().reparent(picture, Some(root)) {
            state.log(LogLevel::Warn, format!("cover: {message}"));
        }
        drop(state);
        *self.cover.borrow_mut() = Some(Cover { root, picture });
        (root, picture)
    }
}
