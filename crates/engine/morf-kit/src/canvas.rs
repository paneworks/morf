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

mod archetype;
mod geometry;
mod gestures;
mod keys;
mod values;

use morf_value::IpcValue;

use crate::Effects;
use crate::control::ControlState;

use geometry::spanned;
use values::{Fields, entries, flat, id_list, ids, item_from, list, port_from};

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
    Pan {
        from: [f64; 2],
        origin: [f64; 2],
    },
    /// From the world point pressed; the item pressed, and whether the
    /// press should narrow the selection to it if nothing moves.
    Move {
        from: [f64; 2],
        item: String,
        narrow: bool,
    },
    Band {
        from: [f64; 2],
        to: [f64; 2],
        additive: bool,
    },
    /// A line, rect or ellipse: from one corner to the other.
    Draw {
        from: [f64; 2],
        to: [f64; 2],
    },
    Freehand,
    Connect {
        from: String,
    },
    Brush {
        from: [f64; 2],
        to: [f64; 2],
    },
    /// An item's box dragged by a handle: which, from where, the box then
    /// and the box it has reached.
    Resize {
        id: String,
        handle: &'static str,
        from: [f64; 2],
        start: [f64; 4],
        now: [f64; 4],
    },
    Zoom {
        from: [f64; 2],
        to: [f64; 2],
        out: bool,
    },
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
            Gesture::Band { from, to, .. } | Gesture::Zoom { from, to, .. } => {
                Some(spanned(*from, *to))
            }
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
        let band = self
            .band()
            .map(|b| list(b.iter().map(|v| (*v).into()).collect()))
            .unwrap_or(IpcValue::Nil);
        let from = match &self.gesture {
            Gesture::Connect { from } => from.clone(),
            _ => String::new(),
        };
        vec![
            ("view_x".into(), self.origin[0].into()),
            ("view_y".into(), self.origin[1].into()),
            (
                "zoom".into(),
                self.zoom[if self.axes == Axes::Y { 1 } else { 0 }].into(),
            ),
            ("zoom_x".into(), self.zoom[0].into()),
            ("zoom_y".into(), self.zoom[1].into()),
            ("viewport_width".into(), self.size[0].into()),
            ("viewport_height".into(), self.size[1].into()),
            ("tool".into(), self.tool.name().into()),
            ("selection".into(), ids(&self.selection)),
            (
                "selected_count".into(),
                (self.selection.len() as i64).into(),
            ),
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
        let (origin, zoom, selection, hovered) = (
            self.origin,
            self.zoom,
            self.selection.clone(),
            self.hovered.clone(),
        );
        let mut effects = Effects::default();
        change(self, &mut effects);
        let mut out = Effects::default();
        for (name, value) in self.fields() {
            if before.iter().find(|(n, _)| *n == name).map(|(_, v)| v) != Some(&value) {
                out.set(&name, value);
            }
        }
        if self.origin != origin || self.zoom != zoom {
            out.raise(
                "view_changed",
                vec![
                    self.origin[0].into(),
                    self.origin[1].into(),
                    self.zoom[0].into(),
                ],
            );
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

    fn hover(&mut self, screen: [f64; 2]) {
        self.hovered_handle = self.handle_at(screen).unwrap_or("");
        let p = self.world(screen);
        self.pointer = Some(p);
        self.hovered_port = self
            .port_at(screen)
            .map(|port| port.id.clone())
            .unwrap_or_default();
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
}

#[cfg(test)]
mod tests;
