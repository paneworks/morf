use morf_scene::{Element, NodeHandle, Scene};

use crate::geometry::{Geometry, Transform2D};
use crate::helpers::LayoutError;
use crate::layout::Layout;
use crate::transform::node_transform;

/// One hit-tested MouseArea or TextInput together with the tested point inside that node.
///
/// The two coordinate spaces are deliberately distinct. A hit test is queried
/// in *surface* space — the coordinates the compositor delivers, shared by
/// every node on the surface — while `local_x`/`local_y` are the same point
/// expressed inside the node that was hit, so a handler can divide by its own
/// width without knowing where any ancestor placed it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The topmost enabled MouseArea containing the point.
    pub node: NodeHandle,
    /// Point x inside `node`: `0.0` at its left edge, its width at the right,
    /// with every ancestor offset and transform already removed.
    pub local_x: f64,
    /// Point y inside `node`: `0.0` at its top edge, its height at the bottom.
    pub local_y: f64,
}

/// What a hit test is looking for: which kind of area, and which of those
/// it takes.
struct Target<'a> {
    elements: &'a [Element],
    accept: &'a dyn Fn(NodeHandle) -> bool,
}

impl Layout {
    /// Returns the topmost enabled MouseArea containing a surface-local point.
    pub fn hit_test(&self, scene: &Scene, x: f64, y: f64) -> Result<Option<Hit>, LayoutError> {
        self.hit_element(scene, &[Element::MouseArea], x, y, &|_| true)
    }

    /// The topmost enabled MouseArea under a point that `accept` takes.
    ///
    /// One that refuses is passed over, not in the way: an area taking only
    /// the right button laid over one taking the left has to let a left
    /// click through, or a right-click menu costs a button its click.
    pub fn hit_test_accepting(
        &self,
        scene: &Scene,
        x: f64,
        y: f64,
        accept: &dyn Fn(NodeHandle) -> bool,
    ) -> Result<Option<Hit>, LayoutError> {
        self.hit_element(scene, &[Element::MouseArea], x, y, accept)
    }

    /// The topmost enabled MouseArea or Flickable under a point that
    /// `accept` takes: where a wheel turn goes.
    ///
    /// A wheel bubbles. A button, a switch or a label's area laid over a
    /// scrolling page has no use for the wheel and must not swallow it, so
    /// `accept` refuses every node that would do nothing with it and the
    /// search walks on down the stack — through the node's ancestors first,
    /// since a node is tested after its children — until one would. A
    /// Flickable is a candidate beside the MouseAreas because it scrolls
    /// itself.
    pub fn wheel_hit_test(
        &self,
        scene: &Scene,
        x: f64,
        y: f64,
        accept: &dyn Fn(NodeHandle) -> bool,
    ) -> Result<Option<Hit>, LayoutError> {
        self.hit_element(
            scene,
            &[Element::MouseArea, Element::Flickable],
            x,
            y,
            accept,
        )
    }

    /// How far a Flickable's content reaches, measured from the content's
    /// own origin: the furthest right and bottom edge of its children, with
    /// the current scroll offset taken back out.
    ///
    /// `None` when the node has not been laid out.
    pub fn content_extent(&self, scene: &Scene, node: NodeHandle) -> Option<(f64, f64)> {
        let geometry = self.geometry(node)?;
        let content_x = scene.number(node, "content_x").unwrap_or(0.0);
        let content_y = scene.number(node, "content_y").unwrap_or(0.0);
        let (mut width, mut height) = (0.0_f64, 0.0_f64);
        for &child in scene.children(node).ok()? {
            if !scene.bool_value(child, "visible").unwrap_or(false) {
                continue;
            }
            let Some(child_geometry) = self.geometry(child) else {
                continue;
            };
            width = width.max(child_geometry.x + child_geometry.width - geometry.x + content_x);
            height = height.max(child_geometry.y + child_geometry.height - geometry.y + content_y);
        }
        Some((width, height))
    }

    /// Returns the topmost enabled DropArea containing a surface-local point.
    ///
    /// Separate from [`Layout::hit_test`] and blind to MouseAreas, as that one
    /// is blind to DropAreas: a drag passes over buttons on its way to a
    /// target, and a click passes over targets on its way to a button. Neither
    /// should stop the other.
    pub fn drop_hit_test(&self, scene: &Scene, x: f64, y: f64) -> Result<Option<Hit>, LayoutError> {
        self.hit_element(scene, &[Element::DropArea], x, y, &|_| true)
    }

    fn hit_element(
        &self,
        scene: &Scene,
        elements: &[Element],
        x: f64,
        y: f64,
        accept: &dyn Fn(NodeHandle) -> bool,
    ) -> Result<Option<Hit>, LayoutError> {
        for root in scene.roots().into_iter().rev() {
            let target = Target { elements, accept };
            if let Some(hit) = self.hit_node(scene, root, &target, Transform2D::IDENTITY, x, y)? {
                return Ok(Some(hit));
            }
        }
        Ok(None)
    }

    /// Converts a surface-local point into one node's own coordinates.
    ///
    /// Unlike [`Layout::hit_test`] the point need not land on the node, so a
    /// drag that has pulled off its handle still reports where the pointer is
    /// relative to that handle — negative to the left of it, past its width to
    /// the right. A node with no resolved geometry or a singular transform is
    /// unreachable by a pointer; the surface point is returned unchanged for
    /// it rather than failing the event.
    pub fn local_point(&self, scene: &Scene, node: NodeHandle, x: f64, y: f64) -> (f64, f64) {
        let Some(geometry) = self.geometry(node) else {
            return (x, y);
        };
        let Ok(transform) = self.chain_transform(scene, node) else {
            return (x, y);
        };
        transform
            .inverse_point(x, y)
            .map_or((x, y), |(local_x, local_y)| {
                (local_x - geometry.x, local_y - geometry.y)
            })
    }

    /// Collects enabled MouseArea and DropArea rectangles for the Wayland
    /// input region.
    pub fn input_geometry(&self, scene: &Scene) -> Result<Vec<Geometry>, LayoutError> {
        let mut rectangles = Vec::new();
        for root in scene.roots() {
            self.collect_input_geometry(scene, root, Transform2D::IDENTITY, &mut rectangles)?;
        }
        Ok(rectangles)
    }

    /// Accumulates the transform chain from the scene root down to one node.
    fn chain_transform(&self, scene: &Scene, node: NodeHandle) -> Result<Transform2D, LayoutError> {
        let mut chain = vec![node];
        let mut current = node;
        while let Some(parent) = scene.parent(current)? {
            chain.push(parent);
            current = parent;
        }
        let mut transform = Transform2D::IDENTITY;
        for node in chain.into_iter().rev() {
            let Some(geometry) = self.geometry(node) else {
                return Err(LayoutError::Scene(
                    "node has no resolved geometry".to_owned(),
                ));
            };
            transform = transform.then(node_transform(scene, node, geometry)?);
        }
        Ok(transform)
    }

    /// Every node asking for a blurred backdrop, with its corner radii.
    ///
    /// Separate from `input_geometry` because the two answer different
    /// questions about the same tree — where a click lands, and where the
    /// compositor should blur — and a node very often wants one without the
    /// other. A frosted panel is usually not interactive; a button usually does
    /// not want its own blur.
    pub fn backdrop_geometry(
        &self,
        scene: &Scene,
    ) -> Result<Vec<(Geometry, [f32; 4])>, LayoutError> {
        let mut regions = Vec::new();
        for root in scene.roots() {
            self.collect_backdrop_geometry(scene, root, Transform2D::IDENTITY, &mut regions)?;
        }
        Ok(regions)
    }

    fn collect_backdrop_geometry(
        &self,
        scene: &Scene,
        node: NodeHandle,
        inherited: Transform2D,
        regions: &mut Vec<(Geometry, [f32; 4])>,
    ) -> Result<(), LayoutError> {
        if !scene.bool_value(node, "visible")? {
            return Ok(());
        }
        let Some(geometry) = self.geometry(node) else {
            return Ok(());
        };
        let transform = inherited.then(node_transform(scene, node, geometry)?);
        if scene.bool_value(node, "backdrop_blur")? {
            regions.push((transform.bounds(geometry), corner_radii(scene, node)?));
        }
        for &child in scene.children(node)? {
            self.collect_backdrop_geometry(scene, child, transform, regions)?;
        }
        Ok(())
    }

    fn collect_input_geometry(
        &self,
        scene: &Scene,
        node: NodeHandle,
        inherited: Transform2D,
        rectangles: &mut Vec<Geometry>,
    ) -> Result<(), LayoutError> {
        if !scene.bool_value(node, "visible")? || !scene.bool_value(node, "enabled")? {
            return Ok(());
        }
        let Some(geometry) = self.geometry(node) else {
            return Ok(());
        };
        let transform = inherited.then(node_transform(scene, node, geometry)?);
        // A DropArea takes input too: the compositor sends a drag only to the
        // surface whose input region is under it; and a text input takes the
        // pointer itself.
        if matches!(
            scene.element(node)?,
            Element::MouseArea | Element::DropArea | Element::TextInput
        ) && let Some(geometry) = self.geometry(node)
        {
            rectangles.push(transform.bounds(geometry));
        }
        for &child in scene.children(node)? {
            self.collect_input_geometry(scene, child, transform, rectangles)?;
        }
        Ok(())
    }

    fn hit_node(
        &self,
        scene: &Scene,
        node: NodeHandle,
        target: &Target<'_>,
        inherited: Transform2D,
        x: f64,
        y: f64,
    ) -> Result<Option<Hit>, LayoutError> {
        if !scene.bool_value(node, "visible")? || !scene.bool_value(node, "enabled")? {
            return Ok(None);
        }
        let Some(geometry) = self.geometry(node) else {
            return Ok(None);
        };
        let transform = inherited.then(node_transform(scene, node, geometry)?);
        let Some((local_x, local_y)) = transform.inverse_point(x, y) else {
            return Ok(None);
        };
        let inside = local_x >= geometry.x
            && local_y >= geometry.y
            && local_x < geometry.x + geometry.width
            && local_y < geometry.y + geometry.height;
        if !inside && scene.bool_value(node, "clip")? {
            return Ok(None);
        }
        for &child in scene.paint_order(node)?.iter().rev() {
            if let Some(hit) = self.hit_node(scene, child, target, transform, x, y)? {
                return Ok(Some(hit));
            }
        }
        // The inverse point is measured in the space the node's own geometry
        // is resolved in, which is absolute; subtracting the node's origin is
        // what makes it node-local.
        Ok(
            (inside && wanted(scene.element(node)?, target.elements) && (target.accept)(node))
                .then_some(Hit {
                    node,
                    local_x: local_x - geometry.x,
                    local_y: local_y - geometry.y,
                }),
        )
    }
}

/// Whether a node of kind `found` answers a hit test looking for `sought`.
///
/// A text input answers the pointer's: a click places its caret and a drag
/// selects, which no MouseArea laid over it could do for it.
fn wanted(found: Element, sought: &[Element]) -> bool {
    sought.contains(&found) || (sought.contains(&Element::MouseArea) && found == Element::TextInput)
}

/// A node's four corner radii, falling back to the uniform one.
///
/// A negative per-corner radius means "unset", which is how the schema says it
/// without needing a nil-able number.
fn corner_radii(scene: &Scene, node: NodeHandle) -> Result<[f32; 4], LayoutError> {
    let uniform = scene.number(node, "radius").unwrap_or(0.0);
    let mut radii = [uniform as f32; 4];
    for (index, name) in [
        "top_left_radius",
        "top_right_radius",
        "bottom_right_radius",
        "bottom_left_radius",
    ]
    .into_iter()
    .enumerate()
    {
        if let Ok(value) = scene.number(node, name)
            && value >= 0.0
        {
            radii[index] = value as f32;
        }
    }
    Ok(radii)
}
