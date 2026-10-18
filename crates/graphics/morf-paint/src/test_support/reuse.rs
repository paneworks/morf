//! Random scenes, and random changes to them, for the draw-list reuse tests
//! here and in morf-render.

use morf_scene::{Element, NodeHandle, Scene, Value};

pub struct Rng(pub u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

const KINDS: [Element; 9] = [
    Element::Item,
    Element::Rect,
    Element::ClipRect,
    Element::Text,
    Element::Row,
    Element::Column,
    Element::Sdf,
    Element::SdfShape,
    Element::Rect,
];

const COLORS: [&str; 4] = ["#ff0000ff", "#00ff0080", "#0000ffff", "#ffffff40"];

/// A line under the text.
pub fn underline() -> Value {
    Value::Map(
        [("line".to_owned(), Value::String("under".to_owned()))]
            .into_iter()
            .collect(),
    )
}

pub fn make(scene: &mut Scene, rng: &mut Rng, depth: usize) -> NodeHandle {
    let element = KINDS[rng.below(KINDS.len())];
    let node = scene.create(element);
    let _ = scene.assign(node, "x", rng.below(40) as f64);
    let _ = scene.assign(node, "y", rng.below(40) as f64);
    let _ = scene.assign(node, "width", 10.0 + rng.below(60) as f64);
    let _ = scene.assign(node, "height", 10.0 + rng.below(40) as f64);
    let _ = scene.assign(node, "color", COLORS[rng.below(COLORS.len())]);
    if element == Element::Text {
        let _ = scene.assign(node, "text", ["a", "bc", "def"][rng.below(3)]);
        let _ = scene.assign(node, "horizontal_alignment", "left");
        if rng.chance(30) {
            scene.assign(node, "decoration", underline()).unwrap();
        }
    }
    if element == Element::ClipRect && rng.chance(50) {
        let _ = scene.assign(node, "border_width", 2.0);
    }
    if rng.chance(20) {
        let _ = scene.assign(node, "opacity", 0.5);
    }
    if depth > 0 {
        for _ in 0..rng.below(4) {
            let child = make(scene, rng, depth - 1);
            scene.reparent(child, Some(node)).unwrap();
        }
    }
    node
}

fn nodes(scene: &Scene, root: NodeHandle) -> Vec<NodeHandle> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        out.push(node);
        stack.extend(scene.children(node).unwrap().iter().copied());
    }
    out
}

pub fn mutate(scene: &mut Scene, rng: &mut Rng, root: NodeHandle) {
    let all = nodes(scene, root);
    let node = all[rng.below(all.len())];
    match rng.below(14) {
        0 | 1 => {
            let _ = scene.assign(node, "color", COLORS[rng.below(COLORS.len())]);
        }
        2 => {
            let _ = scene.assign(node, "width", 5.0 + rng.below(80) as f64);
        }
        3 => {
            let _ = scene.assign(node, "opacity", [1.0, 0.5, 0.0][rng.below(3)]);
        }
        4 => {
            let _ = scene.assign(node, "rotation", [0.0, 15.0][rng.below(2)]);
        }
        5 if node != root => {
            let visible = !scene.bool_value(node, "visible").unwrap();
            scene.assign(node, "visible", visible).unwrap();
        }
        6 => {
            let _ = scene.assign(node, "text", ["x", "yz", ""][rng.below(3)]);
        }
        7 if node != root => {
            let child = make(scene, rng, 1);
            scene.reparent(child, Some(node)).unwrap();
        }
        8 if node != root => scene.remove(node).unwrap(),
        9 => {
            let mut order = scene.children(node).unwrap().to_vec();
            order.reverse();
            scene.reorder_children(node, &order).unwrap();
        }
        10 if node != root => {
            let target = all[rng.below(all.len())];
            let _ = scene.reparent(node, Some(target));
        }
        11 => {
            let _ = scene.assign(node, "z", rng.below(3) as f64);
        }
        12 => {
            let direction = ["rtl", "ltr", "inherit"][rng.below(3)];
            let _ = scene.assign(node, "layout_direction", direction);
        }
        _ if node != root && !scene.is_mask(node) => {
            if scene.mask(node).is_some() && rng.chance(50) {
                scene.set_mask(node, None).unwrap();
            } else {
                let mask = make(scene, rng, 1);
                scene.set_mask(node, Some(mask)).unwrap();
            }
        }
        _ => {
            let _ = scene.assign(node, "clip", rng.chance(50));
        }
    }
}
