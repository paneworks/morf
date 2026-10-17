//! A draw list that copies unchanged subtrees from the last frame against
//! one built afresh, on random scenes under random changes: the two must be
//! the same, command for command and layer for layer, every frame.

use morf_layout::{Geometry, Layout, Size};
use morf_scene::Scene;

use crate::DrawList;
use crate::damage::{DamageTracker, RenderEngine};
use crate::test_support::reuse::{Rng, make, mutate};
use crate::test_support::{NoText, RecordingBackend};

#[test]
fn a_kept_draw_list_matches_one_built_afresh() {
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    for seed in 1..=150u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut scene = Scene::new();
        let root = make(&mut scene, &mut rng, 4);
        let mut engine = RenderEngine::new(RecordingBackend::default());
        for step in 0..40 {
            if step > 0 {
                for _ in 0..1 + rng.below(3) {
                    mutate(&mut scene, &mut rng, root);
                }
            }
            if rng.chance(3) {
                engine.forget();
            }
            let Ok(layout) = Layout::compute(&scene, root, size, &mut NoText) else {
                continue;
            };
            let fresh = DrawList::from_scene(&scene, &layout);
            let kept = engine.render(&scene, &layout, 120, |_| {});
            match (fresh, kept) {
                (Ok(fresh), Ok(_)) => assert!(
                    *engine.last_list() == fresh,
                    "seed {seed}, step {step}: the kept list differs from a fresh one"
                ),
                (Err(_), Err(_)) => {}
                (fresh, kept) => panic!(
                    "seed {seed}, step {step}: fresh {:?}, kept {:?}",
                    fresh.err(),
                    kept.err()
                ),
            }
        }
    }
}

fn sorted(mut changed: Vec<(Geometry, usize)>) -> Vec<[u64; 5]> {
    let mut out: Vec<[u64; 5]> = changed
        .drain(..)
        .map(|(bounds, order)| {
            [
                bounds.x.to_bits(),
                bounds.y.to_bits(),
                bounds.width.to_bits(),
                bounds.height.to_bits(),
                order as u64,
            ]
        })
        .collect();
    out.sort_unstable();
    out
}

#[test]
fn copied_runs_hold_what_they_copied_and_damage_as_a_whole_diff() {
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    let mut compared = 0;
    for seed in 1..=150u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut scene = Scene::new();
        let root = make(&mut scene, &mut rng, 4);
        let mut engine = RenderEngine::new(RecordingBackend::default());
        let mut tracker = DamageTracker::default();
        for step in 0..40 {
            if step > 0 {
                for _ in 0..1 + rng.below(3) {
                    mutate(&mut scene, &mut rng, root);
                }
            }
            if rng.chance(3) {
                engine.forget();
                tracker.forget();
            }
            let Ok(layout) = Layout::compute(&scene, root, size, &mut NoText) else {
                continue;
            };
            let before = engine.last_list().clone();
            if engine.render(&scene, &layout, 120, |_| {}).is_err() {
                continue;
            }
            let mut list = engine.last_list().clone();
            let runs = engine.copies().runs.to_vec();
            for run in &runs {
                assert!(
                    list.commands[run.to..run.to + run.len]
                        == before.commands[run.from..run.from + run.len],
                    "seed {seed}, step {step}: {run:?} is not what it copied"
                );
            }
            let (fast, full) = tracker.both_changes(&list, &runs);
            if let Some((keys, changed)) = fast {
                compared += 1;
                assert_eq!(keys, full.0, "seed {seed}, step {step}: keys");
                assert_eq!(
                    sorted(changed),
                    sorted(full.1),
                    "seed {seed}, step {step}: damage"
                );
            }
            tracker.diff_copied(&list, &runs, 120);
            tracker.retain(&mut list);
        }
    }
    assert!(
        compared > 1000,
        "only {compared} frames diffed between runs"
    );
}

#[test]
fn an_unchanged_frame_copies_every_command() {
    let mut rng = Rng(7);
    let mut scene = Scene::new();
    let root = make(&mut scene, &mut rng, 4);
    let size = Size {
        width: 200.0,
        height: 160.0,
    };
    let layout = Layout::compute(&scene, root, size, &mut NoText).unwrap();
    let mut engine = RenderEngine::new(RecordingBackend::default());
    engine.render(&scene, &layout, 120, |_| {}).unwrap();
    let first = engine.last_list().clone();
    let damage = engine.render(&scene, &layout, 120, |_| {}).unwrap();
    assert!(damage.is_empty());
    assert!(*engine.last_list() == first);
}
