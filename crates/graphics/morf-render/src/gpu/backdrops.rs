//! Frosted glass inside one surface: what each backdrop reads, when it has to
//! blur again, and the textures it keeps between frames.
//!
//! A backdrop reads what the surface drew beneath it in paint order. That is
//! re-rendered into a scratch target — clipped to the region the blur reaches —
//! rather than copied out of the surface's own target, because the surface's
//! target still holds last frame's glass wherever this frame's damage did not
//! reach, and a blur of the glass is not a blur of what is behind it.
//!
//! Re-rendering and re-blurring are skipped whenever what lies beneath is the
//! same as last time, compared command by command. A desk whose clock ticks on
//! top of its panels therefore blurs nothing at all after the first frame.

use std::collections::HashMap;

use morf_layout::Geometry;
use morf_scene::NodeHandle;

use super::textures::BlurPass;
use crate::{DamageRect, DrawCommand, DrawList, Layer};

mod prepare;

/// The layer that starts at each command, by the layer it is drawn into.
pub(crate) type ChildLayers = HashMap<(Option<usize>, usize), usize>;

/// Something a backdrop's picture depends on, compared between frames.
#[derive(Clone, Debug, PartialEq)]
enum Beneath {
    /// A command drawn beneath, and whether it had anything to draw with: an
    /// image still loading draws nothing, and the same command a frame later
    /// draws the picture.
    Command(DrawCommand, bool),
    /// A whole layer composited beneath: how it is composited. What it holds
    /// follows as commands.
    Layer(Layer),
    /// Another backdrop beneath this one, by how many times it has blurred.
    Backdrop(NodeHandle, u64),
}

/// What a backdrop draws into its scratch target to blur it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BeneathOp {
    /// A command, drawn directly.
    Command(usize),
    /// A layer that was complete before the backdrop, composited from its
    /// target.
    Layer(usize),
}

/// One backdrop's textures and what they were made from.
pub(crate) struct BackdropEntry {
    region: DamageRect,
    plan: (u32, f32),
    beneath: Vec<Beneath>,
    /// How many times it has blurred, which a backdrop above it compares.
    pub(crate) generation: u64,
    /// The region, copied out of the scratch target.
    pub(crate) source: wgpu::Texture,
    /// Levels 1 and down; level 1 is what the glass shows.
    levels: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// Each pass's parameters, and which level it writes.
    pub(crate) passes: Vec<(BlurPass, usize)>,
    /// Samples level 1, for the quad that draws the glass.
    pub(crate) bind_group: wgpu::BindGroup,
    used: bool,
}

impl BackdropEntry {
    pub(crate) fn level_view(&self, level: usize) -> &wgpu::TextureView {
        &self.levels[level - 1].1
    }

    pub(crate) fn region(&self) -> DamageRect {
        self.region
    }
}

/// Every backdrop's entry, by node, and the scratch target they share.
#[derive(Default)]
pub(crate) struct BackdropCache {
    pub(crate) entries: HashMap<NodeHandle, BackdropEntry>,
    pub(crate) scratch: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Blurs done since the backend started, for measuring.
    pub(crate) blurs: u64,
}

/// A backdrop in this frame: the quad that draws it, and, when it has to blur
/// again, what goes into the scratch target first.
pub(crate) struct BackdropDraw {
    pub(crate) node: NodeHandle,
    pub(crate) instance: u32,
    pub(crate) refresh: Option<Vec<BeneathOp>>,
}

/// What is beneath command `stop`, in the order it is drawn, as far as it
/// touches `reach`.
///
/// Layers that closed before `stop` are composited whole from their targets.
/// A layer that holds `stop` — the panel's own rounded clip, a fading group
/// around it — has only drawn part of itself by then, so what it has drawn is
/// taken command by command, as though it were not a layer.
pub(crate) fn beneath_ops(
    list: &DrawList,
    command_layers: &[Option<usize>],
    child_layers: &ChildLayers,
    stop: usize,
    reach: Geometry,
) -> Vec<BeneathOp> {
    fn walk(
        list: &DrawList,
        command_layers: &[Option<usize>],
        child_layers: &ChildLayers,
        parent: Option<usize>,
        range: std::ops::Range<usize>,
        (stop, reach): (usize, Geometry),
        ops: &mut Vec<BeneathOp>,
    ) {
        let mut index = range.start;
        while index < range.end && index < stop {
            if let Some(&child) = child_layers.get(&(parent, index)) {
                let layer = &list.layers[child];
                if layer.commands.end <= stop {
                    if overlaps(layer.bounds, reach) {
                        ops.push(BeneathOp::Layer(child));
                    }
                } else {
                    walk(
                        list,
                        command_layers,
                        child_layers,
                        Some(child),
                        layer.commands.clone(),
                        (stop, reach),
                        ops,
                    );
                }
                index = layer.commands.end.max(index + 1);
            } else {
                if command_layers[index] == parent && overlaps(list.commands[index].bounds(), reach)
                {
                    ops.push(BeneathOp::Command(index));
                }
                index += 1;
            }
        }
    }
    let mut ops = Vec::new();
    walk(
        list,
        command_layers,
        child_layers,
        None,
        0..list.commands.len(),
        (stop, reach),
        &mut ops,
    );
    ops
}

/// One piece of offscreen work before the surface's own pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Offscreen {
    /// Render a layer's subtree into its target.
    Layer(usize),
    /// Blur what is beneath the backdrop at this command.
    Backdrop(usize),
}

/// The order offscreen work is done in: paint order, a layer as soon as its
/// last command is behind it.
///
/// That is what lets a backdrop inside one panel read the panels beneath it:
/// every layer that closed before the backdrop has been rendered by the time
/// it blurs, and every backdrop beneath it has blurred. A layer is still
/// rendered after everything inside it, as the reversed order it replaced
/// guaranteed. Layers with nothing in them come first; nothing reads them.
pub(crate) fn offscreen_order(
    list: &DrawList,
    command_layers: &[Option<usize>],
    child_layers: &ChildLayers,
) -> Vec<Offscreen> {
    fn visit(
        list: &DrawList,
        command_layers: &[Option<usize>],
        child_layers: &ChildLayers,
        parent: Option<usize>,
        range: std::ops::Range<usize>,
        order: &mut Vec<Offscreen>,
    ) {
        let mut index = range.start;
        while index < range.end {
            if let Some(&child) = child_layers.get(&(parent, index)) {
                let layer = &list.layers[child];
                visit(
                    list,
                    command_layers,
                    child_layers,
                    Some(child),
                    layer.commands.clone(),
                    order,
                );
                order.push(Offscreen::Layer(child));
                index = layer.commands.end.max(index + 1);
            } else {
                if command_layers[index] == parent
                    && matches!(list.commands[index], DrawCommand::Backdrop { .. })
                {
                    order.push(Offscreen::Backdrop(index));
                }
                index += 1;
            }
        }
    }
    let mut order: Vec<Offscreen> = list
        .layers
        .iter()
        .enumerate()
        .filter(|(_, layer)| layer.commands.is_empty())
        .map(|(index, _)| Offscreen::Layer(index))
        .collect();
    visit(
        list,
        command_layers,
        child_layers,
        None,
        0..list.commands.len(),
        &mut order,
    );
    // Anything the walk could not reach — a layer whose range another
    // swallowed — is still rendered, last, as it always was.
    let mut seen = vec![false; list.layers.len()];
    for step in &order {
        if let Offscreen::Layer(layer) = step {
            seen[*layer] = true;
        }
    }
    for (layer, seen) in seen.iter().enumerate().rev() {
        if !seen {
            order.push(Offscreen::Layer(layer));
        }
    }
    order
}

fn overlaps(left: Geometry, right: Geometry) -> bool {
    left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}
