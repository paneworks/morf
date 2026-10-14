//! A draw list that copies unchanged subtrees from the last frame against
//! one built afresh, on random scenes under random changes: the two must be
//! the same, command for command and layer for layer, every frame.

use super::*;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
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

fn make(scene: &mut Scene, rng: &mut Rng, depth: usize) -> NodeHandle {
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

fn mutate(scene: &mut Scene, rng: &mut Rng, root: NodeHandle) {
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

#[test]
fn a_kept_draw_list_matches_one_built_afresh() {
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    for seed in 1..=150u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut scene = Scene::new();
        let root = make(&mut scene, &mut rng, 4);
        let mut engine = RenderEngine::new(RecordingBackend::default());
        for step in 0..40 {
            if step > 0 {
                for _ in 0..1 + rng.below(3) {
                    mutate(&mut scene, &mut rng, root);
                }
            }
            if rng.chance(3) {
                engine.forget();
            }
            let Ok(layout) = Layout::compute(&scene, root, size, &mut NoText) else {
                continue;
            };
            let fresh = DrawList::from_scene(&scene, &layout);
            let kept = engine.render(&scene, &layout, 120, |_| {});
            match (fresh, kept) {
                (Ok(fresh), Ok(_)) => assert!(
                    *engine.last_list() == fresh,
                    "seed {seed}, step {step}: the kept list differs from a fresh one"
                ),
                (Err(_), Err(_)) => {}
                (fresh, kept) => panic!(
                    "seed {seed}, step {step}: fresh {:?}, kept {:?}",
                    fresh.err(),
                    kept.err()
                ),
            }
        }
    }
}

#[test]
fn an_unchanged_frame_copies_every_command() {
    let mut rng = Rng(7);
    let mut scene = Scene::new();
    let root = make(&mut scene, &mut rng, 4);
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    let layout = Layout::compute(&scene, root, size, &mut NoText).unwrap();
    let mut engine = RenderEngine::new(RecordingBackend::default());
    engine.render(&scene, &layout, 120, |_| {}).unwrap();
    let first = engine.last_list().clone();
    let damage = engine.render(&scene, &layout, 120, |_| {}).unwrap();
    assert!(damage.is_empty());
    assert!(*engine.last_list() == first);
}
