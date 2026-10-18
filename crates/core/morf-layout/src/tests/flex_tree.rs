//! A Taffy tree kept across passes against one built afresh, on random
//! flex and grid trees under random changes: the same geometry, and the
//! text measurer left with the same last answer for every leaf.

use super::*;
use crate::flex::FlexTree;
use morf_scene::FastMap;
use taffy::prelude::{AvailableSpace, Size as TaffySize};

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

/// Wraps at the width it is offered and remembers what it was last asked.
#[derive(Default)]
struct Recording {
    last: FastMap<NodeHandle, (String, Option<u64>)>,
}

impl TextMeasurer for Recording {
    fn measure(
        &mut self,
        node: NodeHandle,
        text: &str,
        _family: &str,
        size: f64,
        options: TextOptions,
    ) -> Size {
        self.last
            .insert(node, (text.to_owned(), options.width.map(f64::to_bits)));
        let full = text.len() as f64 * size / 2.0;
        match options.width {
            Some(width) if width > 0.0 && options.wrap => Size {
                width: full.min(width),
                height: (full / width).ceil().max(1.0) * size,
            },
            _ => Size {
                width: full,
                height: size,
            },
        }
    }
}

const WORDS: [&str; 4] = ["", "clock", "a longer line that wraps", "Wi-Fi"];

fn leaf(scene: &mut Scene, rng: &mut Rng) -> NodeHandle {
    if rng.chance(50) {
        let text = scene.create(Element::Text);
        scene.assign(text, "text", WORDS[rng.below(4)]).unwrap();
        scene.assign(text, "wrap", rng.chance(50)).unwrap();
        text
    } else {
        let rect = scene.create(Element::Rect);
        scene.assign(rect, "width", rng.below(60) as f64).unwrap();
        scene.assign(rect, "height", rng.below(40) as f64).unwrap();
        rect
    }
}

fn container(scene: &mut Scene, rng: &mut Rng, depth: usize) -> NodeHandle {
    let node = scene.create(Element::Flex);
    scene
        .assign(
            node,
            "direction",
            if rng.chance(50) { "row" } else { "column" },
        )
        .unwrap();
    scene.assign(node, "wrap", rng.chance(30)).unwrap();
    for _ in 0..1 + rng.below(4) {
        let child = if depth > 0 && rng.chance(30) {
            container(scene, rng, depth - 1)
        } else {
            leaf(scene, rng)
        };
        scene.reparent(child, Some(node)).unwrap();
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
    let element = scene.element(node).unwrap();
    let container = matches!(element, Element::Flex | Element::Grid);
    match rng.below(9) {
        0 => {
            let property = ["width", "height"][rng.below(2)];
            scene.assign(node, property, rng.below(80) as f64).unwrap();
        }
        1 if element == Element::Text => {
            scene.assign(node, "text", WORDS[rng.below(4)]).unwrap();
        }
        2 if container => {
            scene.assign(node, "gap", rng.below(10) as f64).unwrap();
        }
        3 if container && node != root => {
            let child = leaf(scene, rng);
            scene.reparent(child, Some(node)).unwrap();
        }
        4 if node != root => scene.remove(node).unwrap(),
        5 if node != root => {
            let visible = !scene.bool_value(node, "visible").unwrap();
            scene.assign(node, "visible", visible).unwrap();
        }
        6 => {
            let mut order = scene.children(node).unwrap().to_vec();
            order.reverse();
            scene.reorder_children(node, &order).unwrap();
        }
        7 if node != root => {
            let target = all[rng.below(all.len())];
            if matches!(scene.element(target).unwrap(), Element::Flex) {
                let _ = scene.reparent(node, Some(target));
            }
        }
        _ if container => {
            let grow = Value::Map(BTreeMap::from([(
                "grow".to_owned(),
                Value::Number(rng.below(3) as f64),
            )]));
            scene.assign(node, "layout", grow).unwrap();
        }
        _ => {}
    }
}

/// Measures and places with `tree`, as a layout pass does.
fn run(
    tree: &mut FlexTree,
    scene: &Scene,
    size: Option<(f64, f64)>,
    text: &mut Recording,
) -> (Size, Vec<crate::flex::Flexed>) {
    let requested = FastMap::default();
    tree.set_root_size(size).unwrap();
    let available = match size {
        Some((width, height)) => TaffySize {
            width: AvailableSpace::Definite(width as f32),
            height: AvailableSpace::Definite(height as f32),
        },
        None => TaffySize {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    };
    tree.compute(scene, available, &requested, text).unwrap();
    (
        tree.size().unwrap(),
        tree.geometries(Geometry::default()).unwrap(),
    )
}

#[test]
fn a_kept_flex_tree_matches_one_built_afresh() {
    for seed in 1..=200u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut scene = Scene::new();
        let root = container(&mut scene, &mut rng, 3);
        let requested = FastMap::default();
        let mut kept = FlexTree::build(&scene, root, &requested).unwrap();
        let mut kept_text = Recording::default();
        for step in 0..30 {
            for _ in 0..1 + rng.below(3) {
                mutate(&mut scene, &mut rng, root);
            }
            let size = rng
                .chance(50)
                .then(|| (100.0 + rng.below(300) as f64, 200.0));
            let mut fresh = FlexTree::build(&scene, root, &requested).unwrap();
            let mut fresh_text = Recording {
                last: kept_text.last.clone(),
            };
            let expected = run(&mut fresh, &scene, size, &mut fresh_text);
            kept.sync(&scene, &requested).unwrap();
            let got = run(&mut kept, &scene, size, &mut kept_text);
            assert_eq!(got.0, expected.0, "seed {seed}, step {step}: root size");
            assert_eq!(got.1, expected.1, "seed {seed}, step {step}: geometry");
            for node in nodes(&scene, root) {
                assert_eq!(
                    kept_text.last.get(&node),
                    fresh_text.last.get(&node),
                    "seed {seed}, step {step}: text {node:?} last measured differently"
                );
            }
        }
    }
}
