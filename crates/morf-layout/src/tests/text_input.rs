use super::*;

/// A field inside a root, so it is sized by what it measures rather than by
/// the room a root is given.
fn field(scene: &mut Scene) -> (NodeHandle, NodeHandle) {
    let root = scene.create(Element::Item);
    let input = scene.create(Element::TextInput);
    scene.reparent(input, Some(root)).unwrap();
    (root, input)
}

#[test]
fn an_empty_field_is_still_a_line_tall_and_a_caret_wide() {
    let mut scene = Scene::new();
    let (root, input) = field(&mut scene);
    scene.assign(input, "font_size", 20.0).unwrap();
    let layout = Layout::compute(&scene, root, Size::default(), &mut FixedText).unwrap();
    let geometry = layout.geometry(input).unwrap();
    // FixedText measures nothing as nothing; one line is 1.2 times the size.
    assert_eq!(geometry.height, 24.0);
    assert_eq!(geometry.width, 2.0);
}

#[test]
fn a_field_measures_what_it_shows() {
    let mut scene = Scene::new();
    let (root, input) = field(&mut scene);
    scene.assign(input, "font_size", 10.0).unwrap();
    scene.assign(input, "placeholder", "Search").unwrap();
    scene.assign(input, "caret_width", 0.0).unwrap();
    let layout = Layout::compute(&scene, root, Size::default(), &mut FixedText).unwrap();
    // The placeholder while empty: six letters at five pixels.
    assert_eq!(layout.geometry(input).unwrap().width, 30.0);

    scene.assign(input, "text", "añ").unwrap();
    scene.assign(input, "password", true).unwrap();
    scene.assign(input, "password_char", "*").unwrap();
    let layout = Layout::compute(&scene, root, Size::default(), &mut FixedText).unwrap();
    // Two dots, however many bytes the letters under them take.
    assert_eq!(layout.geometry(input).unwrap().width, 10.0);
}

#[test]
fn a_wrapping_field_grows_as_tall_as_its_lines() {
    let mut scene = Scene::new();
    let column = scene.create(Element::Column);
    scene.assign(column, "width", 50.0).unwrap();
    let input = scene.create(Element::TextInput);
    scene.reparent(input, Some(column)).unwrap();
    scene.assign(input, "font_size", 10.0).unwrap();
    scene.assign(input, "multiline", true).unwrap();
    scene.assign(input, "text", "twenty letters here!").unwrap();
    scene.assign(input, "width", 50.0).unwrap();
    let layout = Layout::compute(&scene, column, Size::default(), &mut WrapText).unwrap();
    // A hundred pixels of text in fifty: two lines of ten.
    assert_eq!(layout.geometry(input).unwrap().height, 20.0);
    // A single line never wraps.
    scene.assign(input, "multiline", false).unwrap();
    let layout = Layout::compute(&scene, column, Size::default(), &mut WrapText).unwrap();
    assert_eq!(layout.geometry(input).unwrap().height, 12.0);
}

#[test]
fn the_pointer_hits_a_field_and_its_box_takes_input() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 100.0).unwrap();
    scene.assign(root, "height", 100.0).unwrap();
    let input = scene.create(Element::TextInput);
    scene.reparent(input, Some(root)).unwrap();
    scene.assign(input, "x", 10.0).unwrap();
    scene.assign(input, "y", 10.0).unwrap();
    scene.assign(input, "width", 50.0).unwrap();
    scene.assign(input, "height", 20.0).unwrap();
    let layout = Layout::compute(
        &scene,
        root,
        Size {
            width: 100.0,
            height: 100.0,
        },
        &mut FixedText,
    )
    .unwrap();
    let hit = layout.hit_test(&scene, 15.0, 12.0).unwrap().unwrap();
    assert_eq!(hit.node, input);
    assert_eq!((hit.local_x, hit.local_y), (5.0, 2.0));
    assert_eq!(layout.input_geometry(&scene).unwrap().len(), 1);
}
