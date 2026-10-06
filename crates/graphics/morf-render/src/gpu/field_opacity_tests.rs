use crate::*;

// A layer of a field fading: the field mixed between the composition without
// the layer and the one with it, seam and all, drawn.

use crate::gpu::field_tests::{alpha_at, field_command, field_layer, render_readback};
use crate::{BlendProfile, Operation, Shape};
use morf_scene::Color;

/// A frame `EDGE` thick round a `size` square, and panels hanging from its
/// top edge by a circular seam, each `(left, width, opacity, colour)`.
fn frame(size: u32, panels: &[(f64, f64, f32, Color)]) -> DrawList {
    const EDGE: f64 = 8.0;
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Sdf);
    let whole = field_layer(0.0, 0.0, f64::from(size), Shape::Box);
    let mut hole = field_layer(EDGE, EDGE, f64::from(size) - EDGE * 2.0, Shape::Box);
    hole.operation = Operation::Subtract;
    hole.radii = [12.0; 4];
    let mut layers = vec![whole, hole];
    for &(left, width, opacity, color) in panels {
        let mut panel = field_layer(left, EDGE, width, Shape::Box);
        panel.bounds.height = 40.0;
        panel.radii = [0.0, 0.0, 10.0, 10.0];
        panel.operation = Operation::SmoothUnion;
        panel.blend = 12.0;
        panel.profile = BlendProfile::Circular;
        panel.opacity = opacity;
        panel.color = color;
        layers.push(panel);
    }
    let mut command = field_command(node, layers);
    if let DrawCommand::Field { bounds, .. } = &mut command {
        bounds.width = f64::from(size);
        bounds.height = f64::from(size);
    }
    DrawList {
        commands: vec![command],
        layers: Vec::new(),
    }
}

const WHITE: Color = Color::rgba8(255, 255, 255, 255);

/// The largest difference between a picture's alpha and the mean of
/// others', weighted, over every pixel, and where it is.
fn worst_mix(size: u32, picture: &[u8], parts: &[(f32, &[u8])]) -> (f32, u32, u32) {
    let mut worst = (0.0, 0, 0);
    for y in 0..size {
        for x in 0..size {
            let expected: f32 = parts
                .iter()
                .map(|(weight, pixels)| weight * f32::from(alpha_at(pixels, size, x, y)))
                .sum();
            let off = (f32::from(alpha_at(picture, size, x, y)) - expected).abs();
            if off > worst.0 {
                worst = (off, x, y);
            }
        }
    }
    worst
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_fading_layer_is_mixed_between_absent_and_whole() {
    const SIZE: u32 = 128;
    let panel = |opacity| frame(SIZE, &[(40.0, 48.0, opacity, WHITE)]);
    let without = render_readback(&frame(SIZE, &[]), SIZE);
    let none = render_readback(&panel(0.0), SIZE);
    let whole = render_readback(&panel(1.0), SIZE);
    let half = render_readback(&panel(0.5), SIZE);
    // At nothing it is not there at all, fillet included.
    assert_eq!(worst_mix(SIZE, &none, &[(1.0, &without)]).0, 0.0);
    // The panel, its fillet into the frame, and the frame beside it.
    assert_eq!(alpha_at(&whole, SIZE, 64, 30), 255);
    assert_eq!(alpha_at(&whole, SIZE, 37, 10), 255, "the fillet");
    assert_eq!(alpha_at(&none, SIZE, 37, 10), 0);
    // Half way, every pixel is half way between the two: the panel, the
    // seam, the antialiased edges; and the frame is untouched.
    let (off, x, y) = worst_mix(SIZE, &half, &[(0.5, &none), (0.5, &whole)]);
    assert!(off <= 2.0, "{off} off the mix at {x},{y}");
    assert!(alpha_at(&half, SIZE, 64, 30).abs_diff(128) <= 2);
    assert!(alpha_at(&half, SIZE, 37, 10).abs_diff(128) <= 2);
    assert_eq!(alpha_at(&half, SIZE, 12, 4), 255, "the frame");
    assert_eq!(alpha_at(&half, SIZE, 64, 100), 0, "the open middle");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn two_fading_layers_are_each_there_or_not() {
    // Two panels close enough for their seams to bridge when both are whole.
    // Fading both to 0.4 and 0.7 is the four combinations weighted by how
    // likely each is.
    const SIZE: u32 = 128;
    let pair = |left, right| {
        frame(
            SIZE,
            &[(20.0, 40.0, left, WHITE), (66.0, 40.0, right, WHITE)],
        )
    };
    let both = render_readback(&pair(1.0, 1.0), SIZE);
    let left = render_readback(&pair(1.0, 0.0), SIZE);
    let right = render_readback(&pair(0.0, 1.0), SIZE);
    let neither = render_readback(&pair(0.0, 0.0), SIZE);
    let faded = render_readback(&pair(0.4, 0.7), SIZE);
    assert!(
        alpha_at(&both, SIZE, 63, 40) > 128,
        "whole, the seam bridges"
    );
    let (off, x, y) = worst_mix(
        SIZE,
        &faded,
        &[
            (0.4 * 0.7, &both),
            (0.4 * 0.3, &left),
            (0.6 * 0.7, &right),
            (0.6 * 0.3, &neither),
        ],
    );
    assert!(off <= 2.0, "{off} off the mix at {x},{y}");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn three_fading_layers_mix_all_eight_compositions() {
    const SIZE: u32 = 128;
    let opacities = [0.25, 0.5, 0.75];
    let picture = |values: [f32; 3]| {
        frame(
            SIZE,
            &[
                (16.0, 32.0, values[0], WHITE),
                (48.0, 32.0, values[1], WHITE),
                (80.0, 32.0, values[2], WHITE),
            ],
        )
    };
    let mut parts = Vec::new();
    for subset in 0..8 {
        let mut weight = 1.0;
        let values = std::array::from_fn(|index| {
            if subset & (1 << index) != 0 {
                weight *= opacities[index];
                1.0
            } else {
                weight *= 1.0 - opacities[index];
                0.0
            }
        });
        parts.push((weight, render_readback(&picture(values), SIZE)));
    }
    let faded = render_readback(&picture(opacities), SIZE);
    let borrowed: Vec<_> = parts
        .iter()
        .map(|(weight, pixels)| (*weight, pixels.as_slice()))
        .collect();
    let (off, x, y) = worst_mix(SIZE, &faded, &borrowed);
    assert!(off <= 2.0, "{off} off the eight-way mix at {x},{y}");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_fading_subtraction_half_fills_its_hole() {
    const SIZE: u32 = 64;
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Sdf);
    let plate = field_layer(0.0, 0.0, 64.0, Shape::Box);
    let mut hole = field_layer(16.0, 16.0, 32.0, Shape::Circle);
    hole.operation = Operation::Subtract;
    hole.opacity = 0.25;
    let pixels = render_readback(
        &DrawList {
            commands: vec![field_command(node, vec![plate, hole])],
            layers: Vec::new(),
        },
        SIZE,
    );
    assert!(
        alpha_at(&pixels, SIZE, 32, 32).abs_diff(191) <= 2,
        "a quarter of the hole is cut: {}",
        alpha_at(&pixels, SIZE, 32, 32)
    );
    assert_eq!(alpha_at(&pixels, SIZE, 4, 4), 255, "the plate beside it");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_fading_layer_brings_its_colour_in_with_it() {
    const SIZE: u32 = 128;
    let red = Color::rgba8(255, 0, 0, 255);
    let pixels = render_readback(&frame(SIZE, &[(40.0, 48.0, 0.5, red)]), SIZE);
    let at = |x: u32, y: u32| {
        let index = ((y * SIZE + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    };
    // Half a red panel over nothing: red, half covering, no white in it.
    let [r, g, b, a] = at(64, 40);
    assert!(a.abs_diff(128) <= 2, "{a}");
    assert!(r > 100 && g < 4 && b < 4, "{r} {g} {b}");
    // The frame keeps its own white.
    assert_eq!(at(12, 4), [255, 255, 255, 255]);
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_large_frame_fading_a_panel_is_tiled_without_losing_the_fade() {
    // Large enough to be drawn as tiles: the tiles a fading panel may reach,
    // and the ones filled without walking the layers, must both allow for
    // every combination.
    const SIZE: u32 = 512;
    let panel = |opacity| frame(SIZE, &[(180.0, 150.0, opacity, WHITE)]);
    let DrawCommand::Field { layers, bounds, .. } = &panel(0.5).commands[0] else {
        unreachable!()
    };
    let tiles = crate::field_tiles(
        layers,
        *bounds,
        [0.0, 0.0, 512.0, 512.0],
        1.0,
        crate::Spill {
            edge: 2.0,
            shadow: None,
            solid: true,
        },
    )
    .expect("tiled while it fades");
    assert!(!tiles.is_empty());
    let none = render_readback(&panel(0.0), SIZE);
    let whole = render_readback(&panel(1.0), SIZE);
    let half = render_readback(&panel(0.5), SIZE);
    let (off, x, y) = worst_mix(SIZE, &half, &[(0.5, &none), (0.5, &whole)]);
    assert!(off <= 2.0, "{off} off the mix at {x},{y}");
    assert!(alpha_at(&half, SIZE, 255, 30).abs_diff(128) <= 2);
}
