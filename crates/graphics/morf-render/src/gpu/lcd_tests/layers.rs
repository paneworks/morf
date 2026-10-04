// Subpixel text inside layers: translucent, faded, rounded and plain opaque
// ones, and with subpixel text turned off.

use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, NodeHandle};

use super::*;

#[test]
#[ignore = "requires a GPU adapter"]
fn text_in_a_translucent_or_faded_layer_stays_greyscale_and_unchanged() {
    // Ground and text both inside a layer that is composited translucent:
    // the fringes would be mixed with whatever is behind. Pixel for pixel
    // what greyscale draws.
    let (rect, text) = scene_nodes();
    for (opacity, blur, why) in [
        (0.9, 0.0, "a translucent layer"),
        (0.02, 0.0, "a layer fading in"),
        (1.0, 2.0, "a blurred layer"),
    ] {
        let mut layer = rounded_panel(rect, 0..2, 4.0);
        layer.opacity = opacity;
        layer.blur = blur;
        let list = DrawList {
            commands: vec![panel_fill(rect, 4.0), ink(text, "Illumination")],
            layers: vec![layer],
        };
        let Some(pixels) = frame(Some(RGB), &list, 120, false) else {
            return;
        };
        let grey = frame(None, &list, 120, false).unwrap();
        assert_eq!(fringed(&pixels, 2), 0, "{why}: drawn in subpixels");
        assert!(pixels == grey, "{why}: not what greyscale drew");
    }
    // A layer with nothing opaque of its own beneath the text, over an
    // opaque surface rectangle: its target is transparent there.
    let list = DrawList {
        commands: vec![ground(rect, 255), ink(text, "Illumination")],
        layers: vec![rounded_panel(rect, 1..2, 4.0)],
    };
    if let Some(pixels) = frame(Some(RGB), &list, 120, false) {
        assert!(
            pixels == frame(None, &list, 120, false).unwrap(),
            "over a bare layer"
        );
    }
}

/// A rounded panel's fill, the whole frame, corners of `radius`.
fn panel_fill(node: NodeHandle, radius: f64) -> DrawCommand {
    let mut quad = ground(node, 255);
    if let DrawCommand::Quad { radii, .. } = &mut quad {
        *radii = [radius; 4];
    }
    quad
}

/// The layer a rounded `ClipRect` over the whole frame makes.
fn rounded_panel(node: NodeHandle, commands: std::ops::Range<usize>, radius: f64) -> Layer {
    let bounds = Geometry {
        x: 0.0,
        y: 0.0,
        width: f64::from(WIDTH),
        height: f64::from(HEIGHT),
    };
    Layer {
        node,
        commands,
        parent: None,
        opacity: 1.0,
        blur: 0.0,
        shadow_color: Color::rgba8(0, 0, 0, 0),
        shadow_blur: 0.0,
        shadow_offset: [0.0, 0.0],
        alpha_mask: None,
        mask_for: None,
        mask: Some(LayerMask {
            bounds,
            transform: Transform2D::IDENTITY,
            radii: [radius; 4],
        }),
        shader: None,
        bounds,
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn text_in_a_rounded_opaque_panel_is_subpixel_as_if_drawn_straight() {
    // The panel's own fill beneath the text, the layer composited opaque:
    // the text is drawn in subpixels into the layer, and the composite
    // leaves every pixel as the surface would have had it without a layer,
    // but for the corners the mask rounds off.
    let (rect, text) = scene_nodes();
    let panel = DrawList {
        commands: vec![panel_fill(rect, 4.0), ink(text, "Illumination")],
        layers: vec![rounded_panel(rect, 0..2, 4.0)],
    };
    let straight = DrawList {
        commands: vec![panel_fill(rect, 4.0), ink(text, "Illumination")],
        layers: Vec::new(),
    };
    let Some(pixels) = frame(Some(RGB), &panel, 120, false) else {
        return;
    };
    assert!(
        fringed(&pixels, 24) > 40,
        "subpixel text in the panel: {}",
        fringed(&pixels, 24)
    );
    let expected = frame(Some(RGB), &straight, 120, false).unwrap();
    let corner = |x: u32, y: u32| {
        let edge = x == 0 || y == 0 || x == WIDTH - 1 || y == HEIGHT - 1;
        edge || (!(6..WIDTH - 6).contains(&x) && !(6..HEIGHT - 6).contains(&y))
    };
    let mut differ = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if corner(x, y) {
                continue;
            }
            let (got, want) = (pixel(&pixels, x, y), pixel(&expected, x, y));
            if got.iter().zip(want).any(|(a, b)| a.abs_diff(b) > 1) {
                differ += 1;
            }
            assert_eq!(got[3], 255, "the panel stays opaque at {x},{y}");
        }
    }
    assert_eq!(differ, 0, "{differ} pixels differ from the straight draw");
    // And the rounded corner is still cut.
    assert!(pixel(&pixels, 0, 0)[3] < 255);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn subpixel_text_off_draws_layers_as_before() {
    // With subpixel text off, a rounded panel is exactly the greyscale path.
    let (rect, text) = scene_nodes();
    let panel = DrawList {
        commands: vec![panel_fill(rect, 4.0), ink(text, "Illumination")],
        layers: vec![rounded_panel(rect, 0..2, 4.0)],
    };
    let Some(off) = frame(None, &panel, 120, false) else {
        return;
    };
    assert_eq!(fringed(&off, 2), 0);
    // At a fractional scale subpixel text is off for the whole frame.
    let fractional = frame(Some(RGB), &panel, 150, false).unwrap();
    assert!(fractional == frame(None, &panel, 150, false).unwrap());
}

#[test]
#[ignore = "requires a GPU adapter"]
fn text_in_an_unmasked_opaque_layer_is_subpixel() {
    let (rect, text) = scene_nodes();
    let mut layer = rounded_panel(rect, 0..2, 0.0);
    layer.mask = None;
    let list = DrawList {
        commands: vec![ground(rect, 255), ink(text, "Illumination")],
        layers: vec![layer],
    };
    let Some(pixels) = frame(Some(RGB), &list, 120, false) else {
        return;
    };
    assert!(fringed(&pixels, 24) > 40, "{}", fringed(&pixels, 24));
}
