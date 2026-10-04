// Subpixel text on the GPU: drawn where lcd.rs says it may be, greyscale
// everywhere else, and in agreement with the CPU reference in lcd.rs.
//
//     nixVulkanIntel cargo test -p morf-render -- --ignored lcd

use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, NodeHandle};

use crate::gpu::tests::test_quad;
use crate::gpu::text_field_tests::text_command;
use crate::*;

mod layers;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 64;
const SIZE: f64 = 40.0;

const RGB: SubpixelText = SubpixelText {
    bgr: false,
    filter: LcdFilter::Default,
};

fn ground(node: NodeHandle, alpha: u8) -> DrawCommand {
    let mut quad = test_quad(
        node,
        Color::rgba8(255, 255, 255, alpha),
        Color::rgba8(0, 0, 0, 0),
        0.0,
    );
    if let DrawCommand::Quad { bounds, .. } = &mut quad {
        *bounds = Geometry {
            x: 0.0,
            y: 0.0,
            width: f64::from(WIDTH),
            height: f64::from(HEIGHT),
        };
    }
    quad
}

fn ink(node: NodeHandle, text: &str) -> DrawCommand {
    let mut command = text_command(node, text, SIZE, DistanceFieldStyle::default());
    if let DrawCommand::Text {
        color, transform, ..
    } = &mut command
    {
        *color = Color::rgba8(0, 0, 0, 255);
        *transform = Transform2D {
            matrix: [1.0, 0.0, 0.0, 1.0, 6.0, 4.0],
        };
    }
    command
}

/// The frame's pixels, or nothing on a device without dual-source blending.
fn frame(
    subpixel: Option<SubpixelText>,
    list: &DrawList,
    scale_120: u32,
    opaque: bool,
) -> Option<Vec<u8>> {
    let mut backend = pollster::block_on(WgpuBackend::new(WIDTH, HEIGHT)).unwrap();
    if !backend.supports_subpixel_text() {
        eprintln!("skipped: this adapter has no dual-source blending");
        return None;
    }
    backend.set_subpixel_text(subpixel);
    backend.set_opaque_surface(opaque);
    backend
        .render(
            list,
            &[DamageRect {
                x: 0,
                y: 0,
                width: WIDTH,
                height: HEIGHT,
            }],
            scale_120,
        )
        .unwrap();
    Some(backend.read_pixels())
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * WIDTH + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

/// Pixels whose channels disagree by more than `by`.
fn fringed(pixels: &[u8], by: u8) -> usize {
    pixels
        .chunks(4)
        .filter(|p| {
            let (low, high) = (p[..3].iter().min().unwrap(), p[..3].iter().max().unwrap());
            high - low > by
        })
        .count()
}

fn linear(byte: u8) -> f32 {
    let value = f32::from(byte) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn scene_nodes() -> (NodeHandle, NodeHandle) {
    let mut scene = morf_scene::Scene::new();
    (
        scene.create(morf_scene::Element::Rect),
        scene.create(morf_scene::Element::Text),
    )
}

#[test]
#[ignore = "requires a GPU adapter"]
fn over_an_opaque_rectangle_text_is_drawn_in_subpixels_with_fringes_on_the_right_sides() {
    let (rect, text) = scene_nodes();
    let list = DrawList {
        commands: vec![ground(rect, 255), ink(text, "Illumination")],
        layers: Vec::new(),
    };
    let Some(grey) = frame(None, &list, 120, false) else {
        return;
    };
    let rgb = frame(Some(RGB), &list, 120, false).unwrap();
    let bgr = frame(Some(SubpixelText { bgr: true, ..RGB }), &list, 120, false).unwrap();
    assert_eq!(fringed(&grey, 2), 0, "greyscale is grey");
    assert!(
        fringed(&rgb, 24) > 40,
        "subpixel text has fringes: {}",
        fringed(&rgb, 24)
    );
    // Where the ink starts to the right of a pixel (a stem's left side), the
    // leftmost stripe is the least covered: red stays brightest in RGB order
    // and blue in BGR. The other way round on a stem's right side.
    let (mut agree, mut disagree) = (0, 0);
    for y in 0..HEIGHT {
        for x in 1..WIDTH - 1 {
            let left = pixel(&grey, x - 1, y)[0];
            let right = pixel(&grey, x + 1, y)[0];
            let [r, _, b, _] = pixel(&rgb, x, y);
            let [r2, _, b2, _] = pixel(&bgr, x, y);
            if r.abs_diff(b) < 16 || left.abs_diff(right) < 96 {
                continue;
            }
            // Darker to the right: ink to the right.
            let ink_right = right < left;
            if (r > b) == ink_right && (b2 > r2) == ink_right {
                agree += 1;
            } else {
                disagree += 1;
            }
        }
    }
    assert!(
        agree > 20 && disagree * 10 < agree,
        "fringes on the wrong sides: {agree} right, {disagree} wrong"
    );
    // The stripes add up to the pixel: the mean of the channels stays close
    // to what greyscale drew, and away from the ink nothing changed.
    let mut total = 0.0;
    for (s, g) in rgb.chunks(4).zip(grey.chunks(4)) {
        let mean = (linear(s[0]) + linear(s[1]) + linear(s[2])) / 3.0;
        total += (mean - linear(g[0])).abs();
        assert_eq!(s[3], 255, "the ground stays opaque");
    }
    let average = total / (WIDTH * HEIGHT) as f32;
    assert!(
        average < 0.02,
        "subpixel and greyscale disagree by {average} on average"
    );
    assert_eq!(pixel(&rgb, 2, 2), [255, 255, 255, 255]);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_stem_edge_matches_the_cpu_reference() {
    // An `l`: one tall vertical stem. Its left edge is placed to a fraction
    // of a pixel by where greyscale put it, and the three channels of the
    // subpixel pixel there are what lcd.rs computes for an edge at that
    // place.
    let (rect, text) = scene_nodes();
    let list = DrawList {
        commands: vec![ground(rect, 255), ink(text, "l")],
        layers: Vec::new(),
    };
    let Some(grey) = frame(None, &list, 120, false) else {
        return;
    };
    let rgb = frame(Some(RGB), &list, 120, false).unwrap();
    let ramp = morf_text::field_units_per_logical_px(SIZE as f32);
    let y = HEIGHT / 2;
    let edge_pixel = (0..WIDTH)
        .find(|x| pixel(&grey, *x, y)[0] < 250)
        .expect("the stem is drawn");
    let mut checked = 0;
    for x in edge_pixel.saturating_sub(1)..edge_pixel + 2 {
        let covered = 1.0 - linear(pixel(&grey, x, y)[0]);
        if !(0.1..0.9).contains(&covered) {
            continue;
        }
        // Where the edge is, from how much greyscale covered: glyph.wgsl's
        // own four readings, solved for the edge's place.
        let at = greyscale_edge(covered, ramp);
        let stripes = stripe_coverage(RGB, 0.5, ramp, |dx, _| 0.5 - (dx - at) * ramp);
        let expected = blend_stripes([1.0; 3], [0.0; 3], 1.0, stripes);
        let [r, g, b, _] = pixel(&rgb, x, y);
        let got = [linear(r), linear(g), linear(b)];
        for channel in 0..3 {
            assert!(
                (got[channel] - expected[channel]).abs() < 0.03,
                "pixel {x}: {got:?} against the reference {expected:?} (edge at {at})"
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no pixel straddled the stem's edge");
}

/// What glyph.wgsl's greyscale pass covers of a pixel whose ink starts
/// `at` pixels right of its centre, the field changing by `ramp` a pixel.
fn greyscale_coverage(at: f32, ramp: f32) -> f32 {
    const TAPS: [(f32, f32); 4] = [
        (-0.125, -0.375),
        (0.375, -0.125),
        (-0.375, 0.125),
        (0.125, 0.375),
    ];
    let feather = (ramp * 0.5).max(1.0 / 255.0);
    TAPS.iter()
        .map(|(dx, _)| {
            let here = 0.5 - (dx - at) * ramp;
            let t = ((here - (0.5 - feather)) / (2.0 * feather)).clamp(0.0, 1.0);
            1.0 - t * t * (3.0 - 2.0 * t)
        })
        .sum::<f32>()
        / 4.0
}

/// The edge place greyscale drew `covered` for, by bisection: coverage
/// falls as the edge moves right.
fn greyscale_edge(covered: f32, ramp: f32) -> f32 {
    let (mut low, mut high) = (-2.0_f32, 2.0_f32);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if greyscale_coverage(middle, ramp) > covered {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) / 2.0
}

fn stays_greyscale(list: &DrawList, scale_120: u32, opaque: bool, why: &str) {
    let Some(pixels) = frame(Some(RGB), list, scale_120, opaque) else {
        return;
    };
    assert_eq!(fringed(&pixels, 2), 0, "{why}: drawn in subpixels");
    // And something was drawn at all.
    assert!(
        pixels.chunks(4).any(|p| p[3] > 0 && p[0] < 128),
        "{why}: nothing drawn"
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn text_over_nothing_or_over_a_translucent_rectangle_stays_greyscale() {
    let (rect, text) = scene_nodes();
    stays_greyscale(
        &DrawList {
            commands: vec![ink(text, "Illumination")],
            layers: Vec::new(),
        },
        120,
        false,
        "over nothing",
    );
    stays_greyscale(
        &DrawList {
            commands: vec![ground(rect, 230), ink(text, "Illumination")],
            layers: Vec::new(),
        },
        120,
        false,
        "over a translucent rectangle",
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_surface_declared_opaque_is_ground_everywhere() {
    // White text, so its fringes show over the cleared (black) target.
    let (_, text) = scene_nodes();
    let mut white = ink(text, "Illumination");
    if let DrawCommand::Text { color, .. } = &mut white {
        *color = Color::rgba8(255, 255, 255, 255);
    }
    let list = DrawList {
        commands: vec![white],
        layers: Vec::new(),
    };
    let Some(pixels) = frame(Some(RGB), &list, 120, true) else {
        return;
    };
    assert!(fringed(&pixels, 24) > 40, "{}", fringed(&pixels, 24));
    let grey = frame(Some(RGB), &list, 120, false).unwrap();
    assert_eq!(fringed(&grey, 2), 0, "not declared opaque: greyscale");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn rotated_morphing_outlined_or_fractionally_scaled_text_stays_greyscale() {
    let (rect, text) = scene_nodes();
    let with = |change: &dyn Fn(&mut DrawCommand)| {
        let mut command = ink(text, "Illumination");
        change(&mut command);
        DrawList {
            commands: vec![ground(rect, 255), command],
            layers: Vec::new(),
        }
    };
    let turned = with(&|command| {
        if let DrawCommand::Text { transform, .. } = command {
            let (sin, cos) = 0.05_f64.sin_cos();
            *transform = Transform2D {
                matrix: [cos, sin, -sin, cos, 8.0, 2.0],
            };
        }
    });
    stays_greyscale(&turned, 120, false, "rotated");
    let morphing = with(&|command| {
        if let DrawCommand::Text {
            morph_to,
            morph_progress,
            ..
        } = command
        {
            *morph_to = "Illuminated".to_owned();
            *morph_progress = 0.5;
        }
    });
    stays_greyscale(&morphing, 120, false, "mid-morph");
    let outlined = with(&|command| {
        if let DrawCommand::Text { field_style, .. } = command {
            field_style.outline_width = 1.0;
            field_style.outline_color = Color::rgba8(255, 0, 0, 255);
        }
    });
    // An outline is coloured on purpose; what matters is that the fill is
    // not fringed, which shows as no pixel off the red-black-white axis.
    if let Some(pixels) = frame(Some(RGB), &outlined, 120, false) {
        let grey = frame(None, &outlined, 120, false).unwrap();
        assert_eq!(pixels, grey, "outlined text drawn in subpixels");
    }
    stays_greyscale(&with(&|_| {}), 150, false, "at 1.25x");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn without_dual_source_blending_it_is_never_turned_on() {
    let mut backend = pollster::block_on(WgpuBackend::new(8, 8)).unwrap();
    backend.lcd_supported = false;
    assert!(!backend.set_subpixel_text(Some(RGB)));
    assert_eq!(backend.subpixel_text(), None);
    assert!(backend.lcd_pipeline.is_none());
}
