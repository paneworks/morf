//! Placing the children of the two containers that place their own.
//!
//! Split from `layout` at the line gate: a flex root hands its whole
//! subtree to Taffy, a custom container hands its children to the host's
//! `place`, and both then let each leaf place its own children by the
//! engine's ordinary rules.

use morf_scene::{Element, FastMap, NodeHandle, Scene};

use crate::custom::CustomLayout;
use crate::flex::FlexTree;
use crate::geometry::{Geometry, Size, TextMeasurer};
use crate::helpers::{LayoutError, positive};
use crate::incremental::Local;
use crate::layout::Layout;

impl Layout {
    /// Places everything under a flex root from one Taffy pass at the
    /// root's resolved size, then lets each leaf place its own children.
    pub(crate) fn resolve_flex(
        &mut self,
        scene: &Scene,
        root: NodeHandle,
        geometry: Geometry,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        let mut flex = FlexTree::build(scene, root)?;
        flex.set_root_size(geometry.width, geometry.height)?;
        flex.compute(
            scene,
            taffy::prelude::Size {
                width: taffy::prelude::AvailableSpace::Definite(geometry.width as f32),
                height: taffy::prelude::AvailableSpace::Definite(geometry.height as f32),
            },
            &self.requested,
            text,
        )?;
        let placed = flex.geometries(scene, geometry)?;
        if self.dirty.is_some() {
            self.forget_hidden_flex_children(scene, root)?;
        }
        let mut containers = vec![root];
        for (node, mut placed, leaf, (x, y)) in placed {
            if !leaf {
                containers.push(node);
            }
            let transition = (
                scene.number(node, "transition_x")?,
                scene.number(node, "transition_y")?,
            );
            self.local.insert(node, Local::Flexed { x, y, transition });
            placed.x += transition.0;
            placed.y += transition.1;
            if leaf {
                self.place(scene, node, placed, text, host)?;
            } else {
                self.geometry.insert(node, placed);
                self.place_mask(scene, node, text, host)?;
            }
        }
        // Taffy is not shown a child on its way out; it keeps its own box.
        for container in containers {
            for &child in scene.children(container)? {
                if scene.is_exiting(child) {
                    self.place_exiting(scene, container, child, text, host)?;
                }
            }
        }
        Ok(())
    }

    /// Drops what the last layout said about the hidden children of a flex
    /// root and of the flex containers inside it.
    ///
    /// Taffy is never shown a hidden child, so a whole pass gives it and
    /// everything under it no geometry at all; a layout brought up to date
    /// must not keep the geometry it had while it was shown.
    fn forget_hidden_flex_children(
        &mut self,
        scene: &Scene,
        container: NodeHandle,
    ) -> Result<(), LayoutError> {
        for &child in scene.children(container)? {
            if !scene.bool_value(child, "visible")? {
                let mut pending = vec![child];
                while let Some(node) = pending.pop() {
                    self.local.remove(&node);
                    if self.geometry.remove(&node).is_some() {
                        pending.extend(scene.children(node)?.iter().copied());
                    }
                }
            } else if crate::flex_style::is_flex_root(scene, child)? {
                self.forget_hidden_flex_children(scene, child)?;
            }
        }
        Ok(())
    }

    /// Places a custom container's children where its `place` function says.
    pub(crate) fn resolve_custom(
        &mut self,
        scene: &Scene,
        parent: NodeHandle,
        geometry: Geometry,
        text: &mut impl TextMeasurer,
        host: &mut dyn CustomLayout,
    ) -> Result<(), LayoutError> {
        // Masks are placed with their owner, not in the flow.
        // The host places the children in the flow; one on its way out keeps
        // its own box.
        let (leaving, children): (Vec<NodeHandle>, Vec<NodeHandle>) = scene
            .children(parent)?
            .iter()
            .filter(|child| !scene.is_mask(**child))
            .partition(|&&child| scene.is_exiting(child));
        for child in leaving {
            self.place_exiting(scene, parent, child, text, host)?;
        }
        let sizes = children
            .iter()
            .map(|child| self.requested[child])
            .collect::<Vec<_>>();
        let bounds = Size {
            width: geometry.width,
            height: geometry.height,
        };
        let placed = host
            .place(parent, bounds, &sizes)
            .map_err(LayoutError::Scene)?;
        for (index, &child) in children.iter().enumerate() {
            let relative = placed.get(index).copied().unwrap_or(Geometry {
                x: 0.0,
                y: 0.0,
                width: sizes[index].width,
                height: sizes[index].height,
            });
            let local = Local::Custom {
                x: relative.x,
                y: relative.y,
                transition: (
                    scene.number(child, "transition_x")?,
                    scene.number(child, "transition_y")?,
                ),
            };
            let (x, y) = local.position(geometry);
            self.local.insert(child, local);
            let child_geometry = Geometry {
                x,
                y,
                width: relative.width,
                height: relative.height,
            };
            self.place(scene, child, child_geometry, text, host)?;
        }
        Ok(())
    }

    /// Text nodes whose resolved width is not the width they were measured
    /// at, and to whom that matters.
    pub(crate) fn texts_to_remeasure(
        &self,
        scene: &Scene,
    ) -> Result<FastMap<NodeHandle, f64>, LayoutError> {
        let mut widths = FastMap::default();
        for (node, geometry) in &self.geometry {
            let element = scene.element(*node)?;
            if !matches!(element, Element::Text | Element::TextInput) {
                continue;
            }
            if positive(scene.number(*node, "width")?).is_some() {
                continue;
            }
            let wraps = if element == Element::TextInput {
                scene.bool_value(*node, "multiline")? && scene.bool_value(*node, "wrap")?
            } else {
                scene.bool_value(*node, "wrap")? || scene.string_value(*node, "elide")? != "none"
            };
            let measured = self.implicit.get(node).map_or(0.0, |size| size.width);
            if wraps && geometry.width > 0.0 && (geometry.width - measured).abs() > 0.5 {
                widths.insert(*node, geometry.width);
            }
        }
        Ok(widths)
    }

    /// Returns the resolved geometry for a node in the computed tree.
    pub fn geometry(&self, node: NodeHandle) -> Option<Geometry> {
        self.geometry.get(&node).copied()
    }

    /// Iterates over every resolved node geometry.
    pub fn geometries(&self) -> impl Iterator<Item = (NodeHandle, Geometry)> + '_ {
        self.geometry
            .iter()
            .map(|(node, geometry)| (*node, *geometry))
    }

    /// Returns the bottom-up implicit size for a node.
    pub fn implicit_size(&self, node: NodeHandle) -> Option<Size> {
        self.implicit.get(&node).copied()
    }
}

/// A grid's gaps: `column_gap` and `row_gap`, or `gap` for both.
pub(crate) fn grid_gaps(scene: &Scene, node: NodeHandle) -> Result<(f64, f64), LayoutError> {
    let gap = scene.number(node, "gap")?;
    let pick = |specific: f64| if specific > 0.0 { specific } else { gap };
    Ok((
        pick(scene.number(node, "column_gap")?),
        pick(scene.number(node, "row_gap")?),
    ))
}
