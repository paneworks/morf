//! Pairing loops and resampling them, the two halves of a morph.

use super::*;

fn square(x: f32, y: f32, side: f32, clockwise: bool) -> Vec<Step> {
    let mut corners = vec![(x, y), (x + side, y), (x + side, y + side), (x, y + side)];
    if clockwise {
        corners.reverse();
    }
    let mut steps = vec![Step::Move(corners[0].0, corners[0].1)];
    steps.extend(corners[1..].iter().map(|&(x, y)| Step::Line(x, y)));
    steps.push(Step::Close);
    steps
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

#[test]
fn resampling_spaces_points_evenly_and_keeps_the_count() {
    let loop_ = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    let points = resample(&loop_, 8, &[]).expect("a closed loop");
    assert_eq!(points.len(), 8);
    // 40 around, so a point every 5 starting from the first corner.
    let expected = [
        (0.0, 0.0),
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (10.0, 10.0),
        (5.0, 10.0),
        (0.0, 10.0),
        (0.0, 5.0),
    ];
    for (got, want) in points.iter().zip(expected) {
        assert!(near(*got, want), "{got:?} against {want:?}");
    }
}

#[test]
fn resampling_refuses_what_is_not_a_loop() {
    assert_eq!(resample(&[(0.0, 0.0), (1.0, 1.0)], 8, &[]), None);
    let collapsed = [(2.0, 2.0); 5];
    assert_eq!(resample(&collapsed, 8, &[]), None, "no length to walk");
}

#[test]
fn a_corner_pulls_its_nearest_point_onto_it() {
    let loop_ = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    // Seven points do not land on the corner at (10, 10) by spacing alone.
    let points = resample(&loop_, 7, &[(10.0, 10.0)]).expect("a closed loop");
    assert_eq!(points.len(), 7, "snapping keeps the count");
    assert!(points.iter().any(|point| near(*point, (10.0, 10.0))));
}

#[test]
fn a_shape_paired_with_itself_does_not_move() {
    let shape = square(0.0, 0.0, 10.0, false);
    let paired = pair_up(contours(&shape), contours(&shape));
    assert_eq!(paired.len(), 1);
    let start = walk(&paired, 0.0);
    let half = walk(&paired, 0.5);
    for (a, b) in start.iter().zip(&half) {
        assert!(near(*a, *b), "{a:?} moved to {b:?}");
    }
}

#[test]
fn pairing_lines_the_loops_up_rather_than_spinning_one() {
    // The same square, its outline started from another corner: the best
    // rotation puts each point over itself, so nothing travels.
    let from = square(0.0, 0.0, 10.0, false);
    let to = vec![
        Step::Move(10.0, 10.0),
        Step::Line(0.0, 10.0),
        Step::Line(0.0, 0.0),
        Step::Line(10.0, 0.0),
        Step::Close,
    ];
    let paired = pair_up(contours(&from), contours(&to));
    let start = walk(&paired, 0.0);
    let end = walk(&paired, 1.0);
    let travel: f32 = start
        .iter()
        .zip(&end)
        .map(|(a, b)| (a.0 - b.0).hypot(a.1 - b.1))
        .fold(0.0, f32::max);
    assert!(travel < 0.5, "a point travelled {travel}");
}

#[test]
fn a_hole_is_never_paired_with_a_body() {
    // Body and hole on one side, a lone body on the other: the hole has no
    // partner, so it collapses to its own centre rather than becoming a body.
    let mut ring = square(0.0, 0.0, 20.0, false);
    ring.extend(square(6.0, 6.0, 8.0, true));
    let solid = square(0.0, 0.0, 20.0, false);
    let paired = pair_up(contours(&ring), contours(&solid));
    assert_eq!(paired.len(), 2);
    let end = walk(&paired, 1.0);
    let hole = &end[CONTOUR_POINTS..];
    let first = hole[0];
    assert!(
        hole.iter().all(|point| near(*point, first)),
        "the hole closes to a point"
    );
    assert!(near(first, (10.0, 10.0)), "at its own centre, {first:?}");
}

#[test]
fn a_loop_appearing_grows_from_its_centre() {
    let solid = square(0.0, 0.0, 20.0, false);
    let mut two = solid.clone();
    two.extend(square(30.0, 0.0, 10.0, false));
    let paired = pair_up(contours(&solid), contours(&two));
    assert_eq!(paired.len(), 2);
    let start = walk(&paired, 0.0);
    let grown = &start[CONTOUR_POINTS..];
    assert!(grown.iter().all(|point| near(*point, (35.0, 5.0))));
}

#[test]
fn between_draws_closed_loops_and_skips_collapsed_ones() {
    let solid = square(0.0, 0.0, 20.0, false);
    let mut two = solid.clone();
    two.extend(square(30.0, 0.0, 10.0, false));
    let paired = pair_up(contours(&solid), contours(&two));
    // At the start the second loop is one point: no edges of its own.
    let start = between(&paired, 0.0);
    let end = between(&paired, 1.0);
    assert!(end.len() > start.len());
    for segments in [&start, &end] {
        let first = &segments[0];
        let closing = segments
            .iter()
            .find(|segment| near((segment.x1, segment.y1), (first.x0, first.y0)));
        assert!(closing.is_some(), "the first loop closes on itself");
    }
}

#[test]
fn nothing_paired_with_nothing_is_nothing() {
    assert!(pair_up(Vec::new(), Vec::new()).is_empty());
    assert!(walk(&[], 0.5).is_empty());
    assert!(between(&[], 0.5).is_empty());
}
