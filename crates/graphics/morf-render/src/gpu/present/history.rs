//! The damage of the last frames, and what a buffer coming back into use has
//! to have copied into it.

use std::collections::VecDeque;

use crate::DamageRect;

use super::super::targets::{intersect_damage, union_damage};

/// How many frames of damage are remembered. A buffer that missed more than
/// this is copied whole; with two or three buffers in rotation it never has.
const HISTORY: usize = 16;

/// Past this many rectangles a repaint is copied as their bounds: each is a
/// draw, and a hundred small ones cost more than one that covers them.
const MAX_RECTS: usize = 16;

/// The damage of the last frames, by frame number.
#[derive(Debug, Default)]
pub(crate) struct DamageHistory {
    /// The number of the latest frame; the first is 1.
    frame: u64,
    frames: VecDeque<(u64, Vec<DamageRect>)>,
}

impl DamageHistory {
    /// Records a frame's damage and returns its number.
    pub(crate) fn record(&mut self, damage: &[DamageRect]) -> u64 {
        self.frame += 1;
        self.frames.push_back((self.frame, damage.to_vec()));
        while self.frames.len() > HISTORY {
            self.frames.pop_front();
        }
        self.frame
    }

    /// The latest frame's number.
    pub(crate) fn frame(&self) -> u64 {
        self.frame
    }

    /// What changed after frame `painted`, up to the latest: `None` when the
    /// buffer has to be copied whole -- it was never painted, or the history
    /// no longer reaches back to it.
    pub(crate) fn since(&self, painted: Option<u64>) -> Option<Vec<DamageRect>> {
        let painted = painted?;
        if painted >= self.frame {
            return Some(Vec::new());
        }
        let oldest = self.frames.front()?.0;
        if painted + 1 < oldest {
            return None;
        }
        Some(
            self.frames
                .iter()
                .filter(|(frame, _)| *frame > painted)
                .flat_map(|(_, rects)| rects.iter().copied())
                .collect(),
        )
    }
}

/// What to copy into a buffer, in surface pixels: `None` is all of it.
pub(crate) fn repaint(
    history: &DamageHistory,
    painted: Option<u64>,
    size: (u32, u32),
) -> Option<Vec<DamageRect>> {
    let whole = DamageRect {
        x: 0,
        y: 0,
        width: size.0,
        height: size.1,
    };
    let mut rects: Vec<DamageRect> = history
        .since(painted)?
        .into_iter()
        .filter_map(|rect| intersect_damage(rect, whole))
        .collect();
    // Rectangles one inside another are one rectangle.
    rects.sort_by_key(|rect| std::cmp::Reverse(u64::from(rect.width) * u64::from(rect.height)));
    let mut kept: Vec<DamageRect> = Vec::with_capacity(rects.len());
    for rect in rects {
        if !kept.iter().any(|seen| contains(*seen, rect)) {
            kept.push(rect);
        }
    }
    if kept.len() > MAX_RECTS {
        let bounds = kept
            .iter()
            .copied()
            .reduce(union_damage)
            .expect("more than none");
        kept = vec![bounds];
    }
    Some(kept)
}

fn contains(outer: DamageRect, inner: DamageRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> DamageRect {
        DamageRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_new_buffer_is_painted_whole() {
        let mut history = DamageHistory::default();
        history.record(&[rect(0, 0, 4, 4)]);
        assert_eq!(repaint(&history, None, (100, 100)), None);
    }

    #[test]
    fn a_buffer_catches_up_on_every_frame_it_missed() {
        let mut history = DamageHistory::default();
        let first = history.record(&[rect(0, 0, 4, 4)]);
        history.record(&[rect(10, 10, 4, 4)]);
        history.record(&[rect(20, 20, 4, 4)]);
        let mut rects = repaint(&history, Some(first), (100, 100)).unwrap();
        rects.sort_by_key(|rect| rect.x);
        assert_eq!(rects, vec![rect(10, 10, 4, 4), rect(20, 20, 4, 4)]);
        // The buffer that showed the latest frame has nothing to catch up.
        assert_eq!(
            repaint(&history, Some(history.frame()), (100, 100)),
            Some(vec![])
        );
    }

    #[test]
    fn a_buffer_older_than_the_history_is_painted_whole() {
        let mut history = DamageHistory::default();
        let first = history.record(&[rect(0, 0, 1, 1)]);
        for _ in 0..=HISTORY {
            history.record(&[rect(1, 1, 1, 1)]);
        }
        assert_eq!(repaint(&history, Some(first), (10, 10)), None);
        assert!(repaint(&history, Some(first + 1), (10, 10)).is_some());
    }

    #[test]
    fn rectangles_inside_others_are_dropped_and_many_become_one() {
        let mut history = DamageHistory::default();
        let start = history.record(&[]);
        history.record(&[rect(0, 0, 50, 50), rect(10, 10, 5, 5)]);
        assert_eq!(
            repaint(&history, Some(start), (100, 100)),
            Some(vec![rect(0, 0, 50, 50)])
        );
        let start = history.record(&[]);
        let many: Vec<_> = (0..MAX_RECTS as u32 + 1)
            .map(|index| rect(index * 3, 0, 2, 2))
            .collect();
        history.record(&many);
        assert_eq!(
            repaint(&history, Some(start), (100, 100)),
            Some(vec![rect(0, 0, MAX_RECTS as u32 * 3 + 2, 2)])
        );
    }

    #[test]
    fn damage_past_the_edge_is_cut_to_the_buffer() {
        let mut history = DamageHistory::default();
        let start = history.record(&[]);
        history.record(&[rect(90, 90, 20, 20), rect(200, 200, 5, 5)]);
        assert_eq!(
            repaint(&history, Some(start), (100, 100)),
            Some(vec![rect(90, 90, 10, 10)])
        );
    }
}
