//! An incremental layout against a whole pass, on random trees under
//! random changes: the two must agree to the bit, every node, every time.

use super::*;
use morf_scene::FastMap;

/// A small, fixed-seed generator, so a failure reproduces.
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

    fn number(&mut self, max: f64) -> f64 {
        // Whole and half pixels, and now and then something awkward.
        if self.chance(10) {
            (self.next() % 10_000) as f64 / 97.0
        } else {
            (self.below((max * 2.0) as usize) as f64) / 2.0
        }
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Wraps like `WrapText`, elides to the width it is given, and remembers
/// what it was last asked for each node -- which, for the real text system,
/// is the shaped buffer the renderer draws.
#[derive(Default)]
struct Recording {
    last: FastMap<NodeHandle, (String, Option<u64>, bool)>,
    calls: usize,
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
        self.calls += 1;
        self.last.insert(
            node,
            (
                text.to_owned(),
                options.width.map(f64::to_bits),
                options.wrap,
            ),
        );
        let full = text.len() as f64 * size / 2.0;
        match options.width {
            Some(width) if width > 0.0 && options.wrap => Size {
                width: full.min(width),
                height: (full / width).ceil().max(1.0) * size,
            },
            Some(width) if width > 0.0 && options.elide != TextElide::None => Size {
                width: full.min(width),
                height: size,
            },
            _ => Size {
                width: full,
                height: size,
            },
        }
    }
}

/// A host for `Custom` containers: children stacked down a column, each
/// pushed right by its index.
struct Stack;

impl CustomLayout for Stack {
    fn measure(&mut self, _: NodeHandle, _: Size, children: &[Size]) -> Result<Size, String> {
        Ok(Size {
            width: children
                .iter()
                .enumerate()
                .map(|(index, size)| size.width + index as f64 * 3.0)
                .fold(0.0, f64::max),
            height: children.iter().map(|size| size.height).sum(),
        })
    }

    fn place(
        &mut self,
        _: NodeHandle,
        bounds: Size,
        children: &[Size],
    ) -> Result<Vec<Geometry>, String> {
        let mut y = 0.0;
        Ok(children
            .iter()
            .enumerate()
            .map(|(index, size)| {
                let placed = Geometry {
                    x: (index as f64 * 3.0).min(bounds.width),
                    y,
                    width: size.width,
                    height: size.height,
                };
                y += size.height;
                placed
            })
            .collect())
    }
}

const CONTAINERS: [Element; 10] = [
    Element::Item,
    Element::Rect,
    Element::ClipRect,
    Element::Row,
    Element::Column,
    Element::Grid,
    Element::Inset,
    Element::Flex,
    Element::Flickable,
    Element::Custom,
];

const WORDS: [&str; 6] = [
    "",
    "clock",
    "a longer line that wraps",
    "Wi-Fi",
    "notifications and more",
    "x",
];

fn anchors(rng: &mut Rng) -> Value {
    let mut map = BTreeMap::new();
    match rng.below(6) {
        0 => {
            map.insert("fill".to_owned(), Value::Bool(true));
        }
        1 => {
            map.insert("center_in".to_owned(), Value::Bool(true));
        }
        2 => {
            map.insert("left".to_owned(), Value::Bool(true));
            map.insert("right".to_owned(), Value::Bool(true));
        }
        3 => {
            map.insert("right".to_owned(), Value::Bool(true));
            map.insert("bottom".to_owned(), Value::Bool(true));
            map.insert("margins".to_owned(), Value::Number(rng.number(8.0)));
        }
        4 => {
            map.insert("horizontal_center".to_owned(), Value::Bool(true));
            map.insert("top".to_owned(), Value::Bool(true));
        }
        _ => {}
    }
    Value::Map(map)
}

/// Whether a node's parent is one that takes anchors at all.
fn anchorable(scene: &Scene, node: NodeHandle) -> bool {
    match scene.parent(node).unwrap() {
        Some(parent) => matches!(
            scene.element(parent).unwrap(),
            Element::Item | Element::Rect | Element::ClipRect | Element::Flickable
        ),
        None => false,
    }
}

fn make(scene: &mut Scene, rng: &mut Rng, depth: usize) -> NodeHandle {
    let leaf = depth == 0 || rng.chance(30);
    let element = if leaf {
        *rng.pick(&[Element::Text, Element::Rect, Element::Item, Element::Text])
    } else {
        *rng.pick(&CONTAINERS)
    };
    let node = scene.create(element);
    match element {
        Element::Text => {
            scene.assign(node, "text", *rng.pick(&WORDS)).unwrap();
            scene
                .assign(node, "font_size", 8.0 + rng.below(8) as f64)
                .unwrap();
            if rng.chance(50) {
                scene.assign(node, "wrap", true).unwrap();
            } else if rng.chance(40) {
                scene.assign(node, "elide", "right").unwrap();
            }
        }
        Element::Row | Element::Column => {
            scene.assign(node, "gap", rng.number(10.0)).unwrap();
            scene
                .assign(
                    node,
                    "justify",
                    *rng.pick(&["start", "center", "end", "space_between"]),
                )
                .unwrap();
            scene
                .assign(
                    node,
                    "align",
                    *rng.pick(&["start", "center", "end", "stretch"]),
                )
                .unwrap();
        }
        Element::Grid => {
            scene
                .assign(node, "columns", 1.0 + rng.below(3) as f64)
                .unwrap();
            scene.assign(node, "gap", rng.number(6.0)).unwrap();
        }
        Element::Flex => {
            scene
                .assign(node, "direction", *rng.pick(&["row", "column"]))
                .unwrap();
            scene.assign(node, "gap", rng.number(8.0)).unwrap();
            scene.assign(node, "wrap", rng.chance(30)).unwrap();
        }
        Element::Inset => {
            scene.assign(node, "left_margin", rng.number(10.0)).unwrap();
            scene.assign(node, "top_margin", rng.number(10.0)).unwrap();
            scene.assign(node, "resize_child", rng.chance(50)).unwrap();
        }
        Element::Flickable => {
            scene.assign(node, "content_y", rng.number(20.0)).unwrap();
        }
        _ => {}
    }
    if rng.chance(40) {
        scene.assign(node, "width", rng.number(200.0)).unwrap();
    }
    if rng.chance(40) {
        scene.assign(node, "height", rng.number(120.0)).unwrap();
    }
    if !leaf {
        let count = if element == Element::Inset {
            1
        } else {
            rng.below(5)
        };
        for _ in 0..count {
            let child = make(scene, rng, depth - 1);
            scene.reparent(child, Some(node)).unwrap();
        }
        // Now and then a mask, laid out in the node's box.
        if rng.chance(15) {
            let mask = make(scene, rng, 1);
            scene.set_mask(node, Some(mask)).unwrap();
        }
    }
    node
}

/// Every node under `root`, root first.
fn nodes(scene: &Scene, root: NodeHandle) -> Vec<NodeHandle> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        out.push(node);
        stack.extend(scene.children(node).unwrap().iter().copied());
    }
    out
}

/// One random change of the kind a running shell makes.
fn mutate(scene: &mut Scene, rng: &mut Rng, root: NodeHandle) {
    let all = nodes(scene, root);
    let node = *rng.pick(&all);
    let element = scene.element(node).unwrap();
    match rng.below(16) {
        0..=3 => {
            // The common case, by far: something animating its size.
            let property = *rng.pick(&["width", "height", "implicit_width"]);
            scene.assign(node, property, rng.number(220.0)).unwrap();
        }
        4 | 5 => {
            let property = *rng.pick(&["x", "y", "transition_x"]);
            scene.assign(node, property, rng.number(40.0)).unwrap();
        }
        6 => {
            let visible = !scene.bool_value(node, "visible").unwrap();
            scene.assign(node, "visible", visible).unwrap();
        }
        7 if element == Element::Text => {
            scene.assign(node, "text", *rng.pick(&WORDS)).unwrap();
        }
        7 | 8 if matches!(element, Element::Row | Element::Column | Element::Flex) => {
            scene.assign(node, "gap", rng.number(12.0)).unwrap();
        }
        9 if anchorable(scene, node) => {
            scene.assign(node, "anchors", anchors(rng)).unwrap();
        }
        10 if node != root && element != Element::Inset => {
            // A child made and added, as a Loader or a view does.
            let child = make(scene, rng, 2);
            scene.reparent(child, Some(node)).unwrap();
        }
        11 if node != root => {
            scene.remove(node).unwrap();
        }
        12 => {
            let mut order = scene.children(node).unwrap().to_vec();
            order.reverse();
            scene.reorder_children(node, &order).unwrap();
        }
        13 if node != root => {
            // Moved elsewhere in the same tree.
            let target = *rng.pick(&all);
            let _ = scene.reparent(node, Some(target));
        }
        15 if node != root && !scene.is_mask(node) => {
            // A mask given, or taken away.
            if scene.mask(node).is_some() && rng.chance(50) {
                scene.set_mask(node, None).unwrap();
            } else {
                let mask = make(scene, rng, 1);
                scene.set_mask(node, Some(mask)).unwrap();
            }
        }
        14 if element == Element::Flickable => {
            scene.assign(node, "content_x", rng.number(30.0)).unwrap();
        }
        _ => {
            scene.assign(node, "height", rng.number(90.0)).unwrap();
        }
    }
}

fn run(seed: u64, steps: usize) {
    let mut rng = Rng(seed);
    let mut scene = Scene::new();
    let root = make(&mut scene, &mut rng, 5);
    let mut available = Size {
        width: 400.0,
        height: 300.0,
    };
    let mut incremental = Layout::default();
    let mut incremental_text = Recording::default();
    let mut full_text = Recording::default();
    for step in 0..steps {
        let changes = 1 + rng.below(3);
        for _ in 0..changes {
            mutate(&mut scene, &mut rng, root);
        }
        if rng.chance(5) {
            available.width = 200.0 + rng.number(300.0);
        }
        let full = Layout::compute_with(&scene, root, available, &mut full_text, &mut Stack);
        let updated =
            incremental.update_with(&scene, root, available, &mut incremental_text, &mut Stack);
        match (&full, &updated) {
            (Ok(full), Ok(())) => {
                if let Some(difference) = full.difference(&incremental) {
                    let chain = nodes(&scene, root)
                        .into_iter()
                        .filter(|node| full.geometry(*node) != incremental.geometry(*node))
                        .map(|node| {
                            let mut path = Vec::new();
                            let mut current = Some(node);
                            while let Some(node) = current {
                                path.push(format!(
                                    "{:?}{:?}v{}",
                                    scene.element(node).unwrap(),
                                    node,
                                    scene.bool_value(node, "visible").unwrap()
                                ));
                                current = scene.parent(node).unwrap();
                            }
                            format!(
                                "{} full {:?} inc {:?}",
                                path.join(" < "),
                                full.geometry(node),
                                incremental.geometry(node)
                            )
                        })
                        .collect::<Vec<_>>();
                    panic!(
                        "seed {seed}, step {step}: {difference}\n{}",
                        chain.join("\n")
                    );
                }
            }
            (Err(_), Err(_)) => continue,
            (full, updated) => panic!(
                "seed {seed}, step {step}: whole pass {:?}, update {:?}",
                full.as_ref().err(),
                updated.as_ref().err()
            ),
        }
        // What each text node was last measured with is what the renderer
        // draws: the update must leave it as the whole pass does -- for every
        // text that is placed. One a flex container hides is measured and
        // never placed, so what it was last shaped at is never drawn; it is
        // placed, and so measured again, before it is shown.
        let full = full.as_ref().expect("compared above");
        for node in nodes(&scene, root) {
            if scene.element(node).unwrap() == Element::Text && full.geometry(node).is_some() {
                assert_eq!(
                    incremental_text.last.get(&node),
                    full_text.last.get(&node),
                    "seed {seed}, step {step}: text {node:?} last measured differently; {}",
                    {
                        let mut path = Vec::new();
                        let mut current = Some(node);
                        while let Some(node) = current {
                            path.push(format!(
                                "{:?}{:?}v{} {:?}",
                                scene.element(node).unwrap(),
                                node,
                                scene.bool_value(node, "visible").unwrap(),
                                full.geometry(node)
                            ));
                            current = scene.parent(node).unwrap();
                        }
                        path.join(" < ")
                    }
                );
            }
        }
    }
}

#[test]
fn an_incremental_layout_matches_a_whole_pass_on_random_trees() {
    for seed in 1..=300u64 {
        run(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1, 40);
    }
}

#[test]
fn a_size_animating_in_one_subtree_leaves_the_rest_unmeasured() {
    // A bar: a row of labels on the left, and in the middle a capsule
    // growing a pixel at a time.
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    let row = scene.create(Element::Row);
    scene.reparent(row, Some(root)).unwrap();
    for word in WORDS {
        let label = scene.create(Element::Text);
        scene.assign(label, "text", word).unwrap();
        scene.reparent(label, Some(row)).unwrap();
    }
    let capsule = scene.create(Element::Rect);
    scene
        .assign(
            capsule,
            "anchors",
            Value::Map(BTreeMap::from([(
                "horizontal_center".to_owned(),
                Value::Bool(true),
            )])),
        )
        .unwrap();
    scene.reparent(capsule, Some(root)).unwrap();
    let inside = scene.create(Element::Text);
    scene.assign(inside, "text", "12:30").unwrap();
    scene.reparent(inside, Some(capsule)).unwrap();
    let available = Size {
        width: 800.0,
        height: 40.0,
    };
    let mut text = Recording::default();
    let mut layout = Layout::default();
    layout.update(&scene, root, available, &mut text).unwrap();
    for step in 0..20 {
        scene.assign(capsule, "width", 100.0 + step as f64).unwrap();
        let before = text.calls;
        layout.update(&scene, root, available, &mut text).unwrap();
        assert_eq!(text.calls, before, "no text is measured again");
        let full = Layout::compute(&scene, root, available, &mut Recording::default()).unwrap();
        assert_eq!(full.difference(&layout), None);
    }
}
