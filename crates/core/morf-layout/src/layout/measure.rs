//! Measuring: the size each node would take on its own, and the size it asks
//! its parent for.

use morf_scene::{Element, NodeHandle, Scene};

use crate::attached::Attached;
use crate::custom::CustomLayout;
use crate::flex::FlexTree;
use crate::flex_style::is_flex_root;
use crate::geometry::{Size, TextMeasurer, TextOptions};
use crate::helpers::{
    LayoutError, attached_layout, grid_columns, grid_size, positive, sum_with_spacing,
    text_alignment, text_elide,
};
use crate::resolve_containers::grid_gaps;
use crate::transform::inset_margin;

use super::Layout;

impl Layout {
    pub(super) fn measure_implicit(
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
            // A mask is laid out in its owner's box, and a node on its way out
            // keeps its own: neither asks for room.
            if scene.is_mask(child) || scene.is_exiting(child) {
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
        let element = scene.element(node)?;
        if matches!(element, Element::Text | Element::TextInput) {
            let reflows = positive(scene.number(node, "width")?).is_none()
                && if element == Element::TextInput {
                    scene.bool_value(node, "multiline")? && scene.bool_value(node, "wrap")?
                } else {
                    scene.bool_value(node, "wrap")? || scene.string_value(node, "elide")? != "none"
                };
            if reflows {
                self.reflow_text.insert(node);
            } else {
                self.reflow_text.remove(&node);
            }
        }
        if element == Element::Text {
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
        let size = match element {
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
                    alignment: text_alignment(scene.directed_alignment(node)?)?,
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

    pub(super) fn requested_size(
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
}
