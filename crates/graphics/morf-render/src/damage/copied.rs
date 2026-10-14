//! Diffing a list most of which was copied from the last one.
//!
//! A run of commands copied from the last list is equal to what it was
//! copied from, and keyed the same: a copied subtree holds every command its
//! nodes made, so no node counts commands both inside a run and outside one.
//! What is left to compare is what lies between the runs. Moves are the one
//! thing a run can hide, so when the runs and the commands between them do
//! not all keep their old order, this gives up and the whole lists are
//! compared.

use morf_layout::Geometry;
use morf_scene::NodeHandle;
use std::collections::{HashMap, HashSet};

use super::keys::{Key, Keyed, command_changes, layer_changes, next_key};
use crate::{CopiedRun, DrawList};

/// The keys of `next` and every area that changed from `previous`, with its
/// paint order, or nothing when the runs cannot be trusted to stand for
/// what they copied.
pub(super) fn copied_changes(
    (previous, previous_keys): (&DrawList, &[Key]),
    next: &DrawList,
    runs: &[CopiedRun],
) -> Option<(Vec<Key>, Vec<(Geometry, usize)>)> {
    if runs.is_empty() || previous_keys.len() != previous.commands.len() {
        return None;
    }
    let mut end = 0;
    for run in runs {
        if run.to < end
            || run.to + run.len > next.commands.len()
            || run.from + run.len > previous.commands.len()
        {
            return None;
        }
        end = run.to + run.len;
    }
    let mut sources: Vec<(usize, usize)> = runs
        .iter()
        .map(|run| (run.from, run.from + run.len))
        .collect();
    sources.sort_unstable();
    if sources.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return None;
    }
    // The last list's commands no run copied.
    let mut previous_loose: Keyed<'_> = HashMap::new();
    let mut at = 0;
    for &(start, end) in sources
        .iter()
        .chain(std::iter::once(&(previous.commands.len(), 0)))
    {
        for order in at..start {
            previous_loose.insert(previous_keys[order], (order, &previous.commands[order]));
        }
        at = end;
    }
    // This list's keys, copied along each run and counted between them, and
    // whether everything in both lists kept its old order.
    let mut keys: Vec<Key> = Vec::with_capacity(next.commands.len());
    let mut current_loose: Keyed<'_> = HashMap::new();
    let mut emitted: HashMap<NodeHandle, u32> = HashMap::new();
    let mut last_place: Option<usize> = None;
    let mut ordered = |place: usize, last: usize| {
        let kept = last_place.is_none_or(|before| before < place);
        last_place = Some(last);
        kept
    };
    let mut at = 0;
    let tail = CopiedRun {
        to: next.commands.len(),
        from: 0,
        len: 0,
    };
    for run in runs.iter().chain(std::iter::once(&tail)) {
        for order in at..run.to {
            let command = &next.commands[order];
            let key = next_key(&mut emitted, command.node());
            if let Some(&(place, _)) = previous_loose.get(&key)
                && !ordered(place, place)
            {
                return None;
            }
            keys.push(key);
            current_loose.insert(key, (order, command));
        }
        if run.len > 0 && !ordered(run.from, run.from + run.len - 1) {
            return None;
        }
        keys.extend_from_slice(&previous_keys[run.from..run.from + run.len]);
        at = run.to + run.len;
    }
    let mut changed = Vec::new();
    layer_changes((previous, previous_keys), (next, &keys), &mut changed);
    command_changes(
        &current_loose,
        &previous_loose,
        &HashSet::new(),
        &mut changed,
    );
    Some((keys, changed))
}
