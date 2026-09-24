//! A drawing written into the source string, drawn both ways a source is.

use morf_layout::{Layout, Size};
use morf_scene::{Element, Scene};

use super::field_tests::read_frame;
use super::*;
use crate::tests::NoText;
use crate::*;

/// A solid red square with a 16-unit viewport, as text.
const RED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#ff0000"/></svg>"##;

fn frame(scene: &Scene, root: morf_scene::NodeHandle) -> Vec<u8> {
    let layout = Layout::compute(
        scene,
        root,
        Size {
            width: 64.0,
            height: 64.0,
        },
        &mut NoText,
    )
    .unwrap();
    let list = DrawList::from_scene(scene, &layout).unwrap();
    let mut backend = pollster::block_on(WgpuBackend::new(64, 64)).unwrap();
    read_frame(&mut backend, &list, 64)
}

fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    let offset = (y * 64 + x) * 4;
    pixels[offset..offset + 4].try_into().unwrap()
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn an_image_draws_svg_written_inline_and_as_a_data_uri() {
    for source in [
        RED.to_owned(),
        format!("data:image/svg+xml,{}", RED.replace('#', "%23")),
    ] {
        let mut scene = Scene::new();
        let root = scene.create(Element::Item);
        scene.assign(root, "width", 64.0).unwrap();
        scene.assign(root, "height", 64.0).unwrap();
        let image = scene.create(Element::Image);
        scene.assign(image, "width", 32.0).unwrap();
        scene.assign(image, "height", 32.0).unwrap();
        scene.assign(image, "source", source.as_str()).unwrap();
        scene.reparent(image, Some(root)).unwrap();
        let pixels = frame(&scene, root);
        assert_eq!(at(&pixels, 16, 16), [255, 0, 0, 255], "inside the image");
        assert_eq!(at(&pixels, 48, 48)[3], 0, "and nothing outside it");
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_field_shape_takes_its_outline_from_svg_written_inline() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Sdf);
    scene.assign(root, "width", 64.0).unwrap();
    scene.assign(root, "height", 64.0).unwrap();
    let shape = scene.create(Element::SdfShape);
    scene.assign(shape, "width", 40.0).unwrap();
    scene.assign(shape, "height", 40.0).unwrap();
    scene.assign(shape, "source", RED).unwrap();
    scene.reparent(shape, Some(root)).unwrap();
    let pixels = frame(&scene, root);
    assert!(at(&pixels, 20, 20)[3] > 200, "the square is filled");
    assert_eq!(at(&pixels, 60, 60)[3], 0, "and ends where it does");
}
