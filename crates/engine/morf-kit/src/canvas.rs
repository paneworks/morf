//! `Canvas`: a world a viewport looks into -- panned, zoomed, and holding
//! items that are picked, selected, moved, connected and drawn. A node
//! graph, a whiteboard, a map, an image viewer, a zoomable chart, a
//! timeline and a diagram are each a Canvas drawn differently.
//!
//! The world is in its own units; the viewport shows it from `view_x`,
//! `view_y` (the world point at its top-left corner) at `zoom` screen
//! pixels to a unit. The items are the configuration's, given as tables
//! the archetype hit-tests; it never holds their data, only their shapes.
//!
//! Settings: `zoom`, `view_x`, `view_y`, `min_zoom` (0.05), `max_zoom`
//! (40), `zoom_step` (1.2 a notch), `axes` (`"both"`, `"x"`, `"y"`: what
//! zooms and pans -- a timeline is `"x"`), `bounds` (`{ x0, y0, x1, y1 }`:
//! the view's centre stays inside), `grid` (world units; 0 none) and
//! `snap` (moves and drawing land on it), `tool` (`"select"`, `"pan"`,
//! `"point"`, `"line"`, `"rect"`, `"ellipse"`, `"polyline"`, `"polygon"`,
//! `"freehand"`, `"connect"`, `"brush"`, `"zoom"`), `items` (a list of `{
//! id, x, y, w, h }` with `shape` `"rect"` (the default), `"ellipse"`,
//! `"point"` (x, y and a radius `r` in pixels), `"line"` or `"polygon"`
//! (`points`, flat; a line's `width` in pixels), and `selectable` (true);
//! the last is topmost), `ports` (`{ id, item, x, y, kind }`: where wires
//! start and end; `kind` `"in"` or `"out"` joins only the other),
//! `port_radius` (px, 8), `selection` (ids), `multi_select` (true),
//! `movable` (true), `resizable` (false: the one selected box -- a rect
//! or an ellipse -- has eight handles a press drags to resize it, on the
//! grid with `snap`, Shift keeping its shape), `min_item` (world units, 8),
//! `wheel_zooms` (false: the wheel pans and Ctrl with it
//! zooms; true: it zooms, as a map does), `fit_padding` (px, 24) and
//! `hit_tolerance` (px, 6).
//!
//! State: `view_x`, `view_y`, `zoom` (`zoom_x`, `zoom_y` apart), the
//! `viewport_width` and `viewport_height`, `tool`, `selection` (ids) and
//! `selected_count`, `hovered` (an item id, or ""), `hovered_port`,
//! `pointer_x`, `pointer_y` (the world point under the pointer) and
//! `pointer_inside`, `gesture` (`"none"`, `"pan"`, `"move"`, `"band"`,
//! `"draw"`, `"draft"`, `"connect"`, `"brush"`, `"zoom"`), `move_dx`,
//! `move_dy` (a move under way, snapped), `band` (`{ x0, y0, x1, y1 }`, the
//! rubber band or brush or zoom box), `draft` (a shape being drawn: its
//! points, flat), `connect_from`, `connect_to`, `connect_x`, `connect_y` (a
//! wire being pulled and where it reaches), `resize` (`{ x, y, w, h }`: the
//! box a resize under way has reached), `resize_id`, `hovered_handle` (a
//! handle under the pointer: `"n"`, `"ne"`, ... or "").
//!
//! Events: the base's; `"resize"` (width, height); `"pressed"` (x, y,
//! width, height, button, modifiers); `"dragged"` (x, y, width, height,
//! modifiers); `"released"`; `"hover"` (x, y); `"exited"`;
//! `"double_clicked"` (x, y); `"wheel"` (steps x, y, pixels x, y, x, y,
//! modifiers); `"pinch"` (scale, phase, x, y); `"key"` (name, modifiers);
//! and the calls `"fit"`, `"zoom_by"` (factor, x, y), `"set_view"` (x, y,
//! zoom), `"center_on"` (x, y), `"cancel"`. Middle-button drags pan under
//! any tool; a right press asks for a context menu.
//!
//! Keys: the arrows nudge the selection (a grid step, or a pixel; Shift
//! ten) or, with none, pan; `+` and `-` zoom about the centre, `0` resets
//! the zoom, Home fits everything; Ctrl+A selects all; Delete asks to
//! delete the selection; Backspace takes back a draft's last point;
//! Return finishes a draft (or activates the one selected item); Escape
//! cancels what is under way, then the selection; Tab and Shift+Tab walk
//! the items, and past the last go on.
//!
//! Signals: `view_changed` (x, y, zoom), `selection_changed` (ids),
//! `hovered` (id), `moving` (dx, dy), `moved` (ids, dx, dy), `drawn` (tool,
//! points), `connected` (from, to), `connect_dropped` (from, x, y),
//! `activated` (id or "", x, y), `context` (id or "", x, y), `brushed` (x0,
//! y0, x1, y1), `deleted` (ids), `resized` (id, x, y, w, h).

use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axes {
    Both,
    X,
    Y,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Select,
    Pan,
    Point,
    Line,
    Rect,
    Ellipse,
    Polyline,
    Polygon,
    Freehand,
    Connect,
    Brush,
    Zoom,
}

impl Tool {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "select" => Self::Select,
            "pan" => Self::Pan,
            "point" => Self::Point,
            "line" => Self::Line,
            "rect" => Self::Rect,
            "ellipse" => Self::Ellipse,
            "polyline" => Self::Polyline,
            "polygon" => Self::Polygon,
            "freehand" => Self::Freehand,
            "connect" => Self::Connect,
            "brush" => Self::Brush,
            "zoom" => Self::Zoom,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Pan => "pan",
            Self::Point => "point",
            Self::Line => "line",
            Self::Rect => "rect",
            Self::Ellipse => "ellipse",
            Self::Polyline => "polyline",
            Self::Polygon => "polygon",
            Self::Freehand => "freehand",
            Self::Connect => "connect",
            Self::Brush => "brush",
            Self::Zoom => "zoom",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    Rect([f64; 4]),
    Ellipse([f64; 4]),
    /// A point and its radius in pixels.
    Point([f64; 2], f64),
    /// Points and a width in pixels.
    Line(Vec<[f64; 2]>, f64),
    Polygon(Vec<[f64; 2]>),
}

#[derive(Clone, Debug, PartialEq)]
struct Item {
    id: String,
    shape: Shape,
    selectable: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Port {
    id: String,
    item: String,
    at: [f64; 2],
    kind: String,
}

/// What a press started.
#[derive(Clone, Debug, PartialEq)]
enum Gesture {
    None,
    /// From the screen point pressed and the view then.
    Pan { from: [f64; 2], origin: [f64; 2] },
    /// From the world point pressed; the item pressed, and whether the
    /// press should narrow the selection to it if nothing moves.
    Move { from: [f64; 2], item: String, narrow: bool },
    Band { from: [f64; 2], to: [f64; 2], additive: bool },
    /// A line, rect or ellipse: from one corner to the other.
    Draw { from: [f64; 2], to: [f64; 2] },
    Freehand,
    Connect { from: String },
    Brush { from: [f64; 2], to: [f64; 2] },
    /// An item's box dragged by a handle: which, from where, the box then
    /// and the box it has reached.
    Resize { id: String, handle: &'static str, from: [f64; 2], start: [f64; 4], now: [f64; 4] },
    Zoom { from: [f64; 2], to: [f64; 2], out: bool },
}

pub(crate) struct Canvas {
    pub(crate) base: ControlState,
    origin: [f64; 2],
    zoom: [f64; 2],
    size: [f64; 2],
    min_zoom: f64,
    max_zoom: f64,
    zoom_step: f64,
    axes: Axes,
    bounds: Option<[f64; 4]>,
    grid: f64,
    snap: bool,
    tool: Tool,
    items: Vec<Item>,
    ports: Vec<Port>,
    port_radius: f64,
    selection: Vec<String>,
    multi_select: bool,
    movable: bool,
    wheel_zooms: bool,
    fit_padding: f64,
    tolerance: f64,
    hovered: String,
    hovered_port: String,
    pointer: Option<[f64; 2]>,
    gesture: Gesture,
    move_delta: [f64; 2],
    /// The points of a shape being drawn: a polyline's clicks, a stroke.
    draft: Vec<[f64; 2]>,
    connect_to: String,
    connect_at: [f64; 2],
    /// The zoom a pinch began at.
    pinch_from: Option<[f64; 2]>,
    resizable: bool,
    min_item: f64,
    hovered_handle: &'static str,
}

/// Every field a skin reads, by name, for diffing.
type Fields = Vec<(String, IpcValue)>;

fn list(values: Vec<IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::List(values)))
}

fn ids(values: &[String]) -> IpcValue {
    list(values.iter().map(|id| IpcValue::from(id.as_str())).collect())
}

fn flat(points: &[[f64; 2]]) -> IpcValue {
    list(points.iter().flat_map(|p| [p[0].into(), p[1].into()]).collect())
}

fn entries(value: &IpcValue) -> Vec<IpcValue> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items.clone(),
            IpcTable::Map(map) => map.values().cloned().collect(),
        },
        _ => Vec::new(),
    }
}

fn field<'a>(value: &'a IpcValue, name: &str) -> Option<&'a IpcValue> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::Map(map) => map.get(name),
            IpcTable::List(_) => None,
        },
        _ => None,
    }
}

/// An id as text: a number names an item as well as a string does.
fn id_of(value: Option<&IpcValue>) -> Option<String> {
    match value? {
        IpcValue::String(s) => Some(s.clone()),
        IpcValue::Integer(n) => Some(n.to_string()),
        IpcValue::Number(n) if n.fract() == 0.0 => Some((*n as i64).to_string()),
        IpcValue::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn points_of(value: Option<&IpcValue>) -> Vec<[f64; 2]> {
    let numbers: Vec<f64> = value.map(entries).unwrap_or_default().iter().filter_map(|v| number(Some(v))).collect();
    numbers.chunks_exact(2).map(|p| [p[0], p[1]]).collect()
}

fn id_list(value: &IpcValue) -> Vec<String> {
    entries(value).iter().filter_map(|v| id_of(Some(v))).collect()
}

fn item_from(value: &IpcValue) -> Option<Item> {
    let id = id_of(field(value, "id"))?;
    let n = |name: &str| number(field(value, name)).unwrap_or(0.0);
    let shape = match text(field(value, "shape")).unwrap_or("rect") {
        "ellipse" | "circle" => Shape::Ellipse([n("x"), n("y"), n("w"), n("h")]),
        "point" => Shape::Point([n("x"), n("y")], number(field(value, "r")).unwrap_or(6.0)),
        "line" => Shape::Line(points_of(field(value, "points")), number(field(value, "width")).unwrap_or(2.0)),
        "polygon" => Shape::Polygon(points_of(field(value, "points"))),
        _ => Shape::Rect([n("x"), n("y"), n("w"), n("h")]),
    };
    let selectable = !matches!(field(value, "selectable"), Some(IpcValue::Boolean(false)));
    Some(Item { id, shape, selectable })
}

fn port_from(value: &IpcValue) -> Option<Port> {
    Some(Port {
        id: id_of(field(value, "id"))?,
        item: id_of(field(value, "item")).unwrap_or_default(),
        at: [number(field(value, "x"))?, number(field(value, "y"))?],
        kind: text(field(value, "kind")).unwrap_or("").to_owned(),
    })
}

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
fn spanned(a: [f64; 2], b: [f64; 2]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])]
}

impl Canvas {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            origin: [0.0, 0.0],
            zoom: [1.0, 1.0],
            size: [0.0, 0.0],
            min_zoom: 0.05,
            max_zoom: 40.0,
            zoom_step: 1.2,
            axes: Axes::Both,
            bounds: None,
            grid: 0.0,
            snap: false,
            tool: Tool::Select,
            items: Vec::new(),
            ports: Vec::new(),
            port_radius: 8.0,
            selection: Vec::new(),
            multi_select: true,
            movable: true,
            wheel_zooms: false,
            fit_padding: 24.0,
            tolerance: 6.0,
            hovered: String::new(),
            hovered_port: String::new(),
            pointer: None,
            gesture: Gesture::None,
            move_delta: [0.0, 0.0],
            draft: Vec::new(),
            connect_to: String::new(),
            connect_at: [0.0, 0.0],
            pinch_from: None,
            resizable: false,
            min_item: 8.0,
            hovered_handle: "",
        }
    }

    fn gesture_name(&self) -> &'static str {
        match &self.gesture {
            Gesture::None if !self.draft.is_empty() => "draft",
            Gesture::None => "none",
            Gesture::Pan { .. } => "pan",
            Gesture::Move { .. } => "move",
            Gesture::Band { .. } => "band",
            Gesture::Draw { .. } => "draw",
            Gesture::Freehand => "draft",
            Gesture::Connect { .. } => "connect",
            Gesture::Brush { .. } => "brush",
            Gesture::Zoom { .. } => "zoom",
            Gesture::Resize { .. } => "resize",
        }
    }

    fn band(&self) -> Option<[f64; 4]> {
        match &self.gesture {
            Gesture::Band { from, to, .. } | Gesture::Zoom { from, to, .. } => Some(spanned(*from, *to)),
            Gesture::Brush { from, to } => Some(self.brushed(*from, *to)),
            _ => None,
        }
    }

    /// A brush spans the whole view across the axes that do not zoom.
    fn brushed(&self, from: [f64; 2], to: [f64; 2]) -> [f64; 4] {
        let mut b = spanned(from, to);
        let view = self.view_box();
        match self.axes {
            Axes::X => (b[1], b[3]) = (view[1], view[3]),
            Axes::Y => (b[0], b[2]) = (view[0], view[2]),
            Axes::Both => {}
        }
        b
    }

    fn draft_points(&self) -> Vec<[f64; 2]> {
        match &self.gesture {
            Gesture::Draw { from, to } => vec![*from, *to],
            _ => self.draft.clone(),
        }
    }

    fn fields(&self) -> Fields {
        let pointer = self.pointer.unwrap_or([0.0, 0.0]);
        let band = self.band().map(|b| list(b.iter().map(|v| (*v).into()).collect())).unwrap_or(IpcValue::Nil);
        let from = match &self.gesture {
            Gesture::Connect { from } => from.clone(),
            _ => String::new(),
        };
        vec![
            ("view_x".into(), self.origin[0].into()),
            ("view_y".into(), self.origin[1].into()),
            ("zoom".into(), self.zoom[if self.axes == Axes::Y { 1 } else { 0 }].into()),
            ("zoom_x".into(), self.zoom[0].into()),
            ("zoom_y".into(), self.zoom[1].into()),
            ("viewport_width".into(), self.size[0].into()),
            ("viewport_height".into(), self.size[1].into()),
            ("tool".into(), self.tool.name().into()),
            ("selection".into(), ids(&self.selection)),
            ("selected_count".into(), (self.selection.len() as i64).into()),
            ("hovered".into(), self.hovered.as_str().into()),
            ("hovered_port".into(), self.hovered_port.as_str().into()),
            ("pointer_x".into(), pointer[0].into()),
            ("pointer_y".into(), pointer[1].into()),
            ("pointer_inside".into(), self.pointer.is_some().into()),
            ("gesture".into(), self.gesture_name().into()),
            ("move_dx".into(), self.move_delta[0].into()),
            ("move_dy".into(), self.move_delta[1].into()),
            ("band".into(), band),
            ("draft".into(), flat(&self.draft_points())),
            ("connect_from".into(), from.into()),
            ("connect_to".into(), self.connect_to.as_str().into()),
            ("connect_x".into(), self.connect_at[0].into()),
            ("connect_y".into(), self.connect_at[1].into()),
            ("grid".into(), self.grid.into()),
            (
                "resize".into(),
                match &self.gesture {
                    Gesture::Resize { now, .. } => list(now.iter().map(|v| (*v).into()).collect()),
                    _ => IpcValue::Nil,
                },
            ),
            (
                "resize_id".into(),
                match &self.gesture {
                    Gesture::Resize { id, .. } => id.as_str().into(),
                    _ => "".into(),
                },
            ),
            ("hovered_handle".into(), self.hovered_handle.into()),
        ]
    }

    /// Runs `change` and says what it moved: every field that differs, the
    /// view and the selection signals when they changed.
    fn changing(&mut self, change: impl FnOnce(&mut Self, &mut Effects)) -> Effects {
        let before = self.fields();
        let (origin, zoom, selection, hovered) = (self.origin, self.zoom, self.selection.clone(), self.hovered.clone());
        let mut effects = Effects::default();
        change(self, &mut effects);
        let mut out = Effects::default();
        for (name, value) in self.fields() {
            if before.iter().find(|(n, _)| *n == name).map(|(_, v)| v) != Some(&value) {
                out.set(&name, value);
            }
        }
        if self.origin != origin || self.zoom != zoom {
            out.raise("view_changed", vec![self.origin[0].into(), self.origin[1].into(), self.zoom[0].into()]);
        }
        if self.selection != selection {
            out.raise("selection_changed", vec![ids(&self.selection)]);
        }
        if self.hovered != hovered {
            out.raise("hovered", vec![self.hovered.as_str().into()]);
        }
        out.signals.extend(effects.signals);
        out.handled = effects.handled;
        out
    }

    fn world(&self, screen: [f64; 2]) -> [f64; 2] {
        [self.origin[0] + screen[0] / self.zoom[0], self.origin[1] + screen[1] / self.zoom[1]]
    }

    fn screen(&self, world: [f64; 2]) -> [f64; 2] {
        [(world[0] - self.origin[0]) * self.zoom[0], (world[1] - self.origin[1]) * self.zoom[1]]
    }

    fn view_box(&self) -> [f64; 4] {
        let end = self.world(self.size);
        [self.origin[0], self.origin[1], end[0], end[1]]
    }

    fn snapped(&self, v: f64) -> f64 {
        if self.snap && self.grid > 0.0 { (v / self.grid).round() * self.grid } else { v }
    }

    fn snapped_point(&self, p: [f64; 2]) -> [f64; 2] {
        [self.snapped(p[0]), self.snapped(p[1])]
    }

    fn zooms(&self, axis: usize) -> bool {
        match self.axes {
            Axes::Both => true,
            Axes::X => axis == 0,
            Axes::Y => axis == 1,
        }
    }

    /// The view's centre kept inside `bounds`.
    fn clamp_view(&mut self) {
        let Some(b) = self.bounds else { return };
        for axis in 0..2 {
            let half = self.size[axis] / self.zoom[axis] / 2.0;
            let centre = (self.origin[axis] + half).clamp(b[axis], b[axis + 2].max(b[axis]));
            self.origin[axis] = centre - half;
        }
    }

    /// Zooms by `factor` keeping the world point under `at` (screen) still.
    fn zoom_about(&mut self, factor: f64, at: [f64; 2]) {
        let anchor = self.world(at);
        for axis in 0..2 {
            if self.zooms(axis) {
                self.zoom[axis] = (self.zoom[axis] * factor).clamp(self.min_zoom, self.max_zoom);
                self.origin[axis] = anchor[axis] - at[axis] / self.zoom[axis];
            }
        }
        self.clamp_view();
    }

    fn zoom_to(&mut self, zoom: f64, at: [f64; 2]) {
        let current = self.zoom[if self.axes == Axes::Y { 1 } else { 0 }];
        if current > 0.0 {
            self.zoom_about(zoom / current, at);
        }
    }

    fn pan_by(&mut self, screen: [f64; 2]) {
        for axis in 0..2 {
            if self.zooms(axis) {
                self.origin[axis] += screen[axis] / self.zoom[axis];
            }
        }
        self.clamp_view();
    }

    fn centre(&self) -> [f64; 2] {
        [self.size[0] / 2.0, self.size[1] / 2.0]
    }

    /// Fits `area` (world) into the viewport, less the padding.
    fn fit_box(&mut self, area: [f64; 4]) {
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

    fn bbox(&self, item: &Item) -> [f64; 4] {
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

    fn everything(&self) -> Option<[f64; 4]> {
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
    fn item_at(&self, p: [f64; 2]) -> Option<String> {
        self.items.iter().rev().find(|item| item.selectable && self.hits(item, p)).map(|item| item.id.clone())
    }

    fn port_at(&self, screen: [f64; 2]) -> Option<&Port> {
        self.ports.iter().rev().find(|port| {
            let s = self.screen(port.at);
            (s[0] - screen[0]).hypot(s[1] - screen[1]) <= self.port_radius
        })
    }

    /// Whether a wire from `from` may end on `to`.
    fn joins(&self, from: &str, to: &Port) -> bool {
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
    fn resizable_box(&self) -> Option<(String, [f64; 4])> {
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
    fn handle_at(&self, screen: [f64; 2]) -> Option<&'static str> {
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
    fn resized(&self, handle: &str, start: [f64; 4], d: [f64; 2], keep: bool) -> [f64; 4] {
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

    fn hover(&mut self, screen: [f64; 2]) {
        self.hovered_handle = self.handle_at(screen).unwrap_or("");
        let p = self.world(screen);
        self.pointer = Some(p);
        self.hovered_port = self.port_at(screen).map(|port| port.id.clone()).unwrap_or_default();
        self.hovered = self.item_at(p).unwrap_or_default();
    }

    fn select_only(&mut self, id: &str) {
        self.selection = vec![id.to_owned()];
    }

    fn toggle(&mut self, id: &str) {
        if let Some(i) = self.selection.iter().position(|s| s == id) {
            self.selection.remove(i);
        } else if self.multi_select {
            self.selection.push(id.to_owned());
        } else {
            self.select_only(id);
        }
    }

    fn finish_draft(&mut self, effects: &mut Effects) {
        let tool = self.tool;
        let minimum = if tool == Tool::Polygon { 3 } else { 2 };
        if self.draft.len() >= minimum {
            effects.raise("drawn", vec![tool.name().into(), flat(&self.draft)]);
        }
        self.draft.clear();
    }

    fn cancel(&mut self) -> bool {
        let busy = self.gesture != Gesture::None || !self.draft.is_empty();
        self.gesture = Gesture::None;
        self.draft.clear();
        self.move_delta = [0.0, 0.0];
        self.connect_to.clear();
        busy
    }

    fn press(&mut self, screen: [f64; 2], button: &str, modifiers: &str, effects: &mut Effects) {
        let p = self.world(screen);
        self.hover(screen);
        let additive = modifiers.contains("shift") || modifiers.contains("ctrl");
        if button == "middle" || (self.tool == Tool::Pan && button != "right") {
            self.gesture = Gesture::Pan { from: screen, origin: self.origin };
            return;
        }
        if button == "right" {
            let hit = self.item_at(p);
            if let Some(id) = &hit
                && !self.selection.contains(id)
            {
                self.select_only(id);
            }
            let id = hit.unwrap_or_default();
            effects.raise("context", vec![id.into(), p[0].into(), p[1].into()]);
            return;
        }
        let at = self.snapped_point(p);
        match self.tool {
            Tool::Select | Tool::Connect => {
                if self.tool == Tool::Select
                    && let Some(handle) = self.handle_at(screen)
                    && let Some((id, start)) = self.resizable_box()
                {
                    self.gesture = Gesture::Resize { id, handle, from: p, start, now: start };
                    return;
                }
                if let Some(port) = self.port_at(screen) {
                    let from = port.id.clone();
                    self.connect_at = port.at;
                    self.gesture = Gesture::Connect { from };
                    return;
                }
                if self.tool == Tool::Connect {
                    return;
                }
                match self.item_at(p) {
                    Some(id) => {
                        let selected = self.selection.contains(&id);
                        if additive {
                            self.toggle(&id);
                        } else if !selected {
                            self.select_only(&id);
                        }
                        if self.movable && self.selection.contains(&id) {
                            self.gesture = Gesture::Move { from: p, item: id, narrow: selected && !additive };
                        }
                    }
                    None => {
                        if !additive {
                            self.selection.clear();
                        }
                        self.gesture = Gesture::Band { from: p, to: p, additive };
                    }
                }
            }
            Tool::Pan => {}
            Tool::Point => effects.raise("drawn", vec!["point".into(), flat(&[at])]),
            Tool::Line | Tool::Rect | Tool::Ellipse => self.gesture = Gesture::Draw { from: at, to: at },
            Tool::Polyline | Tool::Polygon => {
                // Back on the first point closes a polygon.
                if self.tool == Tool::Polygon
                    && self.draft.len() >= 3
                    && let Some(first) = self.draft.first()
                {
                    let s = self.screen(*first);
                    // (On the grid, landing on the same point closes it too.)
                    if (s[0] - screen[0]).hypot(s[1] - screen[1]) <= self.port_radius || at == *first {
                        self.finish_draft(effects);
                        return;
                    }
                }
                self.draft.push(at);
            }
            Tool::Freehand => {
                self.draft = vec![p];
                self.gesture = Gesture::Freehand;
            }
            Tool::Brush => self.gesture = Gesture::Brush { from: p, to: p },
            Tool::Zoom => {
                self.gesture = Gesture::Zoom { from: p, to: p, out: modifiers.contains("shift") || modifiers.contains("alt") }
            }
        }
    }

    fn drag(&mut self, screen: [f64; 2], modifiers: &str, effects: &mut Effects) {
        let p = self.world(screen);
        self.pointer = Some(p);
        match self.gesture.clone() {
            Gesture::None => {}
            Gesture::Pan { from, origin } => {
                self.origin = origin;
                self.pan_by([from[0] - screen[0], from[1] - screen[1]]);
            }
            Gesture::Move { from, .. } => {
                let mut d = [self.snapped(p[0] - from[0]), self.snapped(p[1] - from[1])];
                // Shift holds a move to the axis it leans to.
                if modifiers.contains("shift") {
                    if d[0].abs() >= d[1].abs() { d[1] = 0.0 } else { d[0] = 0.0 }
                }
                if d != self.move_delta {
                    self.move_delta = d;
                    effects.raise("moving", vec![d[0].into(), d[1].into()]);
                }
            }
            Gesture::Band { from, additive, .. } => self.gesture = Gesture::Band { from, to: p, additive },
            Gesture::Draw { from, .. } => {
                let mut to = self.snapped_point(p);
                // Shift makes a rect a square and an ellipse a circle.
                if modifiers.contains("shift") && self.tool != Tool::Line {
                    let side = (to[0] - from[0]).abs().max((to[1] - from[1]).abs());
                    to = [from[0] + side.copysign(to[0] - from[0]), from[1] + side.copysign(to[1] - from[1])];
                }
                self.gesture = Gesture::Draw { from, to };
            }
            Gesture::Freehand => {
                // A point every two pixels or so: enough for a smooth stroke.
                let far = self.draft.last().is_none_or(|last| {
                    let s = self.screen(*last);
                    (s[0] - screen[0]).hypot(s[1] - screen[1]) >= 2.0
                });
                if far {
                    self.draft.push(p);
                }
            }
            Gesture::Connect { from } => {
                let target = self.port_at(screen).filter(|port| self.joins(&from, port)).map(|port| (port.id.clone(), port.at));
                match target {
                    Some((id, at)) => {
                        self.connect_to = id;
                        self.connect_at = at;
                    }
                    None => {
                        self.connect_to.clear();
                        self.connect_at = p;
                    }
                }
            }
            Gesture::Brush { from, .. } => self.gesture = Gesture::Brush { from, to: p },
            Gesture::Zoom { from, out, .. } => self.gesture = Gesture::Zoom { from, to: p, out },
            Gesture::Resize { id, handle, from, start, .. } => {
                let now = self.resized(handle, start, [p[0] - from[0], p[1] - from[1]], modifiers.contains("shift"));
                self.gesture = Gesture::Resize { id, handle, from, start, now };
            }
        }
    }

    fn release(&mut self, effects: &mut Effects) {
        match std::mem::replace(&mut self.gesture, Gesture::None) {
            Gesture::None | Gesture::Pan { .. } => {}
            Gesture::Move { item, narrow, .. } => {
                let d = self.move_delta;
                if d != [0.0, 0.0] {
                    effects.raise("moved", vec![ids(&self.selection), d[0].into(), d[1].into()]);
                } else if narrow {
                    self.select_only(&item);
                }
                self.move_delta = [0.0, 0.0];
            }
            Gesture::Band { from, to, additive } => {
                let b = spanned(from, to);
                let caught: Vec<String> = self
                    .items
                    .iter()
                    .filter(|item| {
                        let i = self.bbox(item);
                        item.selectable && i[0] <= b[2] && i[2] >= b[0] && i[1] <= b[3] && i[3] >= b[1]
                    })
                    .map(|item| item.id.clone())
                    .collect();
                // (A band too small to see was a click on nothing.)
                let s = [self.screen(from), self.screen(to)];
                if (s[0][0] - s[1][0]).hypot(s[0][1] - s[1][1]) >= 3.0 {
                    if !additive {
                        self.selection.clear();
                    }
                    for id in caught {
                        if !self.selection.contains(&id) && (self.multi_select || self.selection.is_empty()) {
                            self.selection.push(id);
                        }
                    }
                }
            }
            Gesture::Draw { from, to } => {
                if from != to {
                    effects.raise("drawn", vec![self.tool.name().into(), flat(&[from, to])]);
                }
            }
            Gesture::Freehand => {
                if self.draft.len() >= 2 {
                    effects.raise("drawn", vec!["freehand".into(), flat(&self.draft)]);
                }
                self.draft.clear();
            }
            Gesture::Connect { from } => {
                let to = std::mem::take(&mut self.connect_to);
                if to.is_empty() {
                    let at = self.connect_at;
                    effects.raise("connect_dropped", vec![from.into(), at[0].into(), at[1].into()]);
                } else {
                    // Out to in, whichever end the wire was pulled from.
                    let backwards = self.ports.iter().any(|p| p.id == from && p.kind == "in");
                    let (a, b) = if backwards { (to, from) } else { (from, to) };
                    effects.raise("connected", vec![a.into(), b.into()]);
                }
            }
            Gesture::Brush { from, to } => {
                let b = self.brushed(from, to);
                effects.raise("brushed", b.iter().map(|v| (*v).into()).collect());
            }
            Gesture::Resize { id, start, now, .. } => {
                if now != start {
                    effects.raise(
                        "resized",
                        vec![id.into(), now[0].into(), now[1].into(), now[2].into(), now[3].into()],
                    );
                }
            }
            Gesture::Zoom { from, to, out } => {
                let (a, b) = (self.screen(from), self.screen(to));
                if (a[0] - b[0]).abs() < 4.0 && (a[1] - b[1]).abs() < 4.0 {
                    let factor = if out { 1.0 / self.zoom_step } else { self.zoom_step };
                    self.zoom_about(factor * factor, a);
                } else {
                    self.fit_box(spanned(from, to));
                }
            }
        }
    }

    fn nudge(&mut self, direction: [f64; 2], shift: bool, effects: &mut Effects) {
        let times = if shift { 10.0 } else { 1.0 };
        if !self.selection.is_empty() && self.movable {
            let step = if self.grid > 0.0 { self.grid } else { 1.0 / self.zoom[0] };
            let d = [direction[0] * step * times, direction[1] * step * times];
            effects.raise("moved", vec![ids(&self.selection), d[0].into(), d[1].into()]);
        } else {
            let part = if shift { 0.5 } else { 0.1 };
            self.pan_by([direction[0] * self.size[0] * part, direction[1] * self.size[1] * part]);
        }
    }

    fn cycle(&mut self, back: bool) -> bool {
        let order: Vec<String> = self.items.iter().filter(|i| i.selectable).map(|i| i.id.clone()).collect();
        if order.is_empty() {
            return false;
        }
        let at = self.selection.last().and_then(|id| order.iter().position(|o| o == id));
        let next = match (at, back) {
            (None, false) => Some(0),
            (None, true) => Some(order.len() - 1),
            (Some(i), false) => (i + 1 < order.len()).then_some(i + 1),
            (Some(i), true) => i.checked_sub(1),
        };
        match next {
            Some(i) => {
                let id = order[i].clone();
                self.select_only(&id);
                self.reveal(&id);
                true
            }
            None => false,
        }
    }

    /// Pans so an item is in view.
    fn reveal(&mut self, id: &str) {
        let Some(item) = self.items.iter().find(|i| i.id == id) else { return };
        let b = self.bbox(item);
        let v = self.view_box();
        if b[0] >= v[0] && b[2] <= v[2] && b[1] >= v[1] && b[3] <= v[3] {
            return;
        }
        let centre = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        for axis in 0..2 {
            if self.zooms(axis) {
                self.origin[axis] = centre[axis] - self.size[axis] / 2.0 / self.zoom[axis];
            }
        }
        self.clamp_view();
    }

    fn key(&mut self, name: &str, modifiers: &str, effects: &mut Effects) -> bool {
        let ctrl = modifiers.contains("ctrl");
        let shift = modifiers.contains("shift");
        match name {
            "Left" => self.nudge([-1.0, 0.0], shift, effects),
            "Right" => self.nudge([1.0, 0.0], shift, effects),
            "Up" => self.nudge([0.0, -1.0], shift, effects),
            "Down" => self.nudge([0.0, 1.0], shift, effects),
            "plus" | "equal" | "KP_Add" => self.zoom_about(self.zoom_step, self.centre()),
            "minus" | "KP_Subtract" => self.zoom_about(1.0 / self.zoom_step, self.centre()),
            "0" | "KP_0" => self.zoom_to(1.0, self.centre()),
            "Home" => {
                if let Some(area) = self.everything() {
                    self.fit_box(area);
                }
            }
            "a" | "A" if ctrl => {
                self.selection = self.items.iter().filter(|i| i.selectable).map(|i| i.id.clone()).collect();
                if !self.multi_select {
                    self.selection.truncate(1);
                }
            }
            "BackSpace" if !self.draft.is_empty() => {
                self.draft.pop();
            }
            "Delete" | "BackSpace" if !self.selection.is_empty() => {
                effects.raise("deleted", vec![ids(&self.selection)]);
            }
            "Return" | "KP_Enter" if !self.draft.is_empty() => self.finish_draft(effects),
            "Return" | "KP_Enter" if self.selection.len() == 1 => {
                let id = self.selection[0].clone();
                let b = self.items.iter().find(|i| i.id == id).map(|i| self.bbox(i)).unwrap_or([0.0; 4]);
                effects.raise("activated", vec![id.into(), ((b[0] + b[2]) / 2.0).into(), ((b[1] + b[3]) / 2.0).into()]);
            }
            "Escape" => {
                if !self.cancel() {
                    if self.selection.is_empty() {
                        return false;
                    }
                    self.selection.clear();
                }
            }
            "Tab" | "ISO_Left_Tab" if !ctrl => return self.cycle(shift || name == "ISO_Left_Tab"),
            _ => return false,
        }
        true
    }
}

impl Archetype for Canvas {
    fn name(&self) -> &'static str {
        "Canvas"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let point = |i: usize| -> Option<[f64; 2]> { Some([number(arguments.get(i))?, number(arguments.get(i + 1))?]) };
        Ok(match event {
            "resize" => {
                let (w, h) = (number(arguments.first()).unwrap_or(0.0), number(arguments.get(1)).unwrap_or(0.0));
                self.changing(|s, _| {
                    s.size = [w.max(0.0), h.max(0.0)];
                    s.clamp_view();
                })
            }
            "pressed" => {
                let mut base = self.base.handle(event, arguments).unwrap_or_default();
                if !self.base.enabled {
                    return Ok(base);
                }
                let Some(at) = point(0) else { return Ok(base) };
                let button = text(arguments.get(4)).unwrap_or("left").to_owned();
                let modifiers = text(arguments.get(5)).unwrap_or("").to_owned();
                base.extend(self.changing(|s, e| s.press(at, &button, &modifiers, e)));
                base
            }
            "dragged" => {
                let Some(at) = point(0) else { return Ok(Effects::default()) };
                let modifiers = text(arguments.get(4)).unwrap_or("").to_owned();
                self.changing(|s, e| s.drag(at, &modifiers, e))
            }
            "released" => {
                let mut base = self.base.handle(event, arguments).unwrap_or_default();
                base.extend(self.changing(|s, e| s.release(e)));
                base
            }
            "canceled" | "cancel" => {
                let mut base = self.base.handle("canceled", arguments).unwrap_or_default();
                base.extend(self.changing(|s, _| {
                    s.cancel();
                }));
                base
            }
            "hover" => match point(0) {
                Some(at) => self.changing(|s, _| s.hover(at)),
                None => Effects::default(),
            },
            "exited" => {
                let mut base = self.base.handle(event, arguments).unwrap_or_default();
                base.extend(self.changing(|s, _| {
                    s.pointer = None;
                    s.hovered.clear();
                    s.hovered_port.clear();
                }));
                base
            }
            "double_clicked" => {
                let Some(at) = point(0) else { return Ok(Effects::default()) };
                self.changing(|s, e| {
                    if !s.draft.is_empty() && matches!(s.tool, Tool::Polyline | Tool::Polygon) {
                        // The double click's first press added a point twice.
                        if s.draft.len() >= 2 && s.draft[s.draft.len() - 1] == s.draft[s.draft.len() - 2] {
                            s.draft.pop();
                        }
                        s.finish_draft(e);
                        return;
                    }
                    let p = s.world(at);
                    let id = s.item_at(p).unwrap_or_default();
                    e.raise("activated", vec![id.into(), p[0].into(), p[1].into()]);
                })
            }
            "wheel" => {
                let steps = [number(arguments.first()).unwrap_or(0.0), number(arguments.get(1)).unwrap_or(0.0)];
                let pixels = [number(arguments.get(2)).unwrap_or(0.0), number(arguments.get(3)).unwrap_or(0.0)];
                let at = point(4).unwrap_or_else(|| self.centre());
                let modifiers = text(arguments.get(6)).unwrap_or("").to_owned();
                let mut effects = self.changing(|s, _| {
                    if s.wheel_zooms != modifiers.contains("ctrl") {
                        let notches = if steps[1] != 0.0 { steps[1] } else { pixels[1] / 40.0 };
                        s.zoom_about(s.zoom_step.powf(-notches), at);
                    } else {
                        let mut d = if pixels != [0.0, 0.0] { pixels } else { [steps[0] * 40.0, steps[1] * 40.0] };
                        // Shift turns a mouse wheel sideways.
                        if modifiers.contains("shift") && d[0] == 0.0 {
                            d = [d[1], 0.0];
                        }
                        s.pan_by(d);
                    }
                });
                effects.handled = true;
                effects
            }
            "pinch" => {
                let scale = number(arguments.first()).unwrap_or(1.0);
                let phase = text(arguments.get(1)).unwrap_or("update").to_owned();
                let at = point(2).unwrap_or_else(|| self.centre());
                self.changing(|s, _| {
                    let from = *s.pinch_from.get_or_insert(s.zoom);
                    let target = from[if s.axes == Axes::Y { 1 } else { 0 }] * scale;
                    s.zoom_to(target, at);
                    if phase == "end" {
                        s.pinch_from = None;
                    }
                })
            }
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("").to_owned();
                let modifiers = text(arguments.get(1)).unwrap_or("").to_owned();
                let mut used = false;
                let mut effects = self.changing(|s, e| used = s.key(&name, &modifiers, e));
                effects.handled = used;
                effects
            }
            "fit" => self.changing(|s, _| {
                if let Some(area) = s.everything() {
                    s.fit_box(area);
                }
            }),
            "zoom_by" => {
                let factor = number(arguments.first()).unwrap_or(1.0);
                let at = point(1).unwrap_or_else(|| self.centre());
                self.changing(|s, _| s.zoom_about(factor, at))
            }
            "set_view" => {
                let (x, y) = (number(arguments.first()), number(arguments.get(1)));
                let zoom = number(arguments.get(2));
                self.changing(|s, _| {
                    if let Some(z) = zoom {
                        s.zoom = [z.clamp(s.min_zoom, s.max_zoom); 2];
                    }
                    s.origin = [x.unwrap_or(s.origin[0]), y.unwrap_or(s.origin[1])];
                    s.clamp_view();
                })
            }
            "center_on" => {
                let at = point(0).unwrap_or([0.0, 0.0]);
                self.changing(|s, _| {
                    for axis in 0..2 {
                        s.origin[axis] = at[axis] - s.size[axis] / 2.0 / s.zoom[axis];
                    }
                    s.clamp_view();
                })
            }
            "clicked" | "key" | "long_pressed" | "drag_started" | "drag_finished" => Effects::default(),
            _ => self.base.handle(event, arguments).unwrap_or_default(),
        })
    }

    fn configure(&mut self, field: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field, value) {
            return result;
        }
        let n = || expect_number(Some(value), field);
        let on = || expect_boolean(Some(value), field);
        Ok(match field {
            "zoom" => {
                let z = n()?;
                self.changing(|s, _| s.zoom_to(z, s.centre()))
            }
            "view_x" => {
                let x = n()?;
                self.changing(|s, _| {
                    s.origin[0] = x;
                    s.clamp_view();
                })
            }
            "view_y" => {
                let y = n()?;
                self.changing(|s, _| {
                    s.origin[1] = y;
                    s.clamp_view();
                })
            }
            "min_zoom" => {
                self.min_zoom = n()?.max(1e-6);
                Effects::default()
            }
            "max_zoom" => {
                self.max_zoom = n()?.max(self.min_zoom);
                Effects::default()
            }
            "zoom_step" => {
                self.zoom_step = n()?.max(1.0001);
                Effects::default()
            }
            "axes" => {
                let axes = match text(Some(value)) {
                    Some("both") => Axes::Both,
                    Some("x") => Axes::X,
                    Some("y") => Axes::Y,
                    _ => return Err("axes is both, x or y".into()),
                };
                self.changing(|s, _| s.axes = axes)
            }
            "bounds" => {
                let b: Vec<f64> = entries(value).iter().filter_map(|v| number(Some(v))).collect();
                let bounds = (b.len() == 4).then(|| [b[0], b[1], b[2], b[3]]);
                self.changing(|s, _| {
                    s.bounds = bounds;
                    s.clamp_view();
                })
            }
            "grid" => {
                let g = n()?.max(0.0);
                self.changing(|s, _| s.grid = g)
            }
            "snap" => {
                self.snap = on()?;
                Effects::default()
            }
            "tool" => {
                let tool = text(Some(value)).and_then(Tool::parse).ok_or_else(|| {
                    "tool is select, pan, point, line, rect, ellipse, polyline, polygon, freehand, connect, brush or zoom"
                        .to_owned()
                })?;
                self.changing(|s, _| {
                    if s.tool != tool {
                        s.cancel();
                        s.tool = tool;
                    }
                })
            }
            "items" => {
                let items: Vec<Item> = entries(value).iter().filter_map(item_from).collect();
                self.changing(|s, _| {
                    s.items = items;
                    // What is gone is not selected or hovered.
                    let known = |id: &String| s.items.iter().any(|i| &i.id == id);
                    let kept: Vec<String> = s.selection.iter().filter(|id| known(id)).cloned().collect();
                    s.selection = kept;
                    if !known(&s.hovered) {
                        s.hovered.clear();
                    }
                })
            }
            "ports" => {
                self.ports = entries(value).iter().filter_map(port_from).collect();
                Effects::default()
            }
            "port_radius" => {
                self.port_radius = n()?.max(0.0);
                Effects::default()
            }
            "selection" => {
                let mut wanted = id_list(value);
                self.changing(|s, _| {
                    if !s.multi_select {
                        wanted.truncate(1);
                    }
                    s.selection = wanted;
                })
            }
            "multi_select" => {
                self.multi_select = on()?;
                Effects::default()
            }
            "movable" => {
                self.movable = on()?;
                Effects::default()
            }
            "wheel_zooms" => {
                self.wheel_zooms = on()?;
                Effects::default()
            }
            "fit_padding" => {
                self.fit_padding = n()?.max(0.0);
                Effects::default()
            }
            "resizable" => {
                self.resizable = on()?;
                Effects::default()
            }
            "min_item" => {
                self.min_item = n()?.max(0.0);
                Effects::default()
            }
            "hit_tolerance" => {
                self.tolerance = n()?.max(0.0);
                Effects::default()
            }
            _ => return Err(format!("Canvas has no setting `{field}`")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, IpcValue)]) -> IpcValue {
        IpcValue::Table(Arc::new(IpcTable::Map(entries.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect())))
    }

    fn canvas() -> Canvas {
        let mut c = Canvas::new();
        c.handle("resize", &[400.0.into(), 300.0.into()]).unwrap();
        let items = list(vec![
            map(&[("id", "a".into()), ("x", 10.0.into()), ("y", 10.0.into()), ("w", 50.0.into()), ("h", 30.0.into())]),
            map(&[("id", "b".into()), ("x", 200.0.into()), ("y", 100.0.into()), ("w", 40.0.into()), ("h", 40.0.into())]),
        ]);
        c.configure("items", &items).unwrap();
        c
    }

    fn signal<'a>(effects: &'a Effects, name: &str) -> Option<&'a Vec<IpcValue>> {
        effects.signals.iter().find(|(n, _)| n == name).map(|(_, a)| a)
    }

    fn press(c: &mut Canvas, x: f64, y: f64, button: &str, modifiers: &str) -> Effects {
        c.handle("pressed", &[x.into(), y.into(), 400.0.into(), 300.0.into(), button.into(), modifiers.into()])
            .unwrap()
    }

    fn drag(c: &mut Canvas, x: f64, y: f64) -> Effects {
        c.handle("dragged", &[x.into(), y.into(), 400.0.into(), 300.0.into(), "".into()]).unwrap()
    }

    #[test]
    fn the_selected_box_resizes_by_a_handle_on_the_grid() {
        let mut c = canvas();
        c.configure("resizable", &true.into()).unwrap();
        c.configure("grid", &10.0.into()).unwrap();
        c.configure("snap", &true.into()).unwrap();
        press(&mut c, 30.0, 30.0, "left", "");
        c.handle("released", &[]).unwrap();
        // The south-east corner of a (10, 10, 50, 30): at (60, 40).
        press(&mut c, 60.0, 40.0, "left", "");
        assert_eq!(c.gesture_name(), "resize");
        drag(&mut c, 87.0, 66.0);
        let e = c.handle("released", &[]).unwrap();
        let resized = signal(&e, "resized").unwrap();
        assert_eq!(resized[0], "a".into());
        assert_eq!((resized[3].clone(), resized[4].clone()), (80.0.into(), 60.0.into()));
        // Too small: held at the minimum.
        press(&mut c, 30.0, 30.0, "left", "");
        c.handle("released", &[]).unwrap();
        press(&mut c, 10.0, 10.0, "left", "");
        drag(&mut c, 200.0, 200.0);
        let e = c.handle("released", &[]).unwrap();
        let resized = signal(&e, "resized").unwrap();
        assert_eq!(resized[3], 8.0.into());
    }

    #[test]
    fn a_press_picks_and_a_drag_moves_on_the_grid() {
        let mut c = canvas();
        c.configure("grid", &10.0.into()).unwrap();
        c.configure("snap", &true.into()).unwrap();
        let e = press(&mut c, 20.0, 20.0, "left", "");
        assert_eq!(signal(&e, "selection_changed"), Some(&vec![ids(&["a".into()])]));
        drag(&mut c, 44.0, 27.0);
        let e = c.handle("released", &[]).unwrap();
        assert_eq!(signal(&e, "moved"), Some(&vec![ids(&["a".into()]), 20.0.into(), 10.0.into()]));
    }

    #[test]
    fn a_band_on_nothing_catches_what_it_crosses_and_shift_adds() {
        let mut c = canvas();
        press(&mut c, 150.0, 5.0, "left", "");
        drag(&mut c, 390.0, 290.0);
        c.handle("released", &[]).unwrap();
        assert_eq!(c.selection, vec!["b".to_owned()]);
        press(&mut c, 30.0, 20.0, "left", "shift");
        c.handle("released", &[]).unwrap();
        assert_eq!(c.selection, vec!["b".to_owned(), "a".to_owned()]);
        let e = c.handle("key", &["Delete".into(), "".into()]).unwrap();
        assert!(e.handled && signal(&e, "deleted").is_some());
    }

    #[test]
    fn ctrl_wheel_zooms_about_the_pointer() {
        let mut c = canvas();
        let before = c.world([100.0, 100.0]);
        let e = c
            .handle("wheel", &[0.into(), (-1).into(), 0.0.into(), 0.0.into(), 100.0.into(), 100.0.into(), "ctrl".into()])
            .unwrap();
        assert!(signal(&e, "view_changed").is_some());
        assert!((c.zoom[0] - 1.2).abs() < 1e-9);
        let after = c.world([100.0, 100.0]);
        assert!((before[0] - after[0]).abs() < 1e-9 && (before[1] - after[1]).abs() < 1e-9);
        // Plain, it pans.
        c.handle("wheel", &[0.into(), 1.into(), 0.0.into(), 0.0.into(), 0.0.into(), 0.0.into(), "".into()]).unwrap();
        assert!(c.origin[1] > after[1] - 100.0 / 1.2);
    }

    #[test]
    fn home_fits_everything_and_the_x_axis_zooms_alone() {
        let mut c = canvas();
        c.handle("key", &["Home".into(), "".into()]).unwrap();
        let v = c.view_box();
        assert!(v[0] <= 10.0 && v[2] >= 240.0 && v[1] <= 10.0 && v[3] >= 140.0);
        c.configure("axes", &"x".into()).unwrap();
        let zy = c.zoom[1];
        c.handle("key", &["plus".into(), "".into()]).unwrap();
        assert_eq!(c.zoom[1], zy);
    }

    #[test]
    fn a_wire_joins_out_to_in_and_dropped_says_where() {
        let mut c = canvas();
        let ports = list(vec![
            map(&[("id", "a.out".into()), ("item", "a".into()), ("x", 60.0.into()), ("y", 25.0.into()), ("kind", "out".into())]),
            map(&[("id", "b.in".into()), ("item", "b".into()), ("x", 200.0.into()), ("y", 120.0.into()), ("kind", "in".into())]),
        ]);
        c.configure("ports", &ports).unwrap();
        press(&mut c, 60.0, 25.0, "left", "");
        assert_eq!(c.gesture_name(), "connect");
        drag(&mut c, 201.0, 121.0);
        assert_eq!(c.connect_to, "b.in");
        let e = c.handle("released", &[]).unwrap();
        assert_eq!(signal(&e, "connected"), Some(&vec!["a.out".into(), "b.in".into()]));
        press(&mut c, 60.0, 25.0, "left", "");
        drag(&mut c, 300.0, 250.0);
        let e = c.handle("released", &[]).unwrap();
        assert!(signal(&e, "connect_dropped").is_some());
    }

    #[test]
    fn a_polygon_is_clicked_out_and_closed_on_its_first_point() {
        let mut c = canvas();
        c.configure("tool", &"polygon".into()).unwrap();
        for (x, y) in [(10.0, 10.0), (100.0, 10.0), (100.0, 100.0)] {
            press(&mut c, x, y, "left", "");
            c.handle("released", &[]).unwrap();
        }
        assert_eq!(c.gesture_name(), "draft");
        let e = press(&mut c, 12.0, 11.0, "left", "");
        let drawn = signal(&e, "drawn").unwrap();
        assert_eq!(drawn[0], "polygon".into());
        assert!(c.draft.is_empty());
    }

    #[test]
    fn escape_cancels_then_clears_then_goes_on() {
        let mut c = canvas();
        press(&mut c, 20.0, 20.0, "left", "");
        c.handle("released", &[]).unwrap();
        let e = c.handle("key", &["Escape".into(), "".into()]).unwrap();
        assert!(e.handled && c.selection.is_empty());
        let e = c.handle("key", &["Escape".into(), "".into()]).unwrap();
        assert!(!e.handled);
        // Tab walks the items and leaves after the last.
        assert!(c.handle("key", &["Tab".into(), "".into()]).unwrap().handled);
        assert!(c.handle("key", &["Tab".into(), "".into()]).unwrap().handled);
        assert!(!c.handle("key", &["Tab".into(), "".into()]).unwrap().handled);
    }
}
