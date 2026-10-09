//! Touch scrolling using Android OverScroller's velocity/distance relation
//! and spline (frameworks/base/core/java/android/widget/OverScroller.java).
//! Coordinates are logical pixels, so the physical coefficient uses 160 dpi.
use std::time::Duration;

const INFLEXION: f64 = 0.35;
const FRICTION: f64 = 0.015;
const PHYSICAL_COEFF: f64 = 9.80665 * 39.37 * 160.0 * 0.84;

#[derive(Clone, Debug)]
pub(crate) struct ScrollFling {
    start: f64,
    distance: f64,
    duration: f64,
    elapsed: f64,
    bounds: (f64, f64),
    pub(crate) position: f64,
    pub(crate) velocity: f64,
}

impl ScrollFling {
    pub(crate) fn new(start: f64, velocity: f64, bounds: (f64, f64)) -> Option<Self> {
        if !start.is_finite()
            || !velocity.is_finite()
            || !bounds.0.is_finite()
            || !bounds.1.is_finite()
            || bounds.0 > bounds.1
        {
            return None;
        }
        let rate = 0.78_f64.ln() / 0.9_f64.ln();
        let duration = if velocity == 0.0 {
            0.0
        } else {
            (INFLEXION * velocity.abs() / (FRICTION * PHYSICAL_COEFF)).powf(1.0 / (rate - 1.0))
        };
        // Equivalent to Android's exponential distance formula. This also
        // guarantees the curve's initial derivative equals release velocity.
        let distance = INFLEXION * velocity * duration;
        if !duration.is_finite() || !(start + distance).is_finite() {
            return None;
        }
        let start = start.clamp(bounds.0, bounds.1);
        Some(Self {
            start,
            distance,
            duration,
            elapsed: 0.0,
            bounds,
            position: start,
            velocity,
        })
    }

    pub(crate) fn advance(&mut self, delta: Duration) -> bool {
        self.elapsed = (self.elapsed + delta.as_secs_f64()).min(self.duration);
        let time = if self.duration > 0.0 {
            self.elapsed / self.duration
        } else {
            1.0
        };
        let (position, slope) = spline(time);
        let unbounded = self.start + self.distance * position;
        self.position = unbounded.clamp(self.bounds.0, self.bounds.1);
        let finished = time >= 1.0
            || unbounded <= self.bounds.0 && self.distance < 0.0
            || unbounded >= self.bounds.1 && self.distance > 0.0;
        self.velocity = if finished {
            0.0
        } else {
            self.distance / self.duration * slope
        };
        finished
    }
}

// Evaluate the same curve Android samples into SPLINE_POSITION, without
// lookup-table quantization: cubic Bezier (.175, .5), (.35, 1).
// Inverting x makes progress a function of time, not the Bezier parameter.
fn spline(time: f64) -> (f64, f64) {
    let (mut low, mut high) = (0.0, 1.0);
    let mut t = time;
    for _ in 0..24 {
        let x = cubic(t, 0.175, 0.35);
        if (x - time).abs() < 1e-10 {
            break;
        }
        if x < time {
            low = t;
        } else {
            high = t;
        }
        t = (low + high) * 0.5;
    }
    (
        cubic(t, 0.5, 1.0),
        derivative(t, 0.5, 1.0) / derivative(t, 0.175, 0.35),
    )
}

fn cubic(t: f64, a: f64, b: f64) -> f64 {
    3.0 * t * (1.0 - t) * ((1.0 - t) * a + t * b) + t * t * t
}

fn derivative(t: f64, a: f64, b: f64) -> f64 {
    3.0 * ((1.0 - t).powi(2) * a + 2.0 * (1.0 - t) * t * (b - a) + t * t * (1.0 - b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fling(speed: f64) -> ScrollFling {
        ScrollFling::new(0.0, speed, (-100_000.0, 100_000.0)).unwrap()
    }

    #[test]
    fn android_distance_and_duration_scale_with_release_speed() {
        let slow = fling(1000.0);
        let fast = fling(2000.0);
        assert!((slow.distance - 194.31).abs() < 0.1);
        assert!((slow.duration - 0.5552).abs() < 0.001);
        assert!(fast.distance > slow.distance * 3.3);
        assert!(fast.duration > slow.duration * 1.6);
        assert_eq!(fling(-2000.0).distance, -fast.distance);
    }

    #[test]
    fn starts_at_release_speed_and_decelerates_to_rest() {
        let mut motion = fling(2000.0);
        motion.advance(Duration::ZERO);
        assert!((motion.velocity - 2000.0).abs() < 1e-6);
        let mut previous = motion.velocity;
        for _ in 0..200 {
            let finished = motion.advance(Duration::from_millis(8));
            assert!(motion.velocity >= 0.0 && motion.velocity <= previous + 1e-6);
            previous = motion.velocity;
            if finished {
                assert!((motion.position - motion.distance).abs() < 1e-6);
                assert_eq!(motion.velocity, 0.0);
                return;
            }
        }
        panic!("fling never settled");
    }

    #[test]
    fn delayed_and_batched_frames_follow_the_same_trajectory() {
        let mut reference = fling(2000.0);
        reference.advance(Duration::from_millis(480));
        for step in [8, 16, 24, 40, 120, 480] {
            let mut motion = fling(2000.0);
            for _ in 0..480 / step {
                motion.advance(Duration::from_millis(step));
            }
            assert!((motion.position - reference.position).abs() < 1e-5);
            assert!((motion.velocity - reference.velocity).abs() < 1e-5);
            assert!(motion.advance(Duration::from_secs(2)));
            assert!((motion.position - reference.distance).abs() < 1e-6);
        }
    }

    #[test]
    fn bounds_stop_without_reversing_or_compressing_the_curve() {
        for direction in [-1.0, 1.0] {
            let mut motion = ScrollFling::new(50.0, direction * 2000.0, (0.0, 100.0)).unwrap();
            assert!(!motion.advance(Duration::from_millis(8)));
            assert!((motion.position - 50.0).abs() > 15.0);
            assert!(motion.advance(Duration::from_millis(80)));
            assert_eq!(motion.position, if direction > 0.0 { 100.0 } else { 0.0 });
            assert_eq!(motion.velocity, 0.0);
        }
        assert!(fling(0.0).advance(Duration::ZERO));
        assert!(ScrollFling::new(0.0, f64::NAN, (0.0, 100.0)).is_none());
    }

    #[test]
    fn a_scroll_keeps_the_display_clock_running_until_caught_or_finished() {
        use crate::{Element, Scene};
        let mut scene = Scene::new();
        let node = scene.create(Element::Flickable);
        scene
            .fling_scroll(node, "content_y", 2000.0, (0.0, 5000.0))
            .unwrap();
        assert!(
            scene.has_motion(),
            "a fling must request its first display frame"
        );
        scene.tick_animations(Duration::from_millis(80)).unwrap();
        assert!(scene.has_motion());
        let caught = scene.number(node, "content_y").unwrap();
        scene.stop_animation(node, "content_y").unwrap();
        scene.tick_animations(Duration::from_secs(1)).unwrap();
        assert_eq!(scene.number(node, "content_y").unwrap(), caught);
        assert!(!scene.has_motion());
        scene
            .fling_scroll(node, "content_y", 2000.0, (0.0, 5000.0))
            .unwrap();
        let mut frames = 0;
        while scene.has_motion() {
            scene.tick_animations(Duration::from_millis(16)).unwrap();
            frames += 1;
            assert!(frames < 100);
        }
        assert!(scene.number(node, "content_y").unwrap() > caught + 600.0);
    }
}
