//! `ui.follow`: one property kept on another node's, every tick.

use morf_scene::NodeHandle;

use super::AnimationHost;

/// One property that follows another node's, as `ui.follow` asked: the
/// target's `property` is `clamp(source's * scale + offset, min, max)`,
/// worked out in the same tick the source moves -- a thing riding out with a
/// sliding panel sits where the panel is on every frame, not where a second
/// animation guesses it will be.
#[derive(Clone, Debug)]
pub struct Follow {
    pub target: NodeHandle,
    pub property: String,
    pub source: NodeHandle,
    pub source_property: String,
    pub scale: f64,
    pub offset: f64,
    pub min: f64,
    pub max: f64,
}

/// Brings every following property up to its source. How many moved.
pub fn apply_follows(host: &mut dyn AnimationHost) -> usize {
    if host.animation().follows.is_empty() {
        return 0;
    }
    let follows = host.animation().follows.clone();
    let mut moved = 0;
    let mut gone = Vec::new();
    for (index, follow) in follows.iter().enumerate() {
        if !host.scene().contains(follow.target) || !host.scene().contains(follow.source) {
            gone.push(index);
            continue;
        }
        let Ok(source) = host.scene().number(follow.source, &follow.source_property) else {
            continue;
        };
        let wanted = (source * follow.scale + follow.offset).clamp(follow.min, follow.max);
        let now = host
            .scene()
            .number(follow.target, &follow.property)
            .unwrap_or(f64::NAN);
        if ((wanted - now).abs() > 1e-3 || now.is_nan())
            && host
                .assign(
                    follow.target,
                    &follow.property,
                    morf_scene::Value::Number(wanted),
                )
                .is_ok()
        {
            moved += 1;
        }
    }
    let follows = &mut host.animation().follows;
    for index in gone.into_iter().rev() {
        follows.remove(index);
    }
    moved
}
