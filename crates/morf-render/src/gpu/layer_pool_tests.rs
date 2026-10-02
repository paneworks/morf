//! Which part of each layer a frame renders, and how the work is grouped —
//! decided on the CPU, so checked without an adapter.

use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, NodeHandle};

use super::backdrops::Offscreen;
use super::layer_pool::{LayerRegion, Stage, layer_regions, pack, schedule};
use crate::*;

#[test]
fn animated_target_sizes_do_not_accumulate_unbounded_spare_memory() {
    // A large panel grows over 120 frames. All but the current size become
    // spare; formerly all 120 survived the frame-count expiry policy.
    let unused: Vec<_> = (0..120)
        .map(|index| (index, (1200 + index as u64) * 800, (120 - index) as u32))
        .collect();
    let budget = 8 * 1024 * 1024;
    let evicted = super::layer_pool::spare_evictions(unused.clone(), budget);
    let retained: u64 = unused
        .iter()
        .filter(|(index, _, _)| !evicted.contains(index))
        .map(|(_, pixels, _)| pixels)
        .sum();
    assert!(retained <= budget);
    assert!(
        !evicted.contains(&119),
        "the most recently used size stays reusable"
    );
    assert!(
        super::layer_pool::spare_evictions(vec![(0, budget + 1, 1)], budget).contains(&0),
        "a single oversized spare must also leave"
    );
    assert!(super::layer_pool::spare_evictions(vec![(0, budget, 1)], budget).is_empty());
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_texture_pool_bounds_spares_and_preserves_the_current_frame() {
    let mut backend = pollster::block_on(WgpuBackend::new(64, 64)).unwrap();
    let budget = 8 * 1024 * 1024;
    for width in (1024..1184).step_by(4) {
        backend.layer_pool.begin_frame();
        let (texture, _) = backend.layer_pool.take(
            &backend.device,
            wgpu::TextureFormat::Rgba8Unorm,
            (width, 1024),
            true,
            (4096, 4096),
        );
        backend.layer_pool.end_frame();
        let (_, pixels) = backend.layer_pool.footprint();
        assert_eq!(texture.width(), width);
        assert!(
            pixels <= budget + u64::from(width) * 1024,
            "{pixels} cached pixels"
        );
    }
    backend.layer_pool.begin_frame();
    let (texture, _) = backend.layer_pool.take(
        &backend.device,
        wgpu::TextureFormat::Rgba8Unorm,
        (4096, 4096),
        true,
        (4096, 4096),
    );
    backend.layer_pool.end_frame();
    assert_eq!(texture.width(), 4096);
    assert!(
        backend.layer_pool.footprint().1 >= 4096 * 4096,
        "the frame's required target survives even above the spare budget"
    );
}

fn node(index: u32) -> NodeHandle {
    let mut scene = morf_scene::Scene::new();
    let mut last = scene.create(morf_scene::Element::Item);
    for _ in 0..index {
        last = scene.create(morf_scene::Element::Item);
    }
    last
}

fn quad(node: NodeHandle, bounds: Geometry) -> DrawCommand {
    let mut command = super::tests::test_quad(
        node,
        Color::rgba8(255, 0, 0, 255),
        Color::rgba8(0, 0, 0, 0),
        0.0,
    );
    if let DrawCommand::Quad { bounds: at, .. } = &mut command {
        *at = bounds;
    }
    command
}

fn layer(
    node: NodeHandle,
    commands: std::ops::Range<usize>,
    parent: Option<usize>,
    bounds: Geometry,
) -> Layer {
    Layer {
        node,
        commands,
        parent,
        opacity: 0.5,
        blur: 0.0,
        shadow_color: Color::rgba8(0, 0, 0, 0),
        shadow_blur: 0.0,
        shadow_offset: [0.0, 0.0],
        alpha_mask: None,
        mask_for: None,
        mask: Some(LayerMask {
            bounds,
            transform: Transform2D::IDENTITY,
            radii: [4.0; 4],
        }),
        shader: None,
        bounds,
    }
}

fn geometry(x: f64, y: f64, width: f64, height: f64) -> Geometry {
    Geometry {
        x,
        y,
        width,
        height,
    }
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> DamageRect {
    DamageRect {
        x,
        y,
        width,
        height,
    }
}

/// Two panels at (0, 0) and (100, 0), 80 square; a child inside the first.
fn desk() -> DrawList {
    let first = geometry(0.0, 0.0, 80.0, 80.0);
    let second = geometry(100.0, 0.0, 80.0, 80.0);
    let child = geometry(20.0, 20.0, 20.0, 20.0);
    DrawList {
        commands: vec![
            quad(node(1), first),
            quad(node(2), child),
            quad(node(3), second),
        ],
        layers: vec![
            layer(node(4), 0..2, None, first),
            layer(node(5), 1..2, Some(0), child),
            layer(node(6), 2..3, None, second),
        ],
    }
}

#[test]
fn a_layer_the_damage_misses_is_not_rendered() {
    let list = desk();
    let regions = layer_regions(&list, &[rect(110, 10, 4, 4)], &[], 120, (200, 100));
    assert_eq!(regions[0], None, "the first panel is not touched");
    assert_eq!(regions[1], None, "nor what is inside it");
    let region = regions[2].as_ref().expect("the second panel is");
    // The damage and a pixel of margin, from an even corner.
    assert_eq!(region.reads, vec![rect(109, 9, 6, 6)]);
    assert_eq!(region.bounds, rect(108, 8, 7, 7));
}

#[test]
fn a_child_is_rendered_where_its_parent_reads_it() {
    let list = desk();
    let regions = layer_regions(&list, &[rect(30, 30, 30, 30)], &[], 120, (200, 100));
    let parent = regions[0].as_ref().expect("the damaged panel");
    assert_eq!(parent.reads, vec![rect(29, 29, 32, 32)]);
    let child = regions[1].as_ref().expect("and what it holds there");
    // The parent's read, cut to the child, with its own margin.
    assert_eq!(child.reads, vec![rect(28, 28, 12, 12)]);
    assert_eq!(regions[2], None);
}

#[test]
fn separate_damage_is_rendered_separately() {
    let list = desk();
    let damage = [rect(2, 2, 4, 4), rect(70, 70, 4, 4)];
    let regions = layer_regions(&list, &damage, &[], 120, (200, 100));
    let panel = regions[0].as_ref().unwrap();
    assert_eq!(
        panel.reads,
        vec![rect(1, 1, 6, 6), rect(69, 69, 6, 6)],
        "two small reads, not the square spanning them"
    );
    assert_eq!(panel.bounds, rect(0, 0, 75, 75), "held in one target");
}

#[test]
fn a_layer_that_samples_around_itself_is_rendered_whole() {
    let mut list = desk();
    list.layers[2].blur = 4.0;
    let regions = layer_regions(&list, &[rect(110, 10, 4, 4)], &[], 120, (200, 100));
    let LayerRegion { bounds, reads } = regions[2].clone().unwrap();
    assert_eq!(bounds, rect(100, 0, 80, 80));
    assert_eq!(reads, vec![bounds]);
}

#[test]
fn what_a_backdrop_reads_is_rendered_too() {
    let list = desk();
    let regions = layer_regions(
        &list,
        &[],
        &[(rect(50, 50, 10, 10), vec![0])],
        120,
        (200, 100),
    );
    assert!(regions[0].is_some(), "the panel beneath the glass");
    assert!(regions[2].is_none());
}

#[test]
fn independent_layers_share_a_pass_and_children_come_first() {
    let list = desk();
    let regions = layer_regions(&list, &[rect(0, 0, 200, 100)], &[], 120, (200, 100));
    let bounds: Vec<Option<DamageRect>> = regions
        .iter()
        .map(|region| region.as_ref().map(|region| region.bounds))
        .collect();
    let order = [
        Offscreen::Layer(1),
        Offscreen::Layer(0),
        Offscreen::Layer(2),
    ];
    assert_eq!(
        schedule(&list, &order, &bounds, |_| false),
        vec![Stage::Atlas(vec![1, 2]), Stage::Atlas(vec![0])],
    );
    // A backdrop that blurs again orders what is on either side of it.
    let order = [
        Offscreen::Layer(1),
        Offscreen::Backdrop(9),
        Offscreen::Layer(0),
        Offscreen::Layer(2),
    ];
    assert_eq!(
        schedule(&list, &order, &bounds, |_| true),
        vec![
            Stage::Atlas(vec![1]),
            Stage::Backdrop(9),
            Stage::Atlas(vec![0, 2]),
        ],
    );
    // One that does not, orders nothing.
    assert_eq!(
        schedule(&list, &order, &bounds, |_| false),
        vec![Stage::Atlas(vec![1, 2]), Stage::Atlas(vec![0])],
    );
}

#[test]
fn packing_keeps_places_apart_and_on_even_texels() {
    let sizes = [(30, 10), (31, 21), (50, 9)];
    let atlases = pack(&sizes, (64, 4096));
    assert_eq!(atlases.len(), 1);
    let (size, placed) = &atlases[0];
    let mut boxes: Vec<(u32, u32, u32, u32)> = placed
        .iter()
        .map(|(index, (x, y))| (*x, *y, sizes[*index].0, sizes[*index].1))
        .collect();
    for (x, y, width, height) in &boxes {
        assert!(x % 2 == 0 && y % 2 == 0, "even: {boxes:?}");
        assert!(
            x + width <= size.0 && y + height <= size.1,
            "inside: {boxes:?}"
        );
    }
    boxes.sort();
    for (index, a) in boxes.iter().enumerate() {
        for b in &boxes[index + 1..] {
            let apart = a.0 + a.2 < b.0 || b.0 + b.2 < a.0 || a.1 + a.3 < b.1 || b.1 + b.3 < a.1;
            assert!(apart, "a gap between {a:?} and {b:?}");
        }
    }
    // Too tall for one texture spills into a second.
    assert_eq!(pack(&[(10, 40), (10, 40)], (10, 60)).len(), 2);
}
