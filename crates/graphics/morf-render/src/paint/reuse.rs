//! Keeping the commands of subtrees that did not change.
//!
//! A frame where one badge changes colour paints the same few hundred nodes
//! as the frame before, reading every property again. The scene stamps every
//! node with the newest paint revision in its subtree (see
//! [`Scene::paint_stamp`]); a subtree stamped no later than the last time its
//! commands were made, laid out in the same box and painted into the same
//! surroundings, draws exactly what it drew, so its commands and layers are
//! copied from the last list instead.
//!
//! Each node's span is kept relative to its parent's, so a copied subtree
//! keeps the spans of everything under it. A span is trusted only along a
//! chain of nodes whose spans were all written for the last list: `stamped`
//! must be the frame its parent's commands were last made in, which fails
//! for anything a hidden or emptied parent did not reach.

use morf_layout::{Geometry, Layout, Transform2D};
use morf_scene::{Color, FastMap, NodeHandle, Scene};

use super::{PaintContext, paint_node};
use crate::commands::{AlphaMask, DrawList};
use crate::sdf::RenderError;

/// Where a node's parent starts, in the list being made and in the last one.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SpanBase {
    commands: usize,
    layers: usize,
    /// Its first command and layer in the last list, and the frame its
    /// commands were made in; nothing when the last list is not to be read.
    previous: Option<(usize, usize, u64)>,
}

/// Everything a subtree's commands take from above it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Surroundings {
    transform: Transform2D,
    clip: Option<Geometry>,
    overlay: Color,
    layered: bool,
    in_field: bool,
    color: Option<Color>,
    rtl: bool,
}

impl Surroundings {
    fn of(context: &PaintContext) -> Self {
        Self {
            transform: context.transform,
            clip: context.clip,
            overlay: context.overlay,
            layered: context.layer.is_some(),
            in_field: context.in_field,
            color: context.color,
            rtl: context.rtl,
        }
    }
}

/// One node's commands and layers in the list its span was written for.
#[derive(Clone, Copy, Debug)]
struct Span {
    /// The frame this span was last written in.
    stamped: u64,
    /// The frame its commands were last made rather than copied.
    built: u64,
    /// First command from the parent's first, and how many.
    commands: (usize, usize),
    /// First layer from the parent's first, and how many.
    layers: (usize, usize),
    /// The paint revision its commands were made at.
    revision: u64,
    geometry: Option<Geometry>,
    around: Surroundings,
    /// It holds a field layer that follows a node elsewhere in the scene,
    /// which no stamp under it records.
    volatile: bool,
}

/// Commands `to..to + len` of a list, copied unchanged from `from..from + len`
/// of the list made before it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopiedRun {
    /// First command in the list made.
    pub to: usize,
    /// First command in the list copied from.
    pub from: usize,
    /// How many.
    pub len: usize,
}

/// Which commands of a list are the last list's, unchanged, for whatever
/// kept work of its own a backend made from that list.
#[derive(Clone, Copy, Debug, Default)]
pub struct Copies<'a> {
    /// The list made, by the frame it was made in.
    pub frame: Option<u64>,
    /// The list copied from, by the frame it was made in.
    pub from: Option<u64>,
    /// In the order of `to`, none overlapping.
    pub runs: &'a [CopiedRun],
}

/// What a surface remembers of its last draw list, to copy from it.
#[derive(Debug, Default)]
pub struct PaintCache {
    spans: FastMap<NodeHandle, Span>,
    /// The frame the last list was made in, while that list is kept.
    previous: Option<u64>,
    frame: u64,
    /// What the list made last copied, and the frame it copied from.
    runs: Vec<CopiedRun>,
    from: Option<u64>,
}

impl PaintCache {
    /// The list just made is the one kept for the next frame to copy from.
    pub(crate) fn kept(&mut self) {
        self.previous = Some(self.frame);
    }

    /// Nothing kept can be copied from.
    pub(crate) fn forget(&mut self) {
        self.spans.clear();
        self.previous = None;
        self.runs.clear();
        self.from = None;
    }

    /// What the list made last copied from the one before it.
    pub(crate) fn copies(&self) -> Copies<'_> {
        Copies {
            frame: Some(self.frame),
            from: self.from,
            runs: if self.from.is_some() { &self.runs } else { &[] },
        }
    }
}

/// One rebuild's view of the cache and the list it may copy from.
pub(crate) struct Reuse<'a> {
    cache: &'a mut PaintCache,
    previous: &'a DrawList,
    revision: u64,
    volatile: bool,
}

impl DrawList {
    /// Refills this list from the scene, copying from `previous`, the list
    /// made last time with `cache`, every subtree that did not change.
    ///
    /// The result is what [`DrawList::rebuild`] makes, command for command.
    pub(crate) fn rebuild_from(
        &mut self,
        scene: &Scene,
        layout: &Layout,
        previous: &DrawList,
        cache: &mut PaintCache,
    ) -> Result<(), RenderError> {
        cache.frame += 1;
        cache.runs.clear();
        cache.from = cache.previous;
        if cache.spans.len() > 2 * scene.node_count() + 1024 {
            cache.spans.retain(|node, _| scene.contains(*node));
        }
        let base = SpanBase {
            commands: 0,
            layers: 0,
            previous: cache.previous.map(|frame| (0, 0, frame)),
        };
        let mut reuse = Reuse {
            cache,
            previous,
            revision: scene.paint_revision(),
            volatile: false,
        };
        let built = self.rebuild_with(scene, layout, base, Some(&mut reuse));
        if built.is_err() {
            reuse.cache.forget();
        }
        built
    }
}

/// Paints `node`, or copies what it painted last time when nothing it draws
/// from has moved.
pub(crate) fn append_node(
    scene: &Scene,
    layout: &Layout,
    node: NodeHandle,
    inherited: PaintContext,
    list: &mut DrawList,
    reuse: Option<&mut Reuse<'_>>,
) -> Result<(), RenderError> {
    let Some(reuse) = reuse else {
        return paint_node(scene, layout, node, inherited, list, None);
    };
    let base = inherited.span;
    let start = (list.commands.len(), list.layers.len());
    let around = Surroundings::of(&inherited);
    let frame = reuse.cache.frame;
    let previous = match (base.previous, reuse.cache.spans.get(&node)) {
        (Some((commands, layers, built)), Some(span)) if span.stamped == built => {
            Some((commands + span.commands.0, layers + span.layers.0, *span))
        }
        _ => None,
    };
    let geometry = layout.geometry(node);
    if let Some((commands, layers, span)) = previous
        && !span.volatile
        && span.geometry == geometry
        && span.around == around
        && scene.paint_stamp(node)? <= span.revision
        && reuse.copy(list, (commands, layers), &span, inherited.layer)
    {
        reuse.cache.spans.insert(
            node,
            Span {
                stamped: frame,
                commands: (start.0 - base.commands, span.commands.1),
                layers: (start.1 - base.layers, span.layers.1),
                ..span
            },
        );
        return Ok(());
    }
    let rtl = match scene.string_value(node, "layout_direction") {
        Ok("rtl") => true,
        Ok("ltr") => false,
        _ => inherited.rtl,
    };
    let own = PaintContext {
        span: SpanBase {
            commands: start.0,
            layers: start.1,
            previous: previous.map(|(commands, layers, span)| (commands, layers, span.built)),
        },
        rtl,
        ..inherited
    };
    let outer = std::mem::take(&mut reuse.volatile);
    paint_node(scene, layout, node, own, list, Some(&mut *reuse))?;
    let volatile = reuse.volatile || scene.track(node).is_some();
    reuse.volatile = outer || volatile;
    reuse.cache.spans.insert(
        node,
        Span {
            stamped: frame,
            built: frame,
            commands: (start.0 - base.commands, list.commands.len() - start.0),
            layers: (start.1 - base.layers, list.layers.len() - start.1),
            revision: reuse.revision,
            geometry,
            around,
            volatile,
        },
    );
    Ok(())
}

impl Reuse<'_> {
    /// Appends the last list's commands and layers from `at`, as `span`
    /// counts them, with every layer index moved to where it lands and a
    /// layer outside the span read as `layer`. False, having appended
    /// nothing, when the last list does not hold them.
    fn copy(
        &mut self,
        list: &mut DrawList,
        (commands, layers): (usize, usize),
        span: &Span,
        layer: Option<usize>,
    ) -> bool {
        let command_range = commands..commands + span.commands.1;
        let layer_range = layers..layers + span.layers.1;
        let (Some(source_commands), Some(source_layers)) = (
            self.previous.commands.get(command_range.clone()),
            self.previous.layers.get(layer_range.clone()),
        ) else {
            return false;
        };
        let inside = |range: &std::ops::Range<usize>| {
            range.start >= command_range.start && range.end <= command_range.end
        };
        let within = |index: usize| layer_range.contains(&index);
        if !source_layers.iter().all(|layer| {
            inside(&layer.commands)
                && layer.alpha_mask.is_none_or(|mask| within(mask.layer))
                && layer.mask_for.is_none_or(within)
        }) {
            return false;
        }
        let (to_command, to_layer) = (list.commands.len(), list.layers.len());
        let moved = |index: usize| index - layers + to_layer;
        list.commands.extend_from_slice(source_commands);
        for source in source_layers {
            let mut copied = source.clone();
            copied.commands = source.commands.start - commands + to_command
                ..source.commands.end - commands + to_command;
            copied.parent = match source.parent {
                Some(parent) if within(parent) => Some(moved(parent)),
                _ => layer,
            };
            copied.alpha_mask = source.alpha_mask.map(|mask| AlphaMask {
                layer: moved(mask.layer),
                ..mask
            });
            copied.mask_for = source.mask_for.map(moved);
            list.layers.push(copied);
        }
        let runs = &mut self.cache.runs;
        match runs.last_mut() {
            Some(last) if last.to + last.len == to_command && last.from + last.len == commands => {
                last.len += span.commands.1;
            }
            _ if span.commands.1 == 0 => {}
            _ => runs.push(CopiedRun {
                to: to_command,
                from: commands,
                len: span.commands.1,
            }),
        }
        true
    }
}
