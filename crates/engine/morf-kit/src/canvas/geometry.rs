//! Where things are: world and screen points, the view's zoom and pan,
//! bounding boxes, hit tests, ports and resize handles.

use super::{Axes, Canvas, Item, Port, Shape};

fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0) };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

fn inside_polygon(p: [f64; 2], points: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = points.len();
    for i in 0..n {
        let (a, b) = (points[i], points[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}

/// The box `{x0, y0, x1, y1}` two corners span.
pub(super) fn spanned(a: [f64; 2], b: [f64; 2]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])]
}

impl Canvas {
    pub(super) fn world(&self, screen: [f64; 2]) -> [f64; 2] {
        [self.origin[0] + screen[0] / self.zoom[0], self.origin[1] + screen[1] / self.zoom[1]]
    }

    pub(super) fn screen(&self, world: [f64; 2]) -> [f64; 2] {
        [(world[0] - self.origin[0]) * self.zoom[0], (world[1] - self.origin[1]) * self.zoom[1]]
    }

    pub(super) fn view_box(&self) -> [f64; 4] {
        let end = self.world(self.size);
        [self.origin[0], self.origin[1], end[0], end[1]]
    }

    pub(super) fn snapped(&self, v: f64) -> f64 {
        if self.snap && self.grid > 0.0 { (v / self.grid).round() * self.grid } else { v }
    }

    pub(super) fn snapped_point(&self, p: [f64; 2]) -> [f64; 2] {
        [self.snapped(p[0]), self.snapped(p[1])]
    }

    pub(super) fn zooms(&self, axis: usize) -> bool {
        match self.axes {
            Axes::Both => true,
            Axes::X => axis == 0,
            Axes::Y => axis == 1,
        }
    }

    /// The view's centre kept inside `bounds`.
    pub(super) fn clamp_view(&mut self) {
        let Some(b) = self.bounds else { return };
        for axis in 0..2 {
            let half = self.size[axis] / self.zoom[axis] / 2.0;
            let centre = (self.origin[axis] + half).clamp(b[axis], b[axis + 2].max(b[axis]));
            self.origin[axis] = centre - half;
        }
    }

    /// Zooms by `factor` keeping the world point under `at` (screen) still.
    pub(super) fn zoom_about(&mut self, factor: f64, at: [f64; 2]) {
        let anchor = self.world(at);
        for axis in 0..2 {
            if self.zooms(axis) {
                self.zoom[axis] = (self.zoom[axis] * factor).clamp(self.min_zoom, self.max_zoom);
                self.origin[axis] = anchor[axis] - at[axis] / self.zoom[axis];
            }
        }
        self.clamp_view();
    }

    pub(super) fn zoom_to(&mut self, zoom: f64, at: [f64; 2]) {
        let current = self.zoom[if self.axes == Axes::Y { 1 } else { 0 }];
        if current > 0.0 {
            self.zoom_about(zoom / current, at);
        }
    }

    pub(super) fn pan_by(&mut self, screen: [f64; 2]) {
        for axis in 0..2 {
            if self.zooms(axis) {
                self.origin[axis] += screen[axis] / self.zoom[axis];
            }
        }
        self.clamp_view();
    }

    pub(super) fn centre(&self) -> [f64; 2] {
        [self.size[0] / 2.0, self.size[1] / 2.0]
    }

    /// Fits `area` (world) into the viewport, less the padding.
    pub(super) fn fit_box(&mut self, area: [f64; 4]) {
        let (w, h) = ((area[2] - area[0]).max(1e-9), (area[3] - area[1]).max(1e-9));
        let room = [(self.size[0] - 2.0 * self.fit_padding).max(1.0), (self.size[1] - 2.0 * self.fit_padding).max(1.0)];
        let both = (room[0] / w).min(room[1] / h).clamp(self.min_zoom, self.max_zoom);
        let fitted = [(room[0] / w).clamp(self.min_zoom, self.max_zoom), (room[1] / h).clamp(self.min_zoom, self.max_zoom)];
        let centre = [(area[0] + area[2]) / 2.0, (area[1] + area[3]) / 2.0];
        for axis in 0..2 {
            if self.zooms(axis) {
                self.zoom[axis] = if self.axes == Axes::Both { both } else { fitted[axis] };
                self.origin[axis] = centre[axis] - self.size[axis] / 2.0 / self.zoom[axis];
            }
        }
        self.clamp_view();
    }

    pub(super) fn bbox(&self, item: &Item) -> [f64; 4] {
        match &item.shape {
            Shape::Rect(r) | Shape::Ellipse(r) => [r[0], r[1], r[0] + r[2], r[1] + r[3]],
            Shape::Point(p, _) => [p[0], p[1], p[0], p[1]],
            Shape::Line(points, _) | Shape::Polygon(points) => {
                let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
                for p in points {
                    b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
                }
                if points.is_empty() { [0.0; 4] } else { b }
            }
        }
    }

    pub(super) fn everything(&self) -> Option<[f64; 4]> {
        let mut out: Option<[f64; 4]> = None;
        for item in &self.items {
            let b = self.bbox(item);
            out = Some(match out {
                None => b,
                Some(o) => [o[0].min(b[0]), o[1].min(b[1]), o[2].max(b[2]), o[3].max(b[3])],
            });
        }
        out.or(self.bounds)
    }

    fn hits(&self, item: &Item, p: [f64; 2]) -> bool {
        let tol = self.tolerance / self.zoom[0].min(self.zoom[1]).max(1e-9);
        match &item.shape {
            Shape::Rect(r) => p[0] >= r[0] && p[0] <= r[0] + r[2] && p[1] >= r[1] && p[1] <= r[1] + r[3],
            Shape::Ellipse(r) => {
                let (rx, ry) = (r[2] / 2.0, r[3] / 2.0);
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let (dx, dy) = ((p[0] - r[0] - rx) / rx, (p[1] - r[1] - ry) / ry);
                dx * dx + dy * dy <= 1.0
            }
            Shape::Point(at, radius) => {
                let (s, q) = (self.screen(*at), self.screen(p));
                (s[0] - q[0]).hypot(s[1] - q[1]) <= radius + self.tolerance
            }
            Shape::Line(points, width) => {
                let reach = tol + width / 2.0 / self.zoom[0].min(self.zoom[1]).max(1e-9);
                points.windows(2).any(|w| segment_distance(p, w[0], w[1]) <= reach)
            }
            Shape::Polygon(points) => {
                inside_polygon(p, points)
                    || (0..points.len()).any(|i| segment_distance(p, points[i], points[(i + 1) % points.len()]) <= tol)
            }
        }
    }

    /// The topmost selectable item at a world point.
    pub(super) fn item_at(&self, p: [f64; 2]) -> Option<String> {
        self.items.iter().rev().find(|item| item.selectable && self.hits(item, p)).map(|item| item.id.clone())
    }

    pub(super) fn port_at(&self, screen: [f64; 2]) -> Option<&Port> {
        self.ports.iter().rev().find(|port| {
            let s = self.screen(port.at);
            (s[0] - screen[0]).hypot(s[1] - screen[1]) <= self.port_radius
        })
    }

    /// Whether a wire from `from` may end on `to`.
    pub(super) fn joins(&self, from: &str, to: &Port) -> bool {
        let Some(from) = self.ports.iter().find(|p| p.id == from) else { return false };
        if from.id == to.id || (!from.item.is_empty() && from.item == to.item) {
            return false;
        }
        match (from.kind.as_str(), to.kind.as_str()) {
            ("in", "in") | ("out", "out") => false,
            _ => true,
        }
    }

    /// The one selected box a handle may resize: its id and `{x, y, w, h}`.
    pub(super) fn resizable_box(&self) -> Option<(String, [f64; 4])> {
        if !self.resizable || self.selection.len() != 1 {
            return None;
        }
        let item = self.items.iter().find(|i| i.id == self.selection[0])?;
        match &item.shape {
            Shape::Rect(r) | Shape::Ellipse(r) => Some((item.id.clone(), *r)),
            _ => None,
        }
    }

    /// The handle of the selected box under a screen point, if any.
    pub(super) fn handle_at(&self, screen: [f64; 2]) -> Option<&'static str> {
        let (_, r) = self.resizable_box()?;
        let (x0, y0) = (r[0], r[1]);
        let (x1, y1) = (r[0] + r[2], r[1] + r[3]);
        let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let points = [
            ("nw", [x0, y0]), ("n", [xm, y0]), ("ne", [x1, y0]), ("e", [x1, ym]),
            ("se", [x1, y1]), ("s", [xm, y1]), ("sw", [x0, y1]), ("w", [x0, ym]),
        ];
        let reach = self.port_radius.max(6.0);
        points.iter().find(|(_, p)| {
            let s = self.screen(*p);
            (s[0] - screen[0]).abs() <= reach && (s[1] - screen[1]).abs() <= reach
        }).map(|(name, _)| *name)
    }

    /// A box dragged by `handle` by `d` (world), kept to the minimum, on
    /// the grid when snapping, Shift keeping its shape.
    pub(super) fn resized(&self, handle: &str, start: [f64; 4], d: [f64; 2], keep: bool) -> [f64; 4] {
        let (mut x0, mut y0) = (start[0], start[1]);
        let (mut x1, mut y1) = (start[0] + start[2], start[1] + start[3]);
        if handle.contains('w') { x0 = self.snapped(x0 + d[0]); }
        if handle.contains('e') { x1 = self.snapped(x1 + d[0]); }
        if handle.starts_with('n') { y0 = self.snapped(y0 + d[1]); }
        if handle.starts_with('s') { y1 = self.snapped(y1 + d[1]); }
        let m = self.min_item;
        if x1 - x0 < m {
            if handle.contains('w') { x0 = x1 - m } else { x1 = x0 + m }
        }
        if y1 - y0 < m {
            if handle.starts_with('n') { y0 = y1 - m } else { y1 = y0 + m }
        }
        if keep && start[2] > 0.0 && start[3] > 0.0 {
            let aspect = start[2] / start[3];
            let (w, h) = (x1 - x0, y1 - y0);
            let corner = (handle.contains('w') || handle.contains('e')) && handle.len() == 2;
            let (w, h) = if corner || handle == "e" || handle == "w" {
                if corner && h * aspect > w { (h * aspect, h) } else { (w, w / aspect) }
            } else {
                (h * aspect, h)
            };
            if handle.contains('w') { x0 = x1 - w } else { x1 = x0 + w }
            if handle.starts_with('n') { y0 = y1 - h } else { y1 = y0 + h }
        }
        [x0, y0, x1 - x0, y1 - y0]
    }
}
