//! Positioners pack only what they can show.

use super::*;

fn rect(scene: &mut Scene, parent: NodeHandle, width: f64, height: f64) -> NodeHandle {
    let node = scene.create(Element::Rect);
    scene.assign(node, "width", width).unwrap();
    scene.assign(node, "height", height).unwrap();
    scene.reparent(node, Some(parent)).unwrap();
    node
}

fn compute(scene: &Scene, root: NodeHandle) -> Layout {
    Layout::compute(
        scene,
        root,
        Size {
            width: 200.0,
            height: 200.0,
        },
        &mut FixedText,
    )
    .unwrap()
}

#[test]
fn a_row_gives_a_hidden_child_no_room_and_no_gap() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let row = scene.create(Element::Row);
    scene.reparent(row, Some(root)).unwrap();
    scene.assign(row, "gap", 10.0).unwrap();
    let a = rect(&mut scene, row, 20.0, 10.0);
    let hidden = rect(&mut scene, row, 30.0, 50.0);
    let c = rect(&mut scene, row, 20.0, 10.0);
    scene.assign(hidden, "visible", false).unwrap();

    let layout = compute(&scene, root);
    assert_eq!(layout.geometry(a).unwrap().x, 0.0);
    assert_eq!(layout.geometry(c).unwrap().x, 30.0);
    // Neither its width nor its height counts toward the row's.
    assert_eq!(
        layout.implicit_size(row),
        Some(Size {
            width: 50.0,
            height: 10.0,
        })
    );

    scene.assign(hidden, "visible", true).unwrap();
    let layout = compute(&scene, root);
    assert_eq!(layout.geometry(c).unwrap().x, 70.0);
    assert_eq!(layout.implicit_size(row).unwrap().width, 90.0);
}

#[test]
fn a_column_justifies_only_what_it_shows() {
    let mut scene = Scene::new();
    let column = scene.create(Element::Column);
    scene.assign(column, "justify", "space_between").unwrap();
    let a = rect(&mut scene, column, 10.0, 20.0);
    let hidden = rect(&mut scene, column, 10.0, 20.0);
    let c = rect(&mut scene, column, 10.0, 20.0);
    scene.assign(hidden, "visible", false).unwrap();

    let layout = compute(&scene, column);
    assert_eq!(layout.geometry(a).unwrap().y, 0.0);
    // Two shown children, one gap: the second sits at the far end.
    assert_eq!(layout.geometry(c).unwrap().y, 180.0);
}

#[test]
fn a_column_of_hidden_children_asks_for_nothing() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let column = scene.create(Element::Column);
    scene.reparent(column, Some(root)).unwrap();
    scene.assign(column, "gap", 8.0).unwrap();
    for _ in 0..3 {
        let child = rect(&mut scene, column, 10.0, 20.0);
        scene.assign(child, "visible", false).unwrap();
    }
    let layout = compute(&scene, root);
    assert_eq!(layout.implicit_size(column), Some(Size::default()));
}

#[test]
fn a_grid_fills_its_cells_with_the_children_it_shows() {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let grid = scene.create(Element::Grid);
    scene.reparent(grid, Some(root)).unwrap();
    scene.assign(grid, "columns", 2.0).unwrap();
    scene.assign(grid, "gap", 5.0).unwrap();
    let a = rect(&mut scene, grid, 10.0, 10.0);
    let hidden = rect(&mut scene, grid, 40.0, 40.0);
    let b = rect(&mut scene, grid, 10.0, 10.0);
    let c = rect(&mut scene, grid, 10.0, 10.0);
    scene.assign(hidden, "visible", false).unwrap();

    let layout = compute(&scene, root);
    let at = |node| {
        let geometry = layout.geometry(node).unwrap();
        (geometry.x, geometry.y)
    };
    assert_eq!(at(a), (0.0, 0.0));
    assert_eq!(at(b), (15.0, 0.0));
    assert_eq!(at(c), (0.0, 15.0));
    assert_eq!(
        layout.implicit_size(grid),
        Some(Size {
            width: 25.0,
            height: 25.0,
        })
    );
}
