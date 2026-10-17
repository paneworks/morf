//! Scene and backend fixtures shared by the paint and GPU test suites.

use std::convert::Infallible;

use morf_layout::{Layout, Size, TextMeasurer};
use morf_scene::{Element, NodeHandle, Scene};

use crate::DrawList;
use crate::damage::{DamageRect, RenderBackend};

pub mod reuse;

pub struct NoText;

impl TextMeasurer for NoText {
    fn measure(
        &mut self,
        _node: NodeHandle,
        _text: &str,
        _family: &str,
        _size: f64,
        _options: morf_layout::TextOptions,
    ) -> Size {
        Size::default()
    }
}

#[derive(Default)]
pub struct RecordingBackend {
    pub frames: usize,
    pub damage: Vec<DamageRect>,
    pub size: (u32, u32),
}

impl RenderBackend for RecordingBackend {
    type Error = Infallible;

    fn render(
        &mut self,
        _list: &DrawList,
        damage: &[DamageRect],
        _scale_120: u32,
    ) -> Result<(), Self::Error> {
        self.frames += 1;
        self.damage = damage.to_vec();
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);
    }
}

/// A 64-square surface with a 32-square bordered ClipRect inside an ancestor
/// moved down by `translate_y = 32` (and scaled by `scale`), holding a green
/// bar that runs past its right edge: the layer a bordered clip composites
/// its contents through must follow the ancestor. Returns the bar.
pub fn translated_bordered_clip(radius: f64, scale: f64) -> (Scene, Layout, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let moved = scene.create(Element::Item);
    let clip = scene.create(Element::ClipRect);
    let bar = scene.create(Element::Rect);
    for (node, width, height) in [(moved, 32.0, 32.0), (clip, 32.0, 32.0), (bar, 64.0, 10.0)] {
        scene.assign(node, "width", width).unwrap();
        scene.assign(node, "height", height).unwrap();
    }
    scene.assign(moved, "translate_y", 32.0).unwrap();
    scene.assign(moved, "scale", scale).unwrap();
    scene.assign(clip, "color", "#0000ffff").unwrap();
    scene.assign(clip, "radius", radius).unwrap();
    scene.assign(clip, "border_width", 2.0).unwrap();
    scene.assign(clip, "border_color", "#ffffffff").unwrap();
    scene.assign(bar, "x", 4.0).unwrap();
    scene.assign(bar, "y", 4.0).unwrap();
    scene.assign(bar, "color", "#00ff00ff").unwrap();
    scene.reparent(moved, Some(root)).unwrap();
    scene.reparent(clip, Some(moved)).unwrap();
    scene.reparent(bar, Some(clip)).unwrap();
    let layout = Layout::compute(
        &scene,
        root,
        Size {
            width: 64.0,
            height: 64.0,
        },
        &mut NoText,
    )
    .unwrap();
    (scene, layout, bar)
}
