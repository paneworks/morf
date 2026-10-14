//! The one-box field pipeline against the general one: a field of a single
//! box, plain, stroked, turned, shadowed outside, inside and around, drawn
//! by each and read back -- within one level in 255 of each other.
//!
//! The general path is reached by adding a layer that is not there: a
//! polygon of no opacity, which the composition leaves out and which sends
//! the field to the general pipeline.

use morf_layout::Geometry;
use morf_scene::Color;

use super::field_pass::FieldVariant;
use super::field_tests::{field_command, field_layer, render_readback};
use crate::{BorderAlignment, DrawCommand, DrawList, Shape};

const SIZE: u32 = 112;

/// One box field, as `edit` shapes it, through the one-box pipeline and
/// through the general one.
fn both(edit: impl Fn(&mut DrawCommand)) -> (DrawList, DrawList) {
    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Sdf);
    let mut layer = field_layer(28.0, 24.0, 56.0, Shape::Box);
    layer.bounds.height = 48.0;
    layer.radii = [12.0, 6.0, 16.0, 2.0];
    layer.color = Color::rgba8(80, 160, 240, 230);
    let mut command = field_command(node, vec![layer.clone()]);
    if let DrawCommand::Field { bounds, .. } = &mut command {
        *bounds = Geometry {
            x: 28.0,
            y: 24.0,
            width: 56.0,
            height: 48.0,
        };
    }
    edit(&mut command);
    let mut general = command.clone();
    if let DrawCommand::Field { layers, .. } = &mut general {
        let mut absent = layers[0].clone();
        absent.shape = Shape::Polygon;
        absent.morph_to = Shape::Polygon;
        absent.opacity = 0.0;
        layers.push(absent);
    }
    assert_eq!(FieldVariant::for_command(&command), FieldVariant::OneBox);
    assert_eq!(FieldVariant::for_command(&general), FieldVariant::General);
    let list = |command| DrawList {
        commands: vec![command],
        layers: Vec::new(),
    };
    (list(command), list(general))
}

fn shadow(command: &mut DrawCommand, offset: (f64, f64), blur: f64, spread: f64, inner: bool) {
    if let DrawCommand::Field {
        shadow_color,
        shadow_blur,
        shadow_spread,
        shadow_offset_x,
        shadow_offset_y,
        shadow_inner,
        ..
    } = command
    {
        *shadow_color = Color::rgba8(10, 0, 30, 170);
        *shadow_blur = blur;
        *shadow_spread = spread;
        (*shadow_offset_x, *shadow_offset_y) = offset;
        *shadow_inner = inner;
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_one_box_field_draws_as_the_general_pipeline_does() {
    require_gpu!();
    type Edit = fn(&mut DrawCommand);
    let cases: [(&str, Edit); 6] = [
        ("plain", |_| {}),
        ("offset shadow", |command| {
            shadow(command, (6.0, 9.0), 10.0, 0.0, false)
        }),
        ("centred shadow, spread", |command| {
            shadow(command, (0.0, 0.0), 6.0, 4.0, false)
        }),
        ("inner shadow", |command| {
            shadow(command, (3.0, -4.0), 5.0, 0.0, true)
        }),
        ("stroked, softened", |command| {
            if let DrawCommand::Field {
                stroke_color,
                stroke_width,
                stroke_alignment,
                softness,
                ..
            } = command
            {
                *stroke_color = Color::rgba8(250, 240, 200, 255);
                *stroke_width = 3.0;
                *stroke_alignment = BorderAlignment::Inside;
                *softness = 1.5;
            }
        }),
        ("turned, morphing box to box, shadowed", |command| {
            if let DrawCommand::Field { layers, .. } = command {
                layers[0].rotation = 20.0;
                layers[0].matrix = [1.0, 0.0, 0.3, 0.9];
                layers[0].morph = 0.5;
            }
            shadow(command, (-5.0, 7.0), 8.0, 2.0, false);
        }),
    ];
    for (name, edit) in cases {
        let (one, general) = both(edit);
        let drawn = render_readback(&one, SIZE);
        let reference = render_readback(&general, SIZE);
        let (difference, x, y) = super::partial_tests::largest_difference(&drawn, &reference, SIZE);
        assert!(
            difference <= 1,
            "{name}: the one-box pipeline differs from the general one by {difference} at ({x}, {y})"
        );
        let painted = drawn.chunks(4).filter(|pixel| pixel[3] > 0).count();
        assert!(painted > 1000, "{name}: only {painted} pixels painted");
    }
}
