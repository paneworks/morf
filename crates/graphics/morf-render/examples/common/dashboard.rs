//! The dashboard the frame benches paint: cards of rows, each an icon, a
//! label and a badge, with every fifth card faded.

use morf_scene::{Color, Element, NodeHandle, Scene, Value};

pub struct Dashboard {
    pub scene: Scene,
    pub root: NodeHandle,
    pub badges: Vec<NodeHandle>,
    pub labels: Vec<NodeHandle>,
}

pub fn dashboard(panels: usize) -> Dashboard {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", 1920.0).unwrap();
    scene.assign(root, "height", 1080.0).unwrap();
    let (mut badges, mut labels) = (Vec::new(), Vec::new());
    for panel in 0..panels {
        let card = scene.create(Element::Rect);
        scene.assign(card, "x", (panel % 8) as f64 * 236.0).unwrap();
        scene.assign(card, "y", (panel / 8) as f64 * 200.0).unwrap();
        scene.assign(card, "width", 220.0).unwrap();
        scene.assign(card, "height", 180.0).unwrap();
        scene.assign(card, "radius", 12.0).unwrap();
        scene
            .assign(card, "color", Value::Color(Color::rgba8(30, 30, 40, 255)))
            .unwrap();
        if panel % 5 == 0 {
            scene.assign(card, "opacity", 0.9).unwrap();
        }
        scene.reparent(card, Some(root)).unwrap();
        let column = scene.create(Element::Column);
        scene.assign(column, "gap", 6.0).unwrap();
        scene.reparent(column, Some(card)).unwrap();
        let title = scene.create(Element::Text);
        scene
            .assign(title, "text", format!("Panel {panel}"))
            .unwrap();
        scene.reparent(title, Some(column)).unwrap();
        for item in 0..5 {
            let row = scene.create(Element::Row);
            scene.assign(row, "gap", 4.0).unwrap();
            scene.reparent(row, Some(column)).unwrap();
            let icon = scene.create(Element::Rect);
            scene.assign(icon, "width", 16.0).unwrap();
            scene.assign(icon, "height", 16.0).unwrap();
            scene.assign(icon, "radius", 4.0).unwrap();
            scene.reparent(icon, Some(row)).unwrap();
            let label = scene.create(Element::Text);
            scene.assign(label, "text", format!("item {item}")).unwrap();
            scene.reparent(label, Some(row)).unwrap();
            let badge = scene.create(Element::Rect);
            scene.assign(badge, "width", 24.0).unwrap();
            scene.assign(badge, "height", 12.0).unwrap();
            scene.reparent(badge, Some(row)).unwrap();
            badges.push(badge);
            labels.push(label);
        }
    }
    Dashboard {
        scene,
        root,
        badges,
        labels,
    }
}

/// Changes one badge's colour on even frames and one label's text on odd.
pub fn change(dashboard: &mut Dashboard, frame: usize) {
    if frame % 2 == 0 {
        let badge = dashboard.badges[frame * 7 % dashboard.badges.len()];
        let shade = (frame % 200) as u8;
        dashboard
            .scene
            .assign(
                badge,
                "color",
                Value::Color(Color::rgba8(shade, 100, 200, 255)),
            )
            .unwrap();
    } else {
        let label = dashboard.labels[frame * 3 % dashboard.labels.len()];
        dashboard
            .scene
            .assign(label, "text", format!("item {}", frame % 9))
            .unwrap();
    }
}
