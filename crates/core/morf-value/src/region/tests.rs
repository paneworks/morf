use super::*;

fn rectangle(x: i32, y: i32, width: i32, height: i32, operation: Operation) -> Region {
    Region {
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        operation,
        ..Region::default()
    }
}

#[test]
fn combines_subtracts_and_merges_vertical_runs() {
    let regions = [Region {
        rect: Rect {
            x: 0,
            y: 0,
            width: 6,
            height: 4,
        },
        children: vec![rectangle(2, 1, 2, 2, Operation::Subtract)],
        ..Region::default()
    }];
    assert_eq!(
        build(6, 4, &regions).unwrap(),
        [
            Rect {
                x: 0,
                y: 0,
                width: 6,
                height: 1
            },
            Rect {
                x: 0,
                y: 1,
                width: 2,
                height: 2
            },
            Rect {
                x: 4,
                y: 1,
                width: 2,
                height: 2
            },
            Rect {
                x: 0,
                y: 3,
                width: 6,
                height: 1
            },
        ]
    );
}

#[test]
fn ellipse_and_xor_are_composable() {
    let ellipse = Region {
        rect: Rect {
            x: 0,
            y: 0,
            width: 5,
            height: 5,
        },
        shape: Shape::Ellipse,
        ..Region::default()
    };
    let regions = [
        ellipse.clone(),
        Region {
            operation: Operation::Xor,
            ..ellipse
        },
    ];
    assert!(build(5, 5, &regions).unwrap().is_empty());
}

#[cfg(test)]
mod equivalence;

#[test]
fn a_star_region_is_not_the_rectangle_around_it() {
    // The point of the merge. A star used to be drawable and not clickable:
    // the only clickable area a star-shaped node could be given was its own
    // bounding rectangle, because the rasteriser's whole vocabulary was a
    // rectangle and an ellipse.
    let star = Region {
        rect: Rect {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        },
        shape: Shape::Star,
        ..Region::default()
    };
    let built = build(32, 32, std::slice::from_ref(&star)).expect("a star composes");
    let covered: i32 = built.iter().map(|rect| rect.width * rect.height).sum();
    assert!(covered > 0, "the star covers something");
    assert!(
        covered < 32 * 32 / 2,
        "a five-pointed star covers well under half its box, not {covered} of 1024",
    );
    // The waist between two points is open, and the centre is solid.
    let filled = |x: i32, y: i32| {
        built
            .iter()
            .any(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
    };
    assert!(filled(16, 16), "the middle of the star is inside it");
    assert!(!filled(0, 0), "and its bounding corner is not");
}

#[test]
fn a_smooth_operation_composes_as_its_hard_one_on_a_mask() {
    // A mask has no partial coverage, so there is no seam to round. The shapes
    // still have to be the same ones — a smooth union that quietly composed
    // nothing would pass a test that only checked it did not crash.
    let square = |operation| Region {
        rect: Rect {
            x: 4,
            y: 4,
            width: 12,
            height: 12,
        },
        shape: Shape::Box,
        operation,
        ..Region::default()
    };
    let smooth = build(
        24,
        24,
        &[square(Operation::Union), square(Operation::SmoothUnion)],
    );
    let hard = build(
        24,
        24,
        &[square(Operation::Union), square(Operation::Union)],
    );
    assert_eq!(smooth.unwrap(), hard.unwrap());
}

#[test]
fn every_family_the_renderer_draws_can_be_composed_into_a_region() {
    // The vocabulary is one list or it is two. If a family is ever added to
    // the shader without a distance function here, it becomes drawable and not
    // clickable again, and this is what says so.
    for name in [
        "circle", "rect", "capsule", "triangle", "hexagon", "star", "ring", "pie", "cross",
        "ellipse",
    ] {
        let shape = Shape::parse(name).unwrap_or_else(|| panic!("{name} is a shape"));
        let region = Region {
            rect: Rect {
                x: 0,
                y: 0,
                width: 24,
                height: 24,
            },
            shape,
            ..Region::default()
        };
        let built = build(24, 24, std::slice::from_ref(&region))
            .unwrap_or_else(|error| panic!("{name} composes: {error}"));
        let covered: i32 = built.iter().map(|rect| rect.width * rect.height).sum();
        assert!(covered > 0, "{name} covers something");
        assert!(covered <= 24 * 24, "{name} stays inside its own rectangle");
    }
}

#[test]
fn a_coarse_build_covers_what_the_fine_one_did() {
    // The point of the coarse grid is speed, and the thing that would make it
    // useless is coming out *smaller* than the shape — a blur region a pixel
    // short of the edge painted over it shows a hard line. Rounding outward is
    // what stops that, so it is asserted rather than assumed.
    let circle = Region {
        rect: Rect {
            x: 30,
            y: 30,
            width: 100,
            height: 100,
        },
        shape: Shape::Box,
        params: ShapeParams {
            radii: [50.0; 4],
            ..ShapeParams::default()
        },
        ..Region::default()
    };
    let fine = build(256, 256, std::slice::from_ref(&circle)).unwrap();
    let covered = |rects: &[Rect], x: i32, y: i32| {
        rects.iter().any(|rect| {
            x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
        })
    };
    // Every divisor a caller might reach for, including the shared default.
    //
    // The claim is not that nothing is dropped — a circle sampled on a coarser
    // grid loses slivers along its tangent, which is inherent and was found by
    // asserting the stronger thing and watching it fail. The claim is that the
    // error is bounded by one cell: everything further inside than that is
    // covered, so a caller painting its own edge over the boundary never sees
    // a hole in the middle of the shape.
    for divisor in [2, 4, COVERED_EDGE_GRID, 16] {
        let coarse = build_scaled(256, 256, std::slice::from_ref(&circle), divisor).unwrap();
        let step = divisor as i32;
        for y in 0..256 {
            for x in 0..256 {
                let well_inside = (-step..=step)
                    .all(|dy| (-step..=step).all(|dx| covered(&fine, x + dx, y + dy)));
                if well_inside {
                    assert!(
                        covered(&coarse, x, y),
                        "divisor {divisor} dropped ({x}, {y}), a cell inside the shape"
                    );
                }
            }
        }
        assert!(
            coarse.len() < fine.len(),
            "divisor {divisor}: coarse {} vs fine {}",
            coarse.len(),
            fine.len()
        );
    }
}

#[test]
fn a_divisor_of_one_is_the_ordinary_build() {
    let square = Region {
        rect: Rect {
            x: 4,
            y: 6,
            width: 20,
            height: 12,
        },
        ..Region::default()
    };
    assert_eq!(
        build_scaled(64, 64, std::slice::from_ref(&square), 1).unwrap(),
        build(64, 64, std::slice::from_ref(&square)).unwrap(),
    );
}

/// Two half-planes meeting at a right angle: `x > 0` (a wall) and `y > 0`
/// (a floor), as distances. Their union's inside corner is at the origin.
fn corner(x: f32, y: f32) -> (f32, f32) {
    (-x, -y)
}

#[test]
fn a_circular_blend_is_a_quarter_circle_fillet() {
    let r = 10.0;
    let joint = |x: f32, y: f32| {
        let (a, b) = corner(x, y);
        combine_profiled(Operation::SmoothUnion, BlendProfile::Circular, a, b, r)
    };
    // The fillet is the circle of radius r centred at (-r, -r): every point
    // on it is on the surface, and the arc's midpoint sits r(√2 - 1) out
    // from the corner along the diagonal.
    for step in 0..=8 {
        let angle = std::f32::consts::FRAC_PI_2 * step as f32 / 8.0;
        let (x, y) = (-r + r * angle.cos(), -r + r * angle.sin());
        assert!(
            joint(x, y).abs() < 1e-4,
            "on the arc at {x},{y}: {}",
            joint(x, y)
        );
    }
    let bulge = r * (std::f32::consts::SQRT_2 - 1.0) / std::f32::consts::SQRT_2;
    assert!(joint(-bulge, -bulge).abs() < 1e-4);
    // Outside the band both shapes are exactly what they were.
    assert_eq!(joint(-30.0, 5.0), -5.0);
    assert_eq!(joint(5.0, -30.0), -5.0);
    assert_eq!(joint(-30.0, -40.0), 30.0);
    // The quadratic seam over the same radius fills the corner less: its
    // surface crosses the diagonal nearer the corner than the arc does.
    let quadratic = |x: f32, y: f32| {
        let (a, b) = corner(x, y);
        combine_profiled(Operation::SmoothUnion, BlendProfile::Quadratic, a, b, r)
    };
    assert!(
        quadratic(-bulge, -bulge) > 0.0,
        "quadratic stops short of the arc"
    );
    assert_eq!(quadratic(-2.0, -2.0).signum(), joint(-2.0, -2.0).signum());
}

#[test]
fn a_circular_subtraction_rounds_the_cut_edge() {
    let r = 6.0;
    // Everything (-1e6 inside) minus the half-plane y < 0 minus x < 0: the
    // quadrant x > 0, y > 0 with its outside corner rounded to radius r.
    let everywhere = -1e6;
    let cut = |x: f32, y: f32| {
        let without_floor = combine_profiled(
            Operation::Subtract,
            BlendProfile::Circular,
            everywhere,
            y,
            0.0,
        );
        combine_profiled(
            Operation::SmoothSubtract,
            BlendProfile::Circular,
            without_floor,
            x,
            r,
        )
    };
    // The corner itself is outside now; the arc centred at (r, r) is the edge.
    assert!(cut(0.5, 0.5) > 0.0);
    let diagonal = r - r / std::f32::consts::SQRT_2;
    assert!(
        cut(diagonal, diagonal).abs() < 1e-3,
        "{}",
        cut(diagonal, diagonal)
    );
    // Far along either edge, the edge is where it was.
    assert!((cut(0.0, 50.0)).abs() < 1e-4);
    assert!((cut(50.0, 0.0)).abs() < 1e-4);
}

#[test]
fn the_profiles_agree_outside_the_seam_and_on_hard_operations() {
    // Outside the band: at least one shape further than the radius.
    for (a, b) in [(-3.0, 40.0), (25.0, 70.0), (-50.0, 20.0)] {
        assert_eq!(
            combine_profiled(Operation::SmoothUnion, BlendProfile::Circular, a, b, 8.0),
            a.min(b)
        );
        assert_eq!(
            combine_profiled(Operation::Union, BlendProfile::Circular, a, b, 8.0),
            combine(Operation::Union, a, b, 8.0)
        );
    }
    assert_eq!(
        BlendProfile::parse("circular"),
        Some(BlendProfile::Circular)
    );
    assert_eq!(BlendProfile::parse("cubic"), None);
}
