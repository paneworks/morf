//! Telling which command and which layer of one frame is which of the last,
//! and what changed between the two.

use morf_layout::Geometry;
use morf_scene::NodeHandle;
use std::collections::{HashMap, HashSet};

use super::explain::{command_change, explain, kind, layer_change};
use crate::commands::*;

/// A command's node and which of that node's commands it is.
pub(super) type Key = (NodeHandle, u32);

/// A list's command keys, and every area that changed with the paint order
/// it was drawn at.
pub(crate) type Changes = (Vec<Key>, Vec<(Geometry, usize)>);

/// A command by its key, with where it is in paint order.
pub(super) type Keyed<'a> = HashMap<Key, (usize, &'a DrawCommand)>;

/// Indexes a frame's commands so that two commands can be compared against the
/// two they correspond to in the previous frame.
///
/// The node alone is not enough to say which command is which. A `ClipRect`
/// emits two — the fill and the border it overlays — and keying on the node
/// collapses them, because collecting a `HashMap` keeps the last entry for a
/// duplicate key. The fill would then never be compared against anything and
/// changing it would repaint nothing. The occurrence index is what separates
/// them, and it is stable because paint always emits a node's commands in the
/// same order.
pub(super) fn keyed_commands<'a>(commands: &'a [DrawCommand], keys: &[Key]) -> Keyed<'a> {
    commands
        .iter()
        .zip(keys)
        .enumerate()
        .map(|(order, (command, key))| (*key, (order, command)))
        .collect()
}

/// The commands present in both frames that changed places relative to the
/// others: everything outside the longest run of them that kept its order.
/// A command added or removed is damaged as such; the ones it pushed along
/// are not moved by it.
pub(super) fn moved_commands(current_keys: &[Key], previous: &Keyed<'_>) -> HashSet<Key> {
    // The old place of each command that is in both, in the new paint order.
    let common: Vec<(Key, usize)> = current_keys
        .iter()
        .filter_map(|key| previous.get(key).map(|(order, _)| (*key, *order)))
        .collect();
    // Longest increasing run of old places (patience sorting): `tails[k]`
    // is the index into `common` ending the best run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut back: Vec<Option<usize>> = vec![None; common.len()];
    for (index, &(_, place)) in common.iter().enumerate() {
        let at = tails.partition_point(|&tail| common[tail].1 < place);
        back[index] = at.checked_sub(1).map(|before| tails[before]);
        if at == tails.len() {
            tails.push(index);
        } else {
            tails[at] = index;
        }
    }
    let mut kept = vec![false; common.len()];
    let mut walk = tails.last().copied();
    while let Some(index) = walk {
        kept[index] = true;
        walk = back[index];
    }
    common
        .iter()
        .zip(kept)
        .filter(|(_, kept)| !kept)
        .map(|((key, _), _)| *key)
        .collect()
}

/// Each command's key, in paint order: its node and which of that node's
/// commands it is.
pub(super) fn command_keys(commands: &[DrawCommand]) -> Vec<Key> {
    let mut emitted: HashMap<NodeHandle, u32> = HashMap::new();
    commands
        .iter()
        .map(|command| next_key(&mut emitted, command.node()))
        .collect()
}

/// The key of `node`'s next command, counting in `emitted`.
pub(super) fn next_key(emitted: &mut HashMap<NodeHandle, u32>, node: NodeHandle) -> Key {
    let occurrence = emitted.entry(node).or_default();
    let key = (node, *occurrence);
    *occurrence += 1;
    key
}

/// Every command of `current` that differs from its counterpart in
/// `previous`, has none, or is `moved`, and every command of `previous` that
/// has none in `current`: where it was and is, with its paint order.
pub(super) fn command_changes(
    current: &Keyed<'_>,
    previous: &Keyed<'_>,
    moved: &HashSet<Key>,
    changed: &mut Vec<(Geometry, usize)>,
) {
    for (key, (order, command)) in current {
        match previous.get(key) {
            Some((_, old)) if !moved.contains(key) && *old == *command => {}
            // Nothing drawn before or after: no pixel changed.
            Some((_, old)) if old.draws_nothing() && command.draws_nothing() => {}
            None if command.draws_nothing() => {}
            Some((old_order, old)) => {
                // A terminal whose screen alone changed damages the rows
                // that did, not its whole rectangle.
                // And a field whose layers alone moved damages where they
                // were and are, not its whole reach.
                match (!moved.contains(key))
                    .then(|| {
                        command
                            .terminal_rows_changed(old)
                            .or_else(|| command.field_layers_changed(old))
                    })
                    .flatten()
                {
                    Some(rows) => changed.extend(rows.into_iter().map(|row| (row, *order))),
                    None => {
                        explain("command changed", command.node(), command.bounds(), || {
                            command_change(old, command, moved.contains(key))
                        });
                        changed.push((old.bounds(), (*old_order).min(*order)));
                        changed.push((command.bounds(), *order));
                    }
                }
            }
            None => {
                explain("command added", command.node(), command.bounds(), || {
                    kind(command).to_owned()
                });
                changed.push((command.bounds(), *order));
            }
        }
    }
    for (key, (order, command)) in previous {
        if !current.contains_key(key) && !command.draws_nothing() {
            explain("command removed", command.node(), command.bounds(), || {
                kind(command).to_owned()
            });
            changed.push((command.bounds(), *order));
        }
    }
}

/// A layer that composites differently — it moved, turned, faded, took or
/// lost a command — changes everything it covers, before and after, even
/// where none of its commands did. Only those areas: a clock hand turning
/// inside one panel used to repaint the whole surface, because any change to
/// any layer did.
pub(super) fn layer_changes(
    (previous, previous_keys): (&DrawList, &[Key]),
    (next, current_keys): (&DrawList, &[Key]),
    changed: &mut Vec<(Geometry, usize)>,
) {
    let previous_layers = keyed_layers(previous, previous_keys);
    let current_layers = keyed_layers(next, current_keys);
    for (key, shape) in &current_layers {
        match previous_layers.get(key) {
            Some(old) if old == shape => {}
            Some(old) => {
                explain("layer changed", shape.layer.node, old.layer.bounds, || {
                    layer_change(&old.layer, &shape.layer)
                });
                changed.push((old.layer.bounds, old.start.min(shape.start)));
                changed.push((shape.layer.bounds, shape.start));
            }
            None => {
                explain(
                    "layer added",
                    shape.layer.node,
                    shape.layer.bounds,
                    String::new,
                );
                changed.push((shape.layer.bounds, shape.start));
            }
        }
    }
    for (key, old) in &previous_layers {
        if !current_layers.contains_key(key) {
            explain(
                "layer removed",
                old.layer.node,
                old.layer.bounds,
                String::new,
            );
            changed.push((old.layer.bounds, old.start));
        }
    }
}

/// How one layer composites, independent of where it sits in the lists.
struct LayerShape {
    /// The layer itself, with its command range and parent index cleared: a
    /// line of text added elsewhere shifts every range without changing how
    /// anything looks.
    layer: Layer,
    /// Its parent, by key rather than index.
    parent: Option<Key>,
    /// Which commands it holds, by key, hashed.
    members: u64,
    /// Where it starts in paint order, for telling what lies beneath a
    /// backdrop; not part of how it looks.
    start: usize,
}

impl PartialEq for LayerShape {
    fn eq(&self, other: &Self) -> bool {
        self.layer == other.layer && self.parent == other.parent && self.members == other.members
    }
}

/// Every layer by its node and which of that node's layers it is — a bordered
/// `ClipRect` makes two — with how it composites.
fn keyed_layers(list: &DrawList, command_keys: &[Key]) -> HashMap<Key, LayerShape> {
    use std::hash::{Hash, Hasher};
    let mut emitted: HashMap<NodeHandle, u32> = HashMap::new();
    let keys: Vec<Key> = list
        .layers
        .iter()
        .map(|layer| next_key(&mut emitted, layer.node))
        .collect();
    list.layers
        .iter()
        .zip(&keys)
        .map(|(layer, key)| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            command_keys
                .get(layer.commands.clone())
                .unwrap_or_default()
                .hash(&mut hasher);
            let shape = LayerShape {
                layer: Layer {
                    commands: 0..0,
                    parent: None,
                    // Indices, like the range: which layer a mask is shows
                    // in the mask layer's parent and members.
                    alpha_mask: layer.alpha_mask.map(|mask| AlphaMask { layer: 0, ..mask }),
                    mask_for: layer.mask_for.map(|_| 0),
                    ..layer.clone()
                },
                parent: layer.parent.and_then(|parent| keys.get(parent).copied()),
                members: hasher.finish(),
                start: layer.commands.start,
            };
            (*key, shape)
        })
        .collect()
}
