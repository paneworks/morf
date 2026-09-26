use crate::pacing::FramePacer;
use std::time::Duration;

const REFRESH: Duration = Duration::from_micros(16_667);

#[test]
fn a_surface_that_keeps_up_paints_on_every_callback() {
    let mut pacer = FramePacer::new();
    // Nothing measured yet: take every callback until there is evidence.
    assert_eq!(pacer.interval(REFRESH), 1);
    assert!(pacer.due(REFRESH));

    pacer.observed(Duration::from_micros(4_000));
    assert_eq!(pacer.interval(REFRESH), 1);
    for _ in 0..10 {
        assert!(pacer.due(REFRESH), "a cheap frame never waits");
    }
}

#[test]
fn a_surface_that_cannot_keep_up_halves_its_rate_rather_than_missing_deadlines() {
    // The point of pacing: a frame that needs a refresh and a half will miss
    // every other deadline anyway. Choosing to paint every second callback
    // gives up the same frames and gets an even rhythm for them.
    let mut pacer = FramePacer::new();
    for _ in 0..12 {
        pacer.observed(Duration::from_micros(25_000));
    }
    assert_eq!(pacer.interval(REFRESH), 2);

    let painted: Vec<bool> = (0..8).map(|_| pacer.due(REFRESH)).collect();
    assert_eq!(
        painted,
        vec![true, false, true, false, true, false, true, false],
        "the first frame is taken, then an even cadence — not a burst and a gap"
    );
}

#[test]
fn the_cadence_follows_the_cost_and_is_bounded_at_both_ends() {
    let mut pacer = FramePacer::new();
    for _ in 0..20 {
        pacer.observed(Duration::from_micros(50_000));
    }
    assert_eq!(pacer.interval(REFRESH), 3, "three refreshes of work");

    // However slow it gets, it keeps painting sometimes.
    for _ in 0..20 {
        pacer.observed(Duration::from_secs(2));
    }
    assert_eq!(pacer.interval(REFRESH), 4, "capped rather than stopping");

    // And it recovers when the work does.
    for _ in 0..40 {
        pacer.observed(Duration::from_micros(2_000));
    }
    assert_eq!(pacer.interval(REFRESH), 1);
}

#[test]
fn a_frame_that_only_just_fits_does_not_halve_the_rate() {
    // Measurement is noisy and a refresh is not a hard wall. A frame a hair
    // over budget should keep trying for every callback rather than dropping
    // to half rate on the strength of a rounding error.
    let mut pacer = FramePacer::new();
    for _ in 0..20 {
        pacer.observed(REFRESH.mul_f64(1.05));
    }
    assert_eq!(pacer.interval(REFRESH), 1);

    // Clearly over, though, and it gives up the frame.
    for _ in 0..20 {
        pacer.observed(REFRESH.mul_f64(1.6));
    }
    assert_eq!(pacer.interval(REFRESH), 2);
}

#[test]
fn resting_clears_the_cadence_so_motion_restarts_on_the_next_callback() {
    // Between animations the surface is idle; when something moves again it
    // should paint at once rather than sit out callbacks it owed from before.
    let mut pacer = FramePacer::new();
    for _ in 0..12 {
        pacer.observed(Duration::from_micros(25_000));
    }
    assert!(pacer.due(REFRESH), "the first frame of motion is taken");
    assert!(!pacer.due(REFRESH), "mid-cadence");
    pacer.rest();
    assert!(
        pacer.due(REFRESH),
        "the first frame of new motion is not skipped"
    );
}

#[test]
fn a_hidden_surface_hands_the_clock_over_after_a_few_refreshes() {
    // A slow frame is not a hidden surface: a few refreshes first, bounded
    // so a very slow or very fast output still hands over in time.
    assert_eq!(crate::surfaces::frame_stall(REFRESH), REFRESH * 4);
    assert_eq!(
        crate::surfaces::frame_stall(Duration::from_millis(4)),
        Duration::from_millis(50)
    );
    assert_eq!(
        crate::surfaces::frame_stall(Duration::from_millis(100)),
        Duration::from_millis(250)
    );
}

#[test]
fn motion_that_lands_on_a_skipped_callback_is_still_painted() {
    // At half rate the last frame of an animation can fall on a callback the
    // cadence gives up. The scene has landed but the screen has not: resting
    // must say a paint is owed, or the surface stays on the frame before —
    // a gradient tile stuck between two colours.
    let mut pacer = FramePacer::new();
    for _ in 0..12 {
        pacer.observed(Duration::from_micros(25_000));
    }
    assert!(pacer.due(REFRESH), "painted");
    assert!(!pacer.due(REFRESH), "the landing frame is skipped");
    assert!(pacer.rest(), "so the next callback owes a paint");
    assert!(!pacer.rest(), "and only one");

    assert!(pacer.due(REFRESH), "painted");
    assert!(
        !pacer.rest(),
        "motion that ended on a painted frame owes nothing"
    );
}

#[test]
fn one_slow_frame_does_not_drop_the_rate() {
    // A layout of the whole tree now and then (25 ms, 100 ms) among cheap
    // frames: the cadence stays at every callback.
    let mut pacer = FramePacer::new();
    for cost in [4_000, 5_000, 100_000, 4_000, 6_000, 25_000, 5_000, 4_000] {
        pacer.observed(Duration::from_micros(cost));
        assert_eq!(pacer.interval(REFRESH), 1, "after a {cost} µs frame");
    }
}
