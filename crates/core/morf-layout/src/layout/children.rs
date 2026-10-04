//! Placing: where each child goes inside the box its parent was given, and
//! where an owner's mask goes inside the owner's.

use morf_scene::{Element, NodeHandle, Scene};

use crate::attached::Attached;
use crate::custom::CustomLayout;
use crate::distribute::align_across;
use crate::flex_style::is_flex_root;
use crate::geometry::{Geometry, TextMeasurer};
use crate::helpers::{
    LayoutError, anchors, apply_anchors, attached_layout, grid_columns, justify_run, positive,
    reject_axis_conflict,
};
use crate::incremental::Local;
use crate::resolve_containers::grid_gaps;
use crate::transform::{distributed_margin, inset_margin};

use super::Layout;

impl Layout {
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
            self.resolve_flex(scene, parent, parent_geometry, text, host)?;
            return self.place_mask(scene, parent, text, host);
        }
        if parent_element == Element::Custom {
            self.resolve_custom(scene, parent, parent_geometry, text, host)?;
            return self.place_mask(scene, parent, text, host);
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
        // Right to left: what is placed against the parent's sides goes
        // against the other ones (`morf_scene::direction`). A scroller's
        // content keeps its own origin.
        let rtl = parent_element != Element::Flickable && scene.is_rtl(parent);
        let packed = matches!(parent_element, Element::Row | Element::Column);
        // Which children the positioner packs: the visible ones. The rest
        // are still placed, where the next shown child would go, at their
        // own size, but take no room, no gap and no grid cell.
        let shown = if packed || parent_element == Element::Grid {
            children
                .iter()
                .map(|&child| {
                    Ok(scene.bool_value(child, "visible")?
                        && !scene.is_mask(child)
                        && !scene.is_exiting(child))
                })
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
            if scene.is_mask(child) {
                continue;
            }
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
            if rtl && mirrors(parent_element, anchors) {
                geometry.x = parent_geometry.width - geometry.x - geometry.width;
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
        self.place_mask(scene, parent, text, host)
    }

    /// Places `owner`'s mask, if it has one, in `owner`'s own box.
    ///
    /// Whatever kind of container the owner is, the mask is placed as a plain
    /// parent places a child — by its `x`, `y`, size and anchors — and with
    /// none of them it fills the box. It is never scrolled: a list that fades
    /// at its edges keeps its fade where the edges are.
    pub(crate) fn place_mask(
        &mut self,
        scene: &Scene,
        owner: NodeHandle,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let Some(mask) = scene.mask(owner) else {
            return Ok(());
        };
        let owner_geometry = self.geometry[&owner];
        let size = match self.requested.get(&mask) {
            Some(size) => *size,
            None => {
                let implicit = self.measure_implicit(scene, mask, text, host)?;
                self.requested_size(scene, mask, implicit)?
            }
        };
        let anchors = anchors(scene.current(mask, "anchors")?)?;
        let sized = positive(scene.number(mask, "width")?).is_some()
            || positive(scene.number(mask, "height")?).is_some();
        let mut geometry = Geometry {
            x: scene.number(mask, "x")?,
            y: scene.number(mask, "y")?,
            width: size.width,
            height: size.height,
        };
        if anchors.is_empty() && !sized {
            geometry.width = owner_geometry.width;
            geometry.height = owner_geometry.height;
        } else {
            apply_anchors(owner_geometry, anchors, &mut geometry);
        }
        let local = Local::Placed {
            x: geometry.x,
            y: geometry.y,
            inset: None,
            scrolled: None,
            transition: (
                scene.number(mask, "transition_x")?,
                scene.number(mask, "transition_y")?,
            ),
        };
        let (x, y) = local.position(owner_geometry);
        geometry.x = x;
        geometry.y = y;
        self.local.insert(mask, local);
        self.place(scene, mask, geometry, text, host)
    }
}

/// Whether a child's horizontal place is relative to its parent's sides --
/// packed by a row, column or grid, inset, or anchored left, right, centred or
/// filling -- and so mirrors right to left. A child placed by its `x` alone
/// keeps it.
fn mirrors(parent: Element, anchors: &std::collections::BTreeMap<String, morf_scene::Value>) -> bool {
    matches!(parent, Element::Row | Element::Column | Element::Grid | Element::Inset)
        || ["left", "right", "horizontal_center", "fill", "center_in"]
            .iter()
            .any(|key| crate::helpers::flag(anchors, key))
}
