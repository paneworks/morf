//! What a field's damage is when a layer joins or leaves it: a shape that
//! takes or loses its size (caelestia's workspace swell coming out of the
//! frame) changes the layer count, and that is no reason to repaint the
//! whole field.

use morf_scene::{Element, Scene};

use super::field_tests::{field_command, field_layer};
use crate::damage::DamageTracker;
use crate::{DrawCommand, DrawList, Operation, SdfLayer, Shape};

const SIDE: f64 = 2000.0;

fn frame(extra: Option<SdfLayer>) -> DrawList {
    let node = {
        let mut scene = Scene::new();
        scene.create(Element::Sdf)
    };
    let mut layers = vec![field_layer(0.0, 0.0, SIDE, Shape::Box)];
    layers.extend(extra);
    let mut command = field_command(node, layers);
    if let DrawCommand::Field { bounds, .. } = &mut command {
        bounds.width = SIDE;
        bounds.height = SIDE;
    }
    DrawList {
        commands: vec![command],
        layers: Vec::new(),
    }
}

fn swell(operation: Operation) -> SdfLayer {
    SdfLayer {
        operation,
        blend: 10.0,
        ..field_layer(100.0, 100.0, 20.0, Shape::Box)
    }
}

fn damaged_area(tracker: &mut DamageTracker, mut list: DrawList) -> u64 {
    let damage = tracker.diff(&list, 120);
    tracker.retain(&mut list);
    damage
        .iter()
        .map(|rect| u64::from(rect.width) * u64::from(rect.height))
        .sum()
}

#[test]
fn a_shape_joining_or_leaving_a_field_damages_only_where_it_is() {
    let whole = (SIDE * SIDE) as u64;
    let mut tracker = DamageTracker::default();
    damaged_area(&mut tracker, frame(None));
    let joined = damaged_area(&mut tracker, frame(Some(swell(Operation::SmoothUnion))));
    assert!(joined > 0, "the shape coming out drew nothing");
    assert!(
        joined < whole / 100,
        "joining damaged {joined} of {whole} px"
    );
    let left = damaged_area(&mut tracker, frame(None));
    assert!(left > 0, "the shape going away left it drawn");
    assert!(left < whole / 100, "leaving damaged {left} of {whole} px");
}

#[test]
fn an_intersection_joining_a_field_still_damages_all_of_it() {
    let whole = (SIDE * SIDE) as u64;
    let mut tracker = DamageTracker::default();
    damaged_area(&mut tracker, frame(None));
    let joined = damaged_area(&mut tracker, frame(Some(swell(Operation::Intersect))));
    assert!(
        joined >= whole,
        "an intersection takes away everything outside it"
    );
}
