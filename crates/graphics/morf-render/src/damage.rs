use morf_layout::Geometry;

use crate::{CopiedRun, commands::*, effects::*};

mod copied;
mod engine;
mod explain;
mod keys;

pub use engine::{RenderBackend, RenderEngine};
use explain::damage_log_wanted;
use keys::{Key, command_changes, command_keys, keyed_commands, layer_changes, moved_commands};

/// Physical damage rectangle with an exclusive lower-right edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DamageRect {
    /// Left edge in physical pixels.
    pub x: u32,
    /// Top edge in physical pixels.
    pub y: u32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

/// Draw-list differ retaining the prior successful frame.
#[derive(Default)]
pub struct DamageTracker {
    previous: DrawList,
    scale_120: u32,
    /// The key of every command of `previous`, while they are known.
    keys: Vec<Key>,
    /// The keys of the list diffed last, for when it is retained.
    pending: Vec<Key>,
}

impl DamageTracker {
    /// Takes the frame just diffed, handing back the buffer it replaces.
    ///
    /// The alternative is what this replaces — `previous = next.clone()`, a
    /// deep copy of every command including its owned strings and layer
    /// vectors, once a frame. That is precisely the cost the surrounding code
    /// is built to avoid: `DrawList::rebuild` keeps its buffers between frames
    /// so their capacity is not returned to the allocator sixty times a second,
    /// and then the tracker copied the whole thing anyway. Swapping keeps two
    /// warm buffers in rotation and copies nothing.
    pub fn retain(&mut self, list: &mut DrawList) {
        std::mem::swap(&mut self.previous, list);
        std::mem::swap(&mut self.keys, &mut self.pending);
        if self.keys.len() != self.previous.commands.len() {
            self.keys.clear();
        }
        self.pending.clear();
    }

    /// Forgets what is on screen, so the next frame is damaged in full.
    ///
    /// For when the surface stops being what the last frame was painted into —
    /// a resize hands back a blank target, and every pixel the tracker believes
    /// is still there is gone with the old one.
    pub fn forget(&mut self) {
        self.previous = DrawList::default();
        self.scale_120 = 0;
        self.keys.clear();
        self.pending.clear();
    }

    /// Diffs commands and converts changed logical bounds at protocol scale in 120ths.
    pub fn diff(&mut self, next: &DrawList, scale_120: u32) -> Vec<DamageRect> {
        self.diff_copied(next, &[], scale_120)
    }

    /// Diffs as [`DamageTracker::diff`] does, told that `runs` of `next` were
    /// copied unchanged from the list retained last, so only the commands
    /// between them are compared. The damage is the same either way.
    pub fn diff_copied(
        &mut self,
        next: &DrawList,
        runs: &[CopiedRun],
        scale_120: u32,
    ) -> Vec<DamageRect> {
        if self.scale_120 != 0 && self.scale_120 != scale_120 {
            if damage_log_wanted() {
                eprintln!(
                    "damage: whole surface: scale {} -> {scale_120}",
                    self.scale_120
                );
            }
            self.scale_120 = scale_120;
            self.pending = command_keys(&next.commands);
            return merge_damage(
                next.commands
                    .iter()
                    .filter_map(|command| physical_damage(command.bounds(), scale_120))
                    .collect(),
            );
        }
        if self.keys.len() != self.previous.commands.len() {
            self.keys = command_keys(&self.previous.commands);
        }
        let (current_keys, changed) =
            copied::copied_changes((&self.previous, &self.keys), next, runs)
                .unwrap_or_else(|| self.changes(next));
        let mut logical: Vec<Geometry> = changed.iter().map(|(bounds, _)| *bounds).collect();
        // A change beneath frosted glass changes all of the glass: the blur
        // spreads it, so the damage has to be the whole panel and not only
        // the few pixels that moved underneath.
        for (order, command) in next.commands.iter().enumerate() {
            let Some(reach) = command.backdrop_reach() else {
                continue;
            };
            if changed
                .iter()
                .any(|(bounds, changed_order)| *changed_order < order && overlaps(*bounds, reach))
            {
                // And its antialiased rim, which the glass draws half a
                // pixel past its shape.
                logical.push(expand_geometry(command.bounds(), 1.0));
            }
        }
        self.scale_120 = scale_120;
        self.pending = current_keys;
        merge_damage(
            logical
                .into_iter()
                .filter_map(|geometry| physical_damage(geometry, scale_120))
                .collect(),
        )
    }

    /// The keys of `next`, and every area that changed from the retained
    /// list, with the paint order it was drawn at, so a frosted panel can
    /// tell a change beneath it from one on top of it.
    fn changes(&self, next: &DrawList) -> (Vec<Key>, Vec<(Geometry, usize)>) {
        let current_keys = command_keys(&next.commands);
        let previous = keyed_commands(&self.previous.commands, &self.keys);
        let current = keyed_commands(&next.commands, &current_keys);
        // Which commands changed places with another, rather than shifting
        // along because something was added or taken away before them: a
        // swell put into a fullscreen frame moved every command after it one
        // place on, and each was repainted whole for it.
        let moved = moved_commands(&current_keys, &previous);
        let mut changed = Vec::new();
        layer_changes(
            (&self.previous, &self.keys),
            (next, &current_keys),
            &mut changed,
        );
        command_changes(&current, &previous, &moved, &mut changed);
        (current_keys, changed)
    }

    /// What comparing only between `runs` makes of `next`, if it can, and
    /// what comparing the whole lists does.
    #[cfg(test)]
    pub(crate) fn both_changes(
        &mut self,
        next: &DrawList,
        runs: &[CopiedRun],
    ) -> (Option<Changes>, Changes) {
        if self.keys.len() != self.previous.commands.len() {
            self.keys = command_keys(&self.previous.commands);
        }
        (
            copied::copied_changes((&self.previous, &self.keys), next, runs),
            self.changes(next),
        )
    }
}

/// A list's command keys and the areas that changed, with their paint order.
#[cfg(test)]
pub(crate) type Changes = (Vec<(morf_scene::NodeHandle, u32)>, Vec<(Geometry, usize)>);

fn overlaps(left: Geometry, right: Geometry) -> bool {
    left.width > 0.0
        && left.height > 0.0
        && right.width > 0.0
        && right.height > 0.0
        && left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}
