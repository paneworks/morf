//! Velocity at the end of a touch history, in logical pixels per second.
//!
//! Like Android's planar VelocityTracker, fit a quadratic to recent input
//! samples, rather than timing the callbacks or averaging the whole drag.
//! Event timestamps must survive batching; an UP isn't a stationary MOVE.

use std::{collections::VecDeque, time::Duration};

const HORIZON: Duration = Duration::from_millis(100);
const STOPPED: Duration = Duration::from_millis(40);
const MAX_SAMPLES: usize = 20;
const MAX_SPEED: f64 = 8_000.0;

#[derive(Default)]
pub struct VelocityTracker {
    samples: VecDeque<(Duration, f64, f64)>,
}

impl VelocityTracker {
    pub fn add(&mut self, time: Duration, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if let Some(&(previous, _, _)) = self.samples.back() {
            if time < previous || time.saturating_sub(previous) > STOPPED {
                self.samples.clear();
            } else if time == previous {
                // Several samples may share a millisecond, but they must not
                // give that instant extra weight or divide by zero.
                self.samples.pop_back();
            }
        }
        self.samples.push_back((time, x, y));
        while self.samples.len() > MAX_SAMPLES
            || self
                .samples
                .front()
                .is_some_and(|s| time.saturating_sub(s.0) > HORIZON)
        {
            self.samples.pop_front();
        }
    }

    pub fn velocity(&self, now: Duration) -> (f64, f64) {
        let Some(&(last, x, y)) = self.samples.back() else {
            return (0.0, 0.0);
        };
        if self.samples.len() < 2 || now.saturating_sub(last) >= STOPPED {
            return (0.0, 0.0);
        }
        // Normalize both time and position before solving. This keeps the
        // fit stable on long-running sessions and large screen coordinates.
        let mut sums = [0.0; 5];
        let mut axes = [[0.0; 3]; 2];
        for &(time, px, py) in &self.samples {
            let t = -last.saturating_sub(time).as_secs_f64() / HORIZON.as_secs_f64();
            let mut power = 1.0;
            for (i, sum) in sums.iter_mut().enumerate() {
                *sum += power;
                if i < 3 {
                    axes[0][i] += power * (px - x);
                    axes[1][i] += power * (py - y);
                }
                power *= t;
            }
        }
        let fit = |values: [f64; 3]| {
            let linear = || {
                let determinant = sums[0] * sums[2] - sums[1] * sums[1];
                if determinant.abs() < 1e-10 {
                    0.0
                } else {
                    (sums[0] * values[1] - sums[1] * values[0]) / determinant
                }
            };
            let slope = if self.samples.len() >= 3 {
                quadratic(sums, values).unwrap_or_else(linear)
            } else {
                linear()
            };
            (slope / HORIZON.as_secs_f64()).clamp(-MAX_SPEED, MAX_SPEED)
        };
        (fit(axes[0]), fit(axes[1]))
    }
}

fn quadratic(sums: [f64; 5], values: [f64; 3]) -> Option<f64> {
    let mut matrix = std::array::from_fn::<_, 3, _>(|row| {
        [sums[row], sums[row + 1], sums[row + 2], values[row]]
    });
    for column in 0..3 {
        let pivot = (column..3)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))?;
        matrix.swap(column, pivot);
        let divisor = matrix[column][column];
        if divisor.abs() < 1e-10 {
            return None;
        }
        for value in &mut matrix[column][column..] {
            *value /= divisor;
        }
        let pivot_row = matrix[column];
        for (index, row) in matrix.iter_mut().enumerate() {
            if index == column {
                continue;
            }
            let factor = row[column];
            for (value, pivot_value) in row[column..].iter_mut().zip(&pivot_row[column..]) {
                *value -= factor * pivot_value;
            }
        }
    }
    Some(matrix[1][3])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(time: u64) -> Duration {
        Duration::from_millis(time)
    }

    #[test]
    fn speed_is_independent_of_sampling_rate_and_coordinate_origin() {
        for interval in [8, 16, 33] {
            let mut tracker = VelocityTracker::default();
            for t in (0..=198).step_by(interval) {
                tracker.add(ms(t), 10_000.0 + t as f64 * 1.2, 500.0 - t as f64 * 0.7);
            }
            let now = tracker.samples.back().unwrap().0;
            let (vx, vy) = tracker.velocity(now);
            assert!((vx - 1200.0).abs() < 0.01, "{interval}: {vx}");
            assert!((vy + 700.0).abs() < 0.01, "{interval}: {vy}");
        }
    }

    #[test]
    fn acceleration_and_reversal_use_release_speed_not_average_speed() {
        let mut tracker = VelocityTracker::default();
        for t in [0, 12, 28, 43, 60, 76, 91, 100] {
            let seconds = t as f64 / 1000.0;
            tracker.add(ms(t), 1000.0 * seconds - 10_000.0 * seconds * seconds, 0.0);
        }
        assert!((tracker.velocity(ms(100)).0 + 1000.0).abs() < 0.01);
    }

    #[test]
    fn duplicate_timestamps_replace_samples_and_release_does_not_dilute_speed() {
        let mut tracker = VelocityTracker::default();
        tracker.add(ms(0), 0.0, 0.0);
        tracker.add(ms(10), 5.0, 0.0);
        tracker.add(ms(10), 10.0, 0.0);
        assert_eq!(tracker.samples.len(), 2);
        assert!((tracker.velocity(ms(20)).0 - 1000.0).abs() < 0.01);
        assert_eq!(tracker.velocity(ms(50)), (0.0, 0.0));
    }

    #[test]
    fn a_pause_starts_a_fresh_history_and_invalid_coordinates_are_ignored() {
        let mut tracker = VelocityTracker::default();
        tracker.add(ms(0), 0.0, 0.0);
        tracker.add(ms(20), 20.0, 0.0);
        tracker.add(ms(100), 20.0, 0.0);
        assert_eq!(tracker.velocity(ms(100)), (0.0, 0.0));
        tracker.add(ms(110), 10.0, 0.0);
        tracker.add(ms(111), f64::NAN, f64::INFINITY);
        assert!((tracker.velocity(ms(110)).0 + 1000.0).abs() < 0.01);
    }

    #[test]
    fn history_is_bounded_and_extreme_speed_is_clamped() {
        let mut tracker = VelocityTracker::default();
        for t in 0..1000 {
            tracker.add(ms(t), t as f64 * 100.0, 0.0);
        }
        assert_eq!(tracker.samples.len(), MAX_SAMPLES);
        assert_eq!(tracker.velocity(ms(999)).0, MAX_SPEED);
    }
}
