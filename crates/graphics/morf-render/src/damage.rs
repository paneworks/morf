use morf_layout::{Geometry, Layout};
use morf_scene::{NodeHandle, Scene};
use std::collections::HashMap;
use std::error::Error as StdError;

use crate::{commands::*, effects::*, sdf::*};

mod explain;
use explain::{command_change, damage_log_wanted, explain, kind, layer_change};

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
    }

    /// Forgets what is on screen, so the next frame is damaged in full.
    ///
    /// For when the surface stops being what the last frame was painted into —
    /// a resize hands back a blank target, and every pixel the tracker believes
    /// is still there is gone with the old one.
    pub fn forget(&mut self) {
        self.previous = DrawList::default();
        self.scale_120 = 0;
    }

    /// Diffs commands and converts changed logical bounds at protocol scale in 120ths.
    pub fn diff(&mut self, next: &DrawList, scale_120: u32) -> Vec<DamageRect> {
        if self.scale_120 != 0 && self.scale_120 != scale_120 {
            if damage_log_wanted() {
                eprintln!(
                    "damage: whole surface: scale {} -> {scale_120}",
                    self.scale_120
                );
            }
            self.scale_120 = scale_120;
            return merge_damage(
                next.commands
                    .iter()
                    .filter_map(|command| physical_damage(command.bounds(), scale_120))
                    .collect(),
            );
        }
        let previous_keys = command_keys(&self.previous.commands);
        let current_keys = command_keys(&next.commands);
        let previous = keyed_commands(&self.previous.commands, &previous_keys);
        let current = keyed_commands(&next.commands, &current_keys);
        // Each changed area with the paint order it was drawn at, so a frosted
        // panel can tell a change beneath it from one on top of it.
        let mut changed: Vec<(Geometry, usize)> = Vec::new();
        // A layer that composites differently — it moved, turned, faded, took
        // or lost a command — changes everything it covers, before and after,
        // even where none of its commands did. Only those areas: a clock hand
        // turning inside one panel used to repaint the whole surface, because
        // any change to any layer did.
        let previous_layers = keyed_layers(&self.previous, &previous_keys);
        let current_layers = keyed_layers(next, &current_keys);
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
        for (key, (order, command)) in &current {
            match previous.get(key) {
                Some((old_order, old)) if old_order == order && *old == *command => {}
                // Nothing drawn before or after: no pixel changed.
                Some((_, old)) if old.draws_nothing() && command.draws_nothing() => {}
                None if command.draws_nothing() => {}
                Some((old_order, old)) => {
                    // A terminal whose screen alone changed damages the rows
                    // that did, not its whole rectangle.
                    // And a field whose layers alone moved damages where they
                    // were and are, not its whole reach.
                    match (old_order == order)
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
                                command_change(old, command, *old_order != *order)
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
        for (key, (order, command)) in &previous {
            if !current.contains_key(key) && !command.draws_nothing() {
                explain("command removed", command.node(), command.bounds(), || {
                    kind(command).to_owned()
                });
                changed.push((command.bounds(), *order));
            }
        }
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
        merge_damage(
            logical
                .into_iter()
                .filter_map(|geometry| physical_damage(geometry, scale_120))
                .collect(),
        )
    }
}

/// Renderer implementation selected by the surface runtime.
pub trait RenderBackend {
    /// Backend error.
    type Error: StdError + Send + Sync + 'static;

    /// Draws an ordered list, restricting pixel work to damage rectangles.
    fn render(
        &mut self,
        list: &DrawList,
        damage: &[DamageRect],
        scale_120: u32,
    ) -> Result<(), Self::Error>;

    /// Resizes the target the frames are painted into.
    ///
    /// Whatever the last frame left there does not survive this, which is why
    /// it is on the trait and not only on the backend: it has to be reachable
    /// through the engine, and the engine is the only thing that knows the
    /// screen has to be repainted in full afterwards.
    fn resize(&mut self, width: u32, height: u32);
}

/// Scene painter and damage tracker driving a selected backend.
pub struct RenderEngine<B> {
    backend: B,
    damage: DamageTracker,
    /// The draw list, kept between frames for its capacity.
    list: DrawList,
}

impl<B: RenderBackend> RenderEngine<B> {
    /// Wraps a renderer backend with draw-list and damage processing.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            damage: DamageTracker::default(),
            list: DrawList::default(),
        }
    }

    /// Paints one resolved scene frame.
    pub fn render(
        &mut self,
        scene: &Scene,
        layout: &Layout,
        scale_120: u32,
        declare: impl FnOnce(&[DamageRect]),
    ) -> Result<Vec<DamageRect>, RenderError> {
        // The list is kept between frames so its buffers are not returned to
        // the allocator and reclaimed sixty times a second.
        let mut list = std::mem::take(&mut self.list);
        let result = list.rebuild(scene, layout).and_then(|()| {
            let damage = self.damage.diff(&list, scale_120);
            // The caller sees the damage before anything is presented, because
            // presenting commits the surface and a commit carries whatever
            // damage was declared before it. A host that waits until afterwards
            // has no way to tell the compositor what actually changed, and ends
            // up declaring the whole surface — which on a fullscreen overlay
            // means a full recomposite every frame.
            declare(&damage);
            if !damage.is_empty() {
                self.backend
                    .render(&list, &damage, scale_120)
                    .map_err(|error| RenderError::Backend(error.to_string()))?;
            }
            Ok(damage)
        });
        // Only on success: a failed rebuild leaves a half-built list, and
        // making that the baseline would silently under-damage the next frame.
        if result.is_ok() {
            self.damage.retain(&mut list);
        }
        self.list = list;
        result
    }

    /// Resizes the target, and forgets what was on it.
    ///
    /// The two belong together. A resize hands back a blank target, so a frame
    /// diffed against the one before it repaints only what changed and leaves
    /// the rest of the screen as the cleared colour — black, with whatever
    /// happens to animate afterwards appearing on it one piece at a time. That
    /// is what this is: the resize goes through the engine so the baseline
    /// cannot be left behind.
    pub fn resize(&mut self, width: u32, height: u32) {
        if damage_log_wanted() {
            eprintln!("damage: whole surface: resized to {width}x{height}");
        }
        self.backend.resize(width, height);
        self.damage.forget();
    }

    /// Forgets what is on screen, so the next frame is drawn in full.
    ///
    /// For when the backend's target was replaced behind the engine's back —
    /// a change of blend space rebuilds it, and nothing the tracker remembers
    /// is on the new one.
    pub fn forget(&mut self) {
        if damage_log_wanted() {
            eprintln!(
                "damage: whole surface: the target was replaced (blend or subpixel text changed)"
            );
        }
        self.damage.forget();
    }

    /// Returns the backend for surface-specific operations.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }
}

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
fn keyed_commands<'a>(
    commands: &'a [DrawCommand],
    keys: &[(NodeHandle, u32)],
) -> HashMap<(NodeHandle, u32), (usize, &'a DrawCommand)> {
    commands
        .iter()
        .zip(keys)
        .enumerate()
        .map(|(order, (command, key))| (*key, (order, command)))
        .collect()
}

/// Each command's key, in paint order: its node and which of that node's
/// commands it is.
fn command_keys(commands: &[DrawCommand]) -> Vec<(NodeHandle, u32)> {
    let mut emitted: HashMap<NodeHandle, u32> = HashMap::new();
    commands
        .iter()
        .map(|command| {
            let node = command.node();
            let occurrence = emitted.entry(node).or_default();
            let key = (node, *occurrence);
            *occurrence += 1;
            key
        })
        .collect()
}

/// How one layer composites, independent of where it sits in the lists.
struct LayerShape {
    /// The layer itself, with its command range and parent index cleared: a
    /// line of text added elsewhere shifts every range without changing how
    /// anything looks.
    layer: Layer,
    /// Its parent, by key rather than index.
    parent: Option<(NodeHandle, u32)>,
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
fn keyed_layers(
    list: &DrawList,
    command_keys: &[(NodeHandle, u32)],
) -> HashMap<(NodeHandle, u32), LayerShape> {
    use std::hash::{Hash, Hasher};
    let mut emitted: HashMap<NodeHandle, u32> = HashMap::new();
    let keys: Vec<(NodeHandle, u32)> = list
        .layers
        .iter()
        .map(|layer| {
            let occurrence = emitted.entry(layer.node).or_default();
            let key = (layer.node, *occurrence);
            *occurrence += 1;
            key
        })
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
