//! `ui.follow`: one property kept on another node's, every tick.

use super::*;

/// One property that follows another node's, as `ui.follow` asked: the
/// target's `property` is `clamp(source's * scale + offset, min, max)`,
/// worked out in the same tick the source moves -- a thing riding out with a
/// sliding panel sits where the panel is on every frame, not where a second
/// animation guesses it will be.
#[derive(Clone, Debug)]
pub(crate) struct Follow {
    pub(crate) target: NodeHandle,
    pub(crate) property: String,
    pub(crate) source: NodeHandle,
    pub(crate) source_property: String,
    pub(crate) scale: f64,
    pub(crate) offset: f64,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

/// Brings every following property up to its source. How many moved.
pub(crate) fn apply_follows(state: &mut ReactiveState) -> usize {
    if state.follows.is_empty() {
        return 0;
    }
    let follows = state.follows.clone();
    let mut moved = 0;
    let mut gone = Vec::new();
    for (index, follow) in follows.iter().enumerate() {
        if !state.scene.contains(follow.target) || !state.scene.contains(follow.source) {
            gone.push(index);
            continue;
        }
        let Ok(source) = state.scene.number(follow.source, &follow.source_property) else {
            continue;
        };
        let wanted = (source * follow.scale + follow.offset).clamp(follow.min, follow.max);
        let now = state
            .scene
            .number(follow.target, &follow.property)
            .unwrap_or(f64::NAN);
        if (wanted - now).abs() > 1e-3 || now.is_nan() {
            if crate::scene_bindings::assign_scene_property(
                state,
                follow.target,
                &follow.property,
                morf_scene::Value::Number(wanted),
            )
            .is_ok()
            {
                moved += 1;
            }
        }
    }
    for index in gone.into_iter().rev() {
        state.follows.remove(index);
    }
    moved
}
