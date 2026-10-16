//! Blurred layers sharing a target: which layers are grouped, that every
//! layer is still scheduled once, that places are kept apart, and that a
//! shared target draws exactly what a target of each layer's own did.
//!
//! The GPU half runs under a Vulkan driver: `nixVulkan cargo test -p
//! morf-render --lib layer_share -- --include-ignored`.

use std::collections::BTreeSet;

use morf_layout::{Layout, Size};
use morf_scene::{Element, NodeHandle, Scene};

use super::backdrops::Offscreen;
use super::layer_pool::{Stage, layer_regions, schedule};
use super::layer_share::{blur_gutter, pack_spaced};
use crate::tests::NoText;
use crate::*;

const WIDTH: u32 = 200;
const HEIGHT: u32 = 120;

/// Six blurred cards, five of them whole on the surface and the last cut by
/// its right edge, each holding a rounded clip of its own colour: a child
/// layer, so a card is a layer holding a layer. `only` keeps one card.
fn cards(only: Option<usize>, blurs: [f64; 6]) -> (Scene, NodeHandle) {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", f64::from(WIDTH)).unwrap();
    scene.assign(root, "height", f64::from(HEIGHT)).unwrap();
    for (index, blur) in blurs.iter().enumerate() {
        if only.is_some_and(|only| only != index) {
            continue;
        }
        let card = scene.create(Element::Rect);
        let (x, y) = if index == 5 {
            (176.0, 70.0)
        } else {
            (
                8.0 + 64.0 * (index % 3) as f64,
                8.0 + 56.0 * (index / 3) as f64,
            )
        };
        for (property, value) in [
            ("x", x),
            ("y", y),
            ("width", 40.0),
            ("height", 36.0),
            ("blur", *blur),
        ] {
            scene.assign(card, property, value).unwrap();
        }
        scene.assign(card, "color", "#30343cff").unwrap();
        scene.reparent(card, Some(root)).unwrap();
        let clip = scene.create(Element::ClipRect);
        for (property, value) in [
            ("x", 6.0),
            ("y", 6.0),
            ("width", 20.0),
            ("height", 14.0),
            ("radius", 4.0),
        ] {
            scene.assign(clip, property, value).unwrap();
        }
        let tone = [
            "#e63946ff",
            "#f1c453ff",
            "#2a9d8fff",
            "#457b9dff",
            "#e76f51ff",
            "#8ab17dff",
        ];
        scene.assign(clip, "color", tone[index]).unwrap();
        scene.reparent(clip, Some(card)).unwrap();
        let bar = scene.create(Element::Rect);
        for (property, value) in [("x", 4.0), ("y", 8.0), ("width", 30.0), ("height", 3.0)] {
            scene.assign(bar, property, value).unwrap();
        }
        scene.assign(bar, "color", "#ffffffff").unwrap();
        scene.reparent(bar, Some(clip)).unwrap();
    }
    (scene, root)
}

fn list(scene: &Scene, root: NodeHandle) -> DrawList {
    let size = Size {
        width: f64::from(WIDTH),
        height: f64::from(HEIGHT),
    };
    let layout = Layout::compute(scene, root, size, &mut NoText).unwrap();
    DrawList::from_scene(scene, &layout).unwrap()
}

/// The stages a full frame of `list` is drawn in.
fn stages(list: &DrawList) -> Vec<Stage> {
    let surface = DamageRect {
        x: 0,
        y: 0,
        width: WIDTH,
        height: HEIGHT,
    };
    let regions = layer_regions(list, &[surface], &[], 120, (WIDTH, HEIGHT));
    let bounds: Vec<Option<DamageRect>> = regions
        .iter()
        .map(|region| region.as_ref().map(|region| region.bounds))
        .collect();
    // Paint order, every layer after the layers it holds.
    let mut order: Vec<usize> = (0..list.layers.len()).collect();
    order.sort_by_key(|layer| std::cmp::Reverse(list.layers[*layer].parent.is_none()));
    let order: Vec<Offscreen> = order.into_iter().rev().map(Offscreen::Layer).collect();
    schedule(list, &order, &bounds, |_| false)
}

#[test]
fn siblings_blurred_alike_share_and_the_rest_stand_alone() {
    let blurs = [3.0, 3.0, 3.0, 5.0, 5.0, 2.0];
    let (scene, root) = cards(None, blurs);
    let list = list(&scene, root);
    let blurred: Vec<usize> = (0..list.layers.len())
        .filter(|layer| list.layers[*layer].blur > 0.0)
        .collect();
    assert_eq!(blurred.len(), 6);
    let stages = stages(&list);
    let mut shared: Vec<BTreeSet<usize>> = stages
        .iter()
        .filter_map(|stage| match stage {
            Stage::Shared(layers) => Some(layers.iter().copied().collect()),
            _ => None,
        })
        .collect();
    shared.sort_by_key(|group| std::cmp::Reverse(group.len()));
    let by_three: BTreeSet<usize> = blurred[..3].iter().copied().collect();
    let by_five: BTreeSet<usize> = blurred[3..5].iter().copied().collect();
    assert_eq!(shared, vec![by_three, by_five], "{stages:?}");
    assert!(
        stages.contains(&Stage::Solo(blurred[5])),
        "the only one blurred by 2 stands alone: {stages:?}"
    );
}

#[test]
fn every_layer_drawn_is_scheduled_exactly_once() {
    let (scene, root) = cards(None, [3.0; 6]);
    let list = list(&scene, root);
    let mut seen = Vec::new();
    for stage in stages(&list) {
        match stage {
            Stage::Atlas(layers) | Stage::Shared(layers) => seen.extend(layers),
            Stage::Solo(layer) => seen.push(layer),
            Stage::Backdrop(_) => {}
        }
    }
    let unique: BTreeSet<usize> = seen.iter().copied().collect();
    assert_eq!(unique.len(), seen.len(), "no layer twice: {seen:?}");
    assert_eq!(
        unique,
        (0..list.layers.len()).collect::<BTreeSet<_>>(),
        "every card and every clip inside one"
    );
    // A card is never in a stage with the clip it holds.
    for stage in stages(&list) {
        if let Stage::Shared(layers) = stage {
            for layer in &layers {
                assert!(
                    layers
                        .iter()
                        .all(|other| list.layers[*other].parent != Some(*layer))
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_shared_target_draws_what_a_target_of_its_own_did() {
    require_gpu!();
    let blurs = [3.0; 6];
    let (scene, root) = cards(None, blurs);
    let mut backend = pollster::block_on(WgpuBackend::new(WIDTH, HEIGHT)).unwrap();
    let shared = read_frame_wide(&mut backend, &list(&scene, root));
    let (textures, _) = backend.layer_pool.footprint();
    assert!(
        textures < 20,
        "five cards share one target and one chain: {textures} textures"
    );
    for card in 0..6 {
        let (scene, root) = cards(Some(card), blurs);
        let mut alone = pollster::block_on(WgpuBackend::new(WIDTH, HEIGHT)).unwrap();
        let own = read_frame_wide(&mut alone, &list(&scene, root));
        let (x, y) = if card == 5 {
            (164, 58)
        } else {
            (64 * (card % 3) as u32, 56 * (card / 3) as u32)
        };
        let mut worst = 0;
        for row in y..(y + 56).min(HEIGHT) {
            for column in x..(x + 64).min(WIDTH) {
                let at = ((row * WIDTH + column) * 4) as usize;
                for channel in 0..4 {
                    worst = worst.max(shared[at + channel].abs_diff(own[at + channel]));
                }
            }
        }
        assert!(
            worst <= 2,
            "card {card}: shared differs from its own by {worst}"
        );
        // And the card is there at all: its clip's colour at its middle.
        let middle = (((y + 22) * WIDTH + x + 24) * 4) as usize;
        assert!(
            shared[middle + 3] > 200,
            "card {card} drew: {:?}",
            &shared[middle..middle + 4]
        );
    }
}

/// `read_frame` for a surface that is not square.
fn read_frame_wide(backend: &mut WgpuBackend, list: &DrawList) -> Vec<u8> {
    backend
        .render(
            list,
            &[DamageRect {
                x: 0,
                y: 0,
                width: WIDTH,
                height: HEIGHT,
            }],
            120,
        )
        .unwrap();
    backend.read_pixels()
}

#[test]
fn shared_places_keep_their_distance_on_the_quarter_grid() {
    let sizes = [(30, 10), (31, 21), (50, 9), (44, 40)];
    let gutter = blur_gutter(1.5);
    assert!(
        gutter >= 12 + 9,
        "past the chain's reach beyond the margin: {gutter}"
    );
    let atlases = pack_spaced(&sizes, (128, 4096), gutter, 4);
    assert_eq!(atlases.len(), 1);
    let (size, placed) = &atlases[0];
    let boxes: Vec<(u32, u32, u32, u32)> = placed
        .iter()
        .map(|(index, (x, y))| (*x, *y, sizes[*index].0, sizes[*index].1))
        .collect();
    for (index, a) in boxes.iter().enumerate() {
        assert!(a.0 % 4 == 0 && a.1 % 4 == 0, "on the grid: {boxes:?}");
        assert!(
            a.0 + a.2 <= size.0 && a.1 + a.3 <= size.1,
            "inside: {boxes:?}"
        );
        for b in &boxes[index + 1..] {
            let apart = a.0 + a.2 + gutter <= b.0
                || b.0 + b.2 + gutter <= a.0
                || a.1 + a.3 + gutter <= b.1
                || b.1 + b.3 + gutter <= a.1;
            assert!(apart, "{gutter} apart: {a:?} and {b:?}");
        }
    }
}
