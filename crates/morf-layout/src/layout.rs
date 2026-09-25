use crate::helpers::anchors;
use crate::helpers::apply_anchors;
use crate::helpers::reject_axis_conflict;

use morf_scene::{Element, FastMap, NodeHandle, Scene};

use crate::incremental::{Basis, Dirty, Local};

use crate::attached::Attached;
use crate::custom::{CustomLayout, NoCustom};
use crate::distribute::align_across;
use crate::flex::FlexTree;
use crate::flex_style::is_flex_root;
use crate::geometry::TextOptions;
use crate::geometry::{Geometry, Size, TextMeasurer};
use crate::helpers::{
    LayoutError, attached_layout, grid_columns, grid_size, justify_run, positive, sum_with_spacing,
    text_alignment, text_elide,
};
use crate::resolve_containers::grid_gaps;
use crate::transform::{distributed_margin, inset_margin};

/// Complete layout output keyed by stable node handles.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub(crate) geometry: FastMap<NodeHandle, Geometry>,
    pub(crate) implicit: FastMap<NodeHandle, Size>,
    /// The size each node asked for, worked out once.
    ///
    /// Resolving it means reading five properties and parsing the attached
    /// layout map, and every node is asked at least twice in a pass — once
    /// while its parent measures, once while its parent places it, and again
    /// for a leaf inside a `Flex`. The answer cannot change between those, so
    /// it is worked out where the implicit size is and looked up thereafter.
    pub(crate) requested: FastMap<NodeHandle, Size>,
    /// Widths to measure text at on the second pass.
    ///
    /// A `Text` with no width of its own is measured unconstrained, then
    /// placed at whatever width its parent gives it -- an `Inset`, a fill, a
    /// stretch. If it wraps or elides, that is the width it should have
    /// been measured at: its height is different, and so is its parent's.
    /// The first pass records those widths here; the second measures with
    /// them. Two passes, never more.
    pub(crate) text_widths: FastMap<NodeHandle, f64>,
    /// What this layout was computed from, so the next can start from it.
    pub(crate) basis: Option<Basis>,
    /// The first of the two passes, when a second one ran: an incremental
    /// update redoes both, each from its own last state.
    pub(crate) first: Option<Box<Layout>>,
    /// During a pass, which nodes it has to look at again; `None` for all.
    pub(crate) dirty: Option<Dirty>,
    /// How each node's position was worked out from its parent's, so a
    /// subtree that has only moved can be moved without being laid out.
    pub(crate) local: FastMap<NodeHandle, Local>,
}

/// Cached layout geometry used by native transform watchers.
#[derive(Debug, Default)]
pub struct TransformTracker {
    pub(crate) geometry: FastMap<NodeHandle, Geometry>,
}

/// Watches the geometry and transform chain between two scene nodes.
#[derive(Clone, Debug)]
pub struct TransformWatcher {
    pub(crate) a: NodeHandle,
    pub(crate) b: NodeHandle,
    pub(crate) common_parent: Option<NodeHandle>,
    pub(crate) signature: Option<u64>,
}

impl Layout {
    /// Resolves the layout rooted at `root` into the supplied surface area.
    ///
    /// Without a host for `Custom` containers: a scene that has one fails
    /// here, and a host that can run its functions uses `compute_with`.
    pub fn compute(
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
    ) -> Result<Self, LayoutError> {
        Self::compute_with(scene, root, available, text, &mut NoCustom)
    }

    /// Resolves the layout, with `host` answering for `Custom` containers.
    pub fn compute_with(
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<Self, LayoutError> {
        let revision = scene.layout_revision();
        let mut layout = Self::default();
        layout.pass(scene, root, available, text, host)?;
        let constrained = layout.texts_to_remeasure(scene)?;
        if !constrained.is_empty() {
            let first = layout.clone();
            layout.text_widths = constrained;
            layout.geometry.clear();
            layout.implicit.clear();
            layout.requested.clear();
            layout.local.clear();
            layout.pass(scene, root, available, text, host)?;
            layout.first = Some(Box::new(first));
        }
        layout.basis = Some(Basis {
            root,
            available,
            revision,
        });
        Ok(layout)
    }

    pub(crate) fn pass(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        available: Size,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        self.capture_exit_frames(scene);
        self.measure_implicit(scene, root, text, host)?;
        let geometry = Geometry {
            width: available.width,
            height: available.height,
            ..Geometry::default()
        };
        self.place(scene, root, geometry, text, host)
    }

    fn measure_implicit(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<Size, LayoutError> {
        let status = self.status(scene, node)?;
        let cached = self.implicit.get(&node).copied();
        if !status.visit
            && let Some(size) = cached
        {
            return Ok(size);
        }
        // A node that joined the tree since is new to this layout, and so
        // is everything under it.
        let forcing = status.fresh && !self.forcing();
        if forcing {
            self.force(true);
        }
        let size = self.measure_visited(scene, node, status.own, cached, text, host);
        if forcing {
            self.force(false);
        }
        size
    }

    fn measure_visited(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        own: bool,
        cached: Option<Size>,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<Size, LayoutError> {
        let children = scene.children(node)?;
        // Whether anything this node's own size is worked out from moved:
        // its own properties, or a child's -- its position or visibility,
        // or the size it asks for.
        let mut moved = own || cached.is_none();
        let mut child_sizes = Vec::with_capacity(children.len());
        // The children the node is sized from: not one on its way out, which
        // keeps the box it had and takes no room.
        let mut flow = Vec::with_capacity(children.len());
        // What a positioner packs: the children it can show. An invisible
        // child keeps its own size, but a Row, Column or Grid gives it no
        // room and no gap, as a `Flex` already does and QML's do.
        let mut shown_sizes = Vec::with_capacity(children.len());
        let positioner = matches!(
            scene.element(node)?,
            Element::Row | Element::Column | Element::Grid
        );
        for &child in children {
            let status = self.status(scene, child)?;
            let implicit = self.measure_implicit(scene, child, text, host)?;
            let requested = match self.requested.get(&child) {
                Some(&requested) if !status.visit => requested,
                before => {
                    let before = before.copied();
                    let requested = self.requested_size(scene, child, implicit)?;
                    self.requested.insert(child, requested);
                    moved |= status.own || before != Some(requested);
                    requested
                }
            };
            if scene.is_exiting(child) {
                continue;
            }
            child_sizes.push(requested);
            flow.push(child);
            if positioner && scene.bool_value(child, "visible")? {
                shown_sizes.push(requested);
            }
        }

        // Nothing it is sized from moved, so neither did its size -- except
        // a flex root's, which Taffy works out from the whole subtree.
        if !moved
            && let Some(size) = cached
            && !is_flex_root(scene, node)?
        {
            return Ok(size);
        }
        if scene.element(node)? == Element::Custom {
            // Unconstrained here: the room on offer is whatever the node
            // asked for, or no limit. The place pass says what it got.
            let available = Size {
                width: positive(scene.number(node, "width")?).unwrap_or(f64::INFINITY),
                height: positive(scene.number(node, "height")?).unwrap_or(f64::INFINITY),
            };
            let size = host
                .measure(node, available, &child_sizes)
                .map_err(LayoutError::Scene)?;
            self.implicit.insert(node, size);
            return Ok(size);
        }
        if is_flex_root(scene, node)? {
            // Taffy sizes the whole subtree at once: unconstrained here, for
            // the implicit size, and again at the resolved size when the
            // node is placed.
            let mut flex = FlexTree::build(scene, node)?;
            flex.compute(
                scene,
                taffy::prelude::Size {
                    width: taffy::prelude::AvailableSpace::MaxContent,
                    height: taffy::prelude::AvailableSpace::MaxContent,
                },
                &self.requested,
                text,
            )?;
            let size = flex.size();
            self.implicit.insert(node, size);
            return Ok(size);
        }
        if scene.element(node)? == Element::Text {
            let wrap = scene.bool_value(node, "wrap")?;
            if !wrap && scene.number(node, "max_lines")? > 0.0 {
                return Err(LayoutError::Scene(
                    "Text: `max_lines` needs `wrap = true`".to_owned(),
                ));
            }
            if wrap && scene.string_value(node, "elide")? != "none" {
                return Err(LayoutError::Scene(
                    "Text: `elide` is for unwrapped text; wrapped text takes `max_lines`"
                        .to_owned(),
                ));
            }
        }
        let size = match scene.element(node)? {
            Element::Text => text.measure(
                node,
                scene.string_value(node, "text")?,
                scene.string_value(node, "font_family")?,
                scene.number(node, "font_size")?,
                TextOptions {
                    width: self
                        .text_widths
                        .get(&node)
                        .copied()
                        .or(positive(scene.number(node, "width")?)),
                    wrap: scene.bool_value(node, "wrap")?,
                    alignment: text_alignment(scene.string_value(node, "horizontal_alignment")?)?,
                    elide: text_elide(scene.string_value(node, "elide")?)?,
                    font_weight: scene.number(node, "font_weight")?,
                    font_source: match scene.string_value(node, "font_source")? {
                        "" => None,
                        source => Some(source.to_owned()),
                    },
                    max_lines: scene.number(node, "max_lines")?.max(0.0) as usize,
                    style: crate::text_style::TextStyle::from_scene(scene, node)?,
                },
            ),
            Element::TextInput => {
                let width = self.text_widths.get(&node).copied();
                let shape = crate::text_input::InputShape::read(scene, node, width)?;
                let line = shape.line_height();
                let measured = text.measure(
                    node,
                    &shape.display.text,
                    &shape.family,
                    shape.size,
                    shape.options,
                );
                // Never shorter than a line, so an empty field with no
                // placeholder is still somewhere to click; and a caret's
                // width wider, so the caret at the end is not clipped away.
                Size {
                    width: measured.width + scene.number(node, "caret_width")?.max(0.0),
                    height: measured.height.max(line),
                }
            }
            Element::Image | Element::Icon => {
                let element = scene.element(node)?;
                let source = if element == Element::Image {
                    scene.string_value(node, "source")?
                } else {
                    scene.string_value(node, "name")?
                };
                let theme = (element == Element::Icon)
                    .then(|| scene.string_value(node, "theme"))
                    .transpose()?;
                let natural = text
                    .measure_image(node, element, source, theme)
                    .unwrap_or_default();
                let width = positive(scene.number(node, "source_width")?).unwrap_or(natural.width);
                let height =
                    positive(scene.number(node, "source_height")?).unwrap_or(natural.height);
                Size { width, height }
            }
            // A path's own size is its view box; without one, path units are
            // the node's pixels and it has no size but the one it is given.
            Element::Path => morf_scene::PathViewBox::parse(scene.current(node, "view_box")?)
                .ok()
                .flatten()
                .map(|view_box| Size {
                    width: view_box.width,
                    height: view_box.height,
                })
                .unwrap_or_default(),
            Element::Row => Size {
                width: sum_with_spacing(&shown_sizes, scene.number(node, "gap")?, true),
                height: shown_sizes
                    .iter()
                    .map(|size| size.height)
                    .fold(0.0, f64::max),
            },
            Element::Column => Size {
                width: shown_sizes
                    .iter()
                    .map(|size| size.width)
                    .fold(0.0, f64::max),
                height: sum_with_spacing(&shown_sizes, scene.number(node, "gap")?, false),
            },
            Element::Grid => {
                let (column_gap, row_gap) = grid_gaps(scene, node)?;
                grid_size(
                    &shown_sizes,
                    grid_columns(scene.number(node, "columns")?),
                    column_gap,
                    row_gap,
                )
            }
            Element::Inset => {
                let width = child_sizes.first().map_or(0.0, |size| size.width);
                let height = child_sizes.first().map_or(0.0, |size| size.height);
                Size {
                    width: width
                        + inset_margin(scene, node, "left_margin")?
                        + inset_margin(scene, node, "right_margin")?,
                    height: height
                        + inset_margin(scene, node, "top_margin")?
                        + inset_margin(scene, node, "bottom_margin")?,
                }
            }
            Element::Item
            | Element::Rect
            | Element::ClipRect
            | Element::Sdf
            | Element::SdfShape
            | Element::MouseArea
            | Element::DropArea
            | Element::Flickable
            | Element::Loader
            | Element::Timer
            | Element::Flex
            | Element::Custom
            // A terminal is as big as it is made: its grid follows its size,
            // not the other way round.
            | Element::Terminal => {
                let mut bounds = Size::default();
                for (child, size) in flow.iter().zip(child_sizes) {
                    bounds.width = bounds.width.max(scene.number(*child, "x")? + size.width);
                    bounds.height = bounds.height.max(scene.number(*child, "y")? + size.height);
                }
                if scene.element(node)? == Element::ClipRect
                    && scene.bool_value(node, "content_inside_border")?
                {
                    let border = scene.number(node, "border_width")?.max(0.0);
                    bounds.width += border * 2.0;
                    bounds.height += border * 2.0;
                }
                bounds
            }
        };
        self.implicit.insert(node, size);
        Ok(size)
    }

    fn requested_size(
        &self,
        scene: &Scene,
        node: NodeHandle,
        implicit: Size,
    ) -> Result<Size, LayoutError> {
        let attached = Attached::read(attached_layout(scene.current(node, "layout")?)?)?;
        // Percent bounds need a parent to be a percent of; here, before
        // placement, only lengths apply. A Flex or Grid resolves the rest.
        let length = |bound: Option<crate::attached::Bound>| match bound {
            Some(crate::attached::Bound::Length(value)) => Some(value),
            _ => None,
        };
        let implicit_width = positive(scene.number(node, "implicit_width")?);
        let implicit_height = positive(scene.number(node, "implicit_height")?);
        let width = positive(scene.number(node, "width")?)
            .or(length(attached.preferred_width))
            .or(implicit_width)
            .unwrap_or(implicit.width);
        let height = positive(scene.number(node, "height")?)
            .or(length(attached.preferred_height))
            .or(implicit_height)
            .unwrap_or(implicit.height);
        let clamp = |value: f64, minimum, maximum| {
            let minimum = length(minimum).unwrap_or(0.0);
            let maximum = length(maximum).unwrap_or(f64::INFINITY);
            value.max(minimum).min(maximum.max(minimum))
        };
        Ok(Size {
            width: clamp(width, attached.minimum_width, attached.maximum_width),
            height: clamp(height, attached.minimum_height, attached.maximum_height),
        })
    }

    pub(crate) fn resolve_children(
        &mut self,
        scene: &Scene,
        parent: NodeHandle,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let mut parent_geometry = self.geometry[&parent];
        let parent_element = scene.element(parent)?;
        if is_flex_root(scene, parent)? {
            return self.resolve_flex(scene, parent, parent_geometry, text, host);
        }
        if parent_element == Element::Custom {
            return self.resolve_custom(scene, parent, parent_geometry, text, host);
        }
        let mut inset = None;
        if parent_element == Element::ClipRect
            && scene.bool_value(parent, "content_inside_border")?
        {
            let border = scene.number(parent, "border_width")?.max(0.0);
            inset = Some(border);
            parent_geometry.x += border;
            parent_geometry.y += border;
            parent_geometry.width = (parent_geometry.width - border * 2.0).max(0.0);
            parent_geometry.height = (parent_geometry.height - border * 2.0).max(0.0);
        }
        let children = scene.children(parent)?;
        let packed = matches!(parent_element, Element::Row | Element::Column);
        // Which children the positioner packs: the visible ones. The rest
        // are still placed, where the next shown child would go, at their
        // own size, but take no room, no gap and no grid cell.
        let shown = if packed || parent_element == Element::Grid {
            children
                .iter()
                .map(|&child| Ok(scene.bool_value(child, "visible")? && !scene.is_exiting(child)))
                .collect::<Result<Vec<_>, LayoutError>>()?
        } else {
            Vec::new()
        };
        let shown_count = shown.iter().filter(|&&shown| shown).count();
        let spacing = if packed {
            scene.number(parent, "gap")?
        } else {
            0.0
        };
        // `justify`: where the run starts along the packed axis and what
        // extra goes between children, from the room left over.
        let (mut cursor, extra) = if packed {
            let horizontal = parent_element == Element::Row;
            let used = children
                .iter()
                .zip(&shown)
                .filter(|(_, shown)| **shown)
                .map(|(child, _)| {
                    let size = self.requested[child];
                    if horizontal { size.width } else { size.height }
                })
                .sum::<f64>()
                + spacing * shown_count.saturating_sub(1) as f64;
            let extent = if horizontal {
                parent_geometry.width
            } else {
                parent_geometry.height
            };
            justify_run(
                scene.string_value(parent, "justify")?,
                extent - used,
                shown_count,
            )?
        } else {
            (0.0, 0.0)
        };
        let spacing = spacing + extra;
        let columns = if parent_element == Element::Grid {
            grid_columns(scene.number(parent, "columns").unwrap_or(1.0))
        } else {
            1
        };
        let (column_spacing, row_spacing) = if parent_element == Element::Grid {
            grid_gaps(scene, parent)?
        } else {
            (0.0, 0.0)
        };
        let mut grid_widths = Vec::new();
        let mut grid_heights = Vec::new();
        if parent_element == Element::Grid {
            grid_widths.resize(columns, 0.0_f64);
            grid_heights.resize(shown_count.div_ceil(columns), 0.0_f64);
            let cells = children
                .iter()
                .zip(&shown)
                .filter(|(_, shown)| **shown)
                .map(|(child, _)| child);
            for (index, child) in cells.enumerate() {
                let size = self.requested[child];
                grid_widths[index % columns] = grid_widths[index % columns].max(size.width);
                grid_heights[index / columns] = grid_heights[index / columns].max(size.height);
            }
        }
        let alignment = if packed {
            Some(scene.string_value(parent, "align")?.to_owned())
        } else {
            None
        };
        let scrolled = if parent_element == Element::Flickable {
            Some((
                scene.number(parent, "content_x")?,
                scene.number(parent, "content_y")?,
            ))
        } else {
            None
        };

        // The grid cell the next shown child takes.
        let mut cell = 0;
        for (index, &child) in children.iter().enumerate() {
            if scene.is_exiting(child) {
                self.place_exiting(scene, parent, child, text, host)?;
                continue;
            }
            let size = self.requested[&child];
            let visible = shown.get(index).copied().unwrap_or(true);
            let anchors = anchors(scene.current(child, "anchors")?)?;
            reject_axis_conflict(parent_element, anchors)?;
            let mut geometry = Geometry {
                x: scene.number(child, "x")?,
                y: scene.number(child, "y")?,
                width: size.width,
                height: size.height,
            };
            let attached = Attached::read(attached_layout(scene.current(child, "layout")?)?)?;
            if parent_element == Element::Inset {
                let left = inset_margin(scene, parent, "left_margin")?;
                let right = inset_margin(scene, parent, "right_margin")?;
                let top = inset_margin(scene, parent, "top_margin")?;
                let bottom = inset_margin(scene, parent, "bottom_margin")?;
                if scene.bool_value(parent, "resize_child")? {
                    geometry.x = left;
                    geometry.y = top;
                    geometry.width = (parent_geometry.width - left - right).max(0.0);
                    geometry.height = (parent_geometry.height - top - bottom).max(0.0);
                } else {
                    geometry.x =
                        distributed_margin(parent_geometry.width - geometry.width, left, right);
                    geometry.y =
                        distributed_margin(parent_geometry.height - geometry.height, top, bottom);
                }
            }
            if let Some(alignment) = &alignment {
                let alignment = attached.align_self.as_deref().unwrap_or(alignment);
                align_across(parent_element, alignment, parent_geometry, &mut geometry)?;
            }
            apply_anchors(parent_geometry, anchors, &mut geometry);
            match parent_element {
                Element::Row => {
                    geometry.x = cursor;
                    if visible {
                        cursor += geometry.width + spacing;
                    }
                }
                Element::Column => {
                    geometry.y = cursor;
                    if visible {
                        cursor += geometry.height + spacing;
                    }
                }
                Element::Grid => {
                    let column = cell % columns;
                    // Past the last row when every child after it is hidden.
                    let row = (cell / columns).min(grid_heights.len());
                    geometry.x =
                        grid_widths[..column].iter().sum::<f64>() + column_spacing * column as f64;
                    geometry.y = grid_heights[..row].iter().sum::<f64>() + row_spacing * row as f64;
                    if visible {
                        cell += 1;
                    }
                }
                _ => {}
            }
            let local = Local::Placed {
                x: geometry.x,
                y: geometry.y,
                inset,
                scrolled,
                transition: (
                    scene.number(child, "transition_x")?,
                    scene.number(child, "transition_y")?,
                ),
            };
            let (x, y) = local.position(self.geometry[&parent]);
            geometry.x = x;
            geometry.y = y;
            self.local.insert(child, local);
            self.place(scene, child, geometry, text, host)?;
        }
        Ok(())
    }

    /// Fixes the box each node that has started to leave keeps, from where
    /// this layout last placed it, relative to its parent: before a pass
    /// overwrites either. A node this layout never placed is left for one
    /// that did, or for [`Layout::place_exiting`] to fix where it is now.
    pub(crate) fn capture_exit_frames(&self, scene: &Scene) {
        for node in scene.exiting_nodes() {
            if scene.exit_frame(node).is_some() {
                continue;
            }
            let Some(parent) = scene.parent(node).ok().flatten() else {
                continue;
            };
            let (Some(own), Some(around)) = (self.geometry.get(&node), self.geometry.get(&parent))
            else {
                continue;
            };
            scene.fix_exit_frame(
                node,
                [own.x - around.x, own.y - around.y, own.width, own.height],
            );
        }
    }

    /// Places a node on its way out: in the box it keeps relative to its
    /// parent, whatever its parent's rules would do with it, taking no room
    /// from its siblings. See [`morf_scene::Scene::begin_exit`].
    pub(crate) fn place_exiting(
        &mut self,
        scene: &Scene,
        parent: NodeHandle,
        child: NodeHandle,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let frame = match scene.exit_frame(child) {
            Some(frame) => frame,
            None => {
                // Never placed before it started to leave: where its own
                // `x` and `y` put it, at the size it asks for.
                let size = self.requested.get(&child).copied().unwrap_or_default();
                scene.fix_exit_frame(
                    child,
                    [
                        scene.number(child, "x")?,
                        scene.number(child, "y")?,
                        size.width,
                        size.height,
                    ],
                );
                scene.exit_frame(child).unwrap_or_default()
            }
        };
        let local = Local::Placed {
            x: frame[0],
            y: frame[1],
            inset: None,
            scrolled: None,
            transition: (0.0, 0.0),
        };
        let (x, y) = local.position(self.geometry[&parent]);
        self.local.insert(child, local);
        let geometry = Geometry {
            x,
            y,
            width: frame[2],
            height: frame[3],
        };
        self.place(scene, child, geometry, text, host)
    }

    /// Gives `node` its geometry and places what is under it -- unless
    /// nothing under it moved and it sits exactly where it did, when the
    /// last layout's answer for the whole subtree still holds.
    pub(crate) fn place(
        &mut self,
        scene: &Scene,
        node: NodeHandle,
        geometry: Geometry,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let before = self.geometry.insert(node, geometry);
        if scene.has_exits()
            && !scene.is_exiting(node)
            && let Some(parent) = scene.parent(node)?
            && let Some(around) = self.geometry.get(&parent)
        {
            scene.note_placed(
                node,
                [
                    geometry.x - around.x,
                    geometry.y - around.y,
                    geometry.width,
                    geometry.height,
                ],
            );
        }
        let status = self.status(scene, node)?;
        if !status.visit
            && let Some(before) = before
        {
            if before == geometry {
                return Ok(());
            }
            // Moved, not resized, with nothing under it changed: everything
            // under it moves with it, by the same arithmetic that put it
            // where it was.
            if before.width == geometry.width && before.height == geometry.height {
                return self.move_children(scene, node, geometry);
            }
        }
        let forcing = status.fresh && !self.forcing();
        if forcing {
            self.force(true);
        }
        let placed = self.resolve_children(scene, node, text, host);
        if forcing {
            self.force(false);
        }
        placed
    }
}
