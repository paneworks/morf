//! Clipped pictures must not perform source I/O or decode work. A picture
//! moving back into view still renders through the same cache and clip.
use super::*;
use crate::{DrawList, tests::NoText};
use morf_layout::{Layout, Size};
use morf_scene::{Element, Scene};

const RED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#ff0000"/></svg>"##;

#[test]
#[ignore = "requires a GPU adapter"]
fn a_transformed_clipped_image_is_not_loaded_until_it_can_draw() {
    // This explicit gate requires an actual adapter; a missing one is a
    // failure when this test is selected, never a reported passing skip.
    let mut backend = pollster::block_on(WgpuBackend::new(64, 64)).unwrap();
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let clip = scene.create(Element::ClipRect);
    scene.assign(clip, "width", 32.0).unwrap();
    scene.assign(clip, "height", 32.0).unwrap();
    scene.reparent(clip, Some(root)).unwrap();
    let group = scene.create(Element::Item);
    scene.assign(group, "translate_x", 64.0).unwrap();
    scene.reparent(group, Some(clip)).unwrap();
    let image = scene.create(Element::Image);
    scene.assign(image, "width", 16.0).unwrap();
    scene.assign(image, "height", 16.0).unwrap();
    let invalid = "data:image/svg+xml,not-an-svg";
    scene.assign(image, "source", invalid).unwrap();
    scene.reparent(image, Some(group)).unwrap();
    let list = |scene: &Scene| {
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
        DrawList::from_scene(scene, &layout).unwrap()
    };
    let clipped = list(&scene);
    assert!(clipped.commands.iter().any(
        |command| matches!(command,crate::DrawCommand::Texture {source,..} if source==invalid)
    ));
    super::field_tests::read_frame(&mut backend, &clipped, 64);
    assert!(
        backend.images.failure(invalid).is_none(),
        "a fully clipped source was loaded"
    );
    assert!(backend.image_textures.is_empty());

    scene.assign(group, "translate_x", 0.0).unwrap();
    scene.assign(image, "x", 24.0).unwrap();
    scene.assign(image, "source", RED).unwrap();
    let visible = list(&scene);
    let pixels = super::field_tests::read_frame(&mut backend, &visible, 64);
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4];
    assert_eq!(
        pixel(28, 8),
        [255, 0, 0, 255],
        "the visible part of the picture"
    );
    assert_eq!(pixel(36, 8)[3], 0, "ancestor clipping still applies");
    assert!(!backend.image_textures.is_empty());
}
