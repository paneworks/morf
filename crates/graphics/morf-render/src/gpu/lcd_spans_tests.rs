// Which glyphs are drawn in subpixels, decided on the CPU: one glyph, one
// ground, and the layer between them changed one property at a time.

use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, NodeHandle};

use super::glyphs::{GlyphBatch, GlyphInstance, GlyphSpan};
use super::lcd_spans::mark_subpixel_glyphs;
use crate::gpu::tests::test_quad;
use crate::gpu::text_field_tests::text_command;
use crate::*;

const TARGET: (u32, u32) = (200, 100);

fn nodes() -> (NodeHandle, NodeHandle) {
    let mut scene = morf_scene::Scene::new();
    (
        scene.create(morf_scene::Element::ClipRect),
        scene.create(morf_scene::Element::Text),
    )
}

fn whole() -> Geometry {
    Geometry {
        x: 0.0,
        y: 0.0,
        width: f64::from(TARGET.0),
        height: f64::from(TARGET.1),
    }
}

/// A rounded panel's own fill: the whole target, corners of twelve.
fn panel(node: NodeHandle, alpha: u8) -> DrawCommand {
    let mut quad = test_quad(
        node,
        Color::rgba8(0, 0, 0, alpha),
        Color::rgba8(0, 0, 0, 0),
        0.0,
    );
    if let DrawCommand::Quad { bounds, radii, .. } = &mut quad {
        *bounds = whole();
        *radii = [12.0; 4];
    }
    quad
}

/// The layer a rounded `ClipRect` makes, holding `commands`.
fn rounded(node: NodeHandle, commands: std::ops::Range<usize>) -> Layer {
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
            bounds: whole(),
            transform: Transform2D::IDENTITY,
            radii: [12.0; 4],
        }),
        shader: None,
        bounds: whole(),
    }
}

/// One plain glyph over surface pixels `(x, y, width, height)`, for the text
/// command at `command`.
fn batch(
    commands: usize,
    command: usize,
    (x, y, width, height): (f32, f32, f32, f32),
) -> GlyphBatch {
    let (tw, th) = (TARGET.0 as f32, TARGET.1 as f32);
    let mut command_spans: Vec<Vec<GlyphSpan>> = (0..commands).map(|_| Vec::new()).collect();
    command_spans[command].push(GlyphSpan {
        range: 0..1,
        color: false,
        lcd: false,
    });
    GlyphBatch {
        instances: vec![GlyphInstance {
            origin: [x / tw * 2.0 - 1.0, 1.0 - y / th * 2.0],
            axes: [width / tw * 2.0, 0.0, 0.0, -height / th * 2.0],
            ..GlyphInstance::default()
        }],
        command_spans,
        plain: vec![true],
    }
}

const MIDDLE: (f32, f32, f32, f32) = (80.0, 40.0, 20.0, 20.0);

/// Whether the one glyph comes out subpixel.
fn decide(list: &DrawList, text: usize, at: (f32, f32, f32, f32), opaque: bool) -> bool {
    decide_with(list, text, at, opaque, true)
}

fn decide_with(
    list: &DrawList,
    text: usize,
    at: (f32, f32, f32, f32),
    opaque: bool,
    plain: bool,
) -> bool {
    let mut glyphs = batch(list.commands.len(), text, at);
    glyphs.plain[0] = plain;
    // The innermost layer owning each command, as the renderer finds it.
    let mut owner = vec![None; list.commands.len()];
    for (index, layer) in list.layers.iter().enumerate() {
        for slot in &mut owner[layer.commands.clone()] {
            *slot = Some(index);
        }
    }
    mark_subpixel_glyphs(
        &mut glyphs,
        list,
        |command| owner[command],
        120,
        TARGET,
        opaque,
    );
    let spans = &glyphs.command_spans[text];
    assert_eq!(spans.len(), 1);
    spans[0].lcd
}

/// A rounded panel with its fill and its text, the layer changed by `change`.
fn in_panel(change: impl Fn(&mut Layer)) -> DrawList {
    let (clip, text) = nodes();
    let mut layer = rounded(clip, 0..2);
    change(&mut layer);
    DrawList {
        commands: vec![
            panel(clip, 255),
            text_command(text, "a", 12.0, DistanceFieldStyle::default()),
        ],
        layers: vec![layer],
    }
}

#[test]
fn text_on_a_rounded_opaque_panel_is_subpixel() {
    assert!(decide(&in_panel(|_| {}), 1, MIDDLE, false));
    // A plain layer (`layer = true`, a shadow) keeps its pixels as well.
    assert!(decide(
        &in_panel(|layer| {
            layer.mask = None;
            layer.shadow_color = Color::rgba8(0, 0, 0, 128);
            layer.shadow_blur = 8.0;
        }),
        1,
        MIDDLE,
        false
    ));
}

#[test]
fn text_in_a_faded_blurred_or_shaded_panel_is_greyscale() {
    assert!(!decide(
        &in_panel(|layer| layer.opacity = 0.99),
        1,
        MIDDLE,
        false
    ));
    assert!(!decide(
        &in_panel(|layer| layer.opacity = 0.0),
        1,
        MIDDLE,
        false
    ));
    assert!(!decide(
        &in_panel(|layer| layer.blur = 4.0),
        1,
        MIDDLE,
        false
    ));
    assert!(!decide(
        &in_panel(|layer| {
            layer.shader = Some(crate::ShaderBinding {
                program: 7,
                params: Vec::new(),
                data: Vec::new(),
                samples_behind: true,
                owns_coverage: false,
            })
        }),
        1,
        MIDDLE,
        false
    ));
    // A mask that is turned: the layer's contents are too.
    assert!(!decide(
        &in_panel(|layer| {
            if let Some(mask) = &mut layer.mask {
                let (sin, cos) = 0.1_f64.sin_cos();
                mask.transform = Transform2D {
                    matrix: [cos, sin, -sin, cos, 0.0, 0.0],
                };
            }
        }),
        1,
        MIDDLE,
        false
    ));
}

#[test]
fn text_on_a_translucent_panel_or_none_is_greyscale() {
    let (clip, text) = nodes();
    let translucent = DrawList {
        commands: vec![
            panel(clip, 200),
            text_command(text, "a", 12.0, DistanceFieldStyle::default()),
        ],
        layers: vec![rounded(clip, 0..2)],
    };
    assert!(!decide(&translucent, 1, MIDDLE, false));
    // The ground drawn after the text is no ground for it.
    let after = DrawList {
        commands: vec![
            text_command(text, "a", 12.0, DistanceFieldStyle::default()),
            panel(clip, 255),
        ],
        layers: vec![rounded(clip, 0..2)],
    };
    assert!(!decide(&after, 0, MIDDLE, false));
}

#[test]
fn a_layer_target_is_never_ground_by_itself() {
    let (clip, text) = nodes();
    // Opaque ground on the surface, text in a transparent panel above it:
    // the layer's own pixels under the glyph are transparent.
    let list = DrawList {
        commands: vec![
            panel(clip, 255),
            text_command(text, "a", 12.0, DistanceFieldStyle::default()),
        ],
        layers: vec![rounded(clip, 1..2)],
    };
    assert!(!decide(&list, 1, MIDDLE, false));
    // A surface declared opaque does not make a layer's target opaque.
    assert!(!decide(&list, 1, MIDDLE, true));
    // ...but it still is ground for text drawn straight onto it.
    let straight = DrawList {
        commands: vec![text_command(text, "a", 12.0, DistanceFieldStyle::default())],
        layers: Vec::new(),
    };
    assert!(decide(&straight, 0, MIDDLE, true));
    assert!(!decide(&straight, 0, MIDDLE, false));
}

#[test]
fn a_glyph_reaching_into_a_rounded_corner_is_greyscale() {
    // The panel's fill is square here: a child rect as large as the panel,
    // so only the mask rounds the corner off.
    let (clip, text) = nodes();
    let mut square = panel(clip, 255);
    if let DrawCommand::Quad { radii, .. } = &mut square {
        *radii = [0.0; 4];
    }
    let list = DrawList {
        commands: vec![
            square,
            text_command(text, "a", 12.0, DistanceFieldStyle::default()),
        ],
        layers: vec![rounded(clip, 0..2)],
    };
    assert!(decide(&list, 1, MIDDLE, false));
    assert!(!decide(&list, 1, (2.0, 2.0, 10.0, 10.0), false));
    assert!(!decide(&list, 1, (188.0, 88.0, 10.0, 10.0), false));
    // Along an edge, clear of the corners, it is inside.
    assert!(decide(&list, 1, (90.0, 6.0, 10.0, 10.0), false));
}

#[test]
fn a_panel_inside_a_faded_one_is_greyscale() {
    let (clip, text) = nodes();
    let mut outer = rounded(clip, 0..2);
    outer.opacity = 0.5;
    let mut inner = rounded(clip, 0..2);
    inner.parent = Some(0);
    let list = DrawList {
        commands: vec![
            panel(clip, 255),
            text_command(text, "a", 12.0, DistanceFieldStyle::default()),
        ],
        layers: vec![outer, inner],
    };
    assert!(!decide(&list, 1, MIDDLE, false));
    // With the outer one opaque, the inner one's fill is the ground.
    let mut list = list;
    list.layers[0].opacity = 1.0;
    assert!(decide(&list, 1, MIDDLE, false));
    // The outer one's corners bound the inner one's text too.
    list.layers[1].mask = None;
    assert!(!decide(&list, 1, (2.0, 2.0, 10.0, 10.0), false));
}

#[test]
fn only_plain_glyphs_are_subpixel() {
    // Morphing, outlined, turned: `plain` says so, and the panel changes
    // nothing about it.
    assert!(!decide_with(&in_panel(|_| {}), 1, MIDDLE, false, false));
}
