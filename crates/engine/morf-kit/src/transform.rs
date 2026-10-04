//! `Transform`: a box the user moves, resizes from its edges and corners,
//! and turns -- a floating panel, an image cropper's frame, a canvas
//! item's handles, a picture-in-picture window, a scheduler's event.
//!
//! Settings: `x`, `y`, `width`, `height` (the box, in its parent's units),
//! `min_width`, `min_height` (16), `max_width`, `max_height`, `aspect`
//! (width over height to keep; 0 none -- Shift keeps the one it has),
//! `bounds` (`{ x0, y0, x1, y1 }`: the box stays inside), `snap` (a grid;
//! 0 none), `movable`, `resizable` (true), `rotatable` (false), `angle`
//! (degrees), `handles` (`"all"`, `"corners"`, `"edges"`, `"none"`).
//!
//! State: `x`, `y`, `box_width`, `box_height`, `angle`, `active` (`"none"`,
//! `"move"`, `"resize"`, `"rotate"`), `handle` (the one held: `"n"`,
//! `"ne"`, `"e"`, `"se"`, `"s"`, `"sw"`, `"w"`, `"nw"`, `"body"`,
//! `"rotate"`, or ""), `maximized`, `minimized`.
//!
//! Events: the base's; `"pressed"` (surface x, y, handle, modifiers, and
//! the box's centre on the surface, centre x, centre y, for a turn);
//! `"dragged"` (surface x, y, modifiers: Shift keeps the aspect, Alt
//! resizes about the centre, and turning in 15-degree steps);
//! `"released"`; `"key"` (the arrows move by a pixel, Shift ten; with
//! Ctrl they resize; with Alt they turn; Return toggles maximized, Escape
//! puts back a drag under way); `"container"` (width, height: what
//! maximized fills); `"maximize"`, `"minimize"`, `"restore"`; `"set"` (x,
//! y, width, height). Signals: `changed` (x, y, width, height, angle) as it
//! moves, `committed` (the same) once let go or keyed, `maximized`
//! (bool), `minimized` (bool).

mod archetype;

use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::Effects;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

pub(crate) struct Transform {
    pub(crate) base: ControlState,
    rect: Rect,
    angle: f64,
    min: [f64; 2],
    max: [f64; 2],
    aspect: f64,
    bounds: Option<[f64; 4]>,
    snap: f64,
    movable: bool,
    resizable: bool,
    rotatable: bool,
    handles: String,
    /// A gesture under way: what is held, where the press was, the box and
    /// the angle then.
    gesture: Option<(String, [f64; 2], Rect, f64)>,
    container: [f64; 2],
    /// Where a turn turns about, on the surface.
    centre: [f64; 2],
    /// The box to go back to from maximized or minimized.
    saved: Option<Rect>,
    maximized: bool,
    minimized: bool,
}

fn handle_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "n" => "n",
        "ne" => "ne",
        "e" => "e",
        "se" => "se",
        "s" => "s",
        "sw" => "sw",
        "w" => "w",
        "nw" => "nw",
        "body" | "" => "body",
        "rotate" => "rotate",
        _ => return None,
    })
}

impl Transform {
    pub(crate) fn new() -> Self {
        Self {
            base: ControlState::default(),
            rect: Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 },
            angle: 0.0,
            min: [16.0, 16.0],
            max: [f64::INFINITY, f64::INFINITY],
            aspect: 0.0,
            bounds: None,
            snap: 0.0,
            movable: true,
            resizable: true,
            rotatable: false,
            handles: "all".into(),
            gesture: None,
            container: [0.0, 0.0],
            centre: [0.0, 0.0],
            saved: None,
            maximized: false,
            minimized: false,
        }
    }

    fn fields(&self) -> Vec<(String, IpcValue)> {
        let (active, handle) = match &self.gesture {
            None => ("none", String::new()),
            Some((h, ..)) if h == "body" => ("move", h.clone()),
            Some((h, ..)) if h == "rotate" => ("rotate", h.clone()),
            Some((h, ..)) => ("resize", h.clone()),
        };
        vec![
            ("x".into(), self.rect.x.into()),
            ("y".into(), self.rect.y.into()),
            // (Not `width`/`height`: those are the node's laid-out size in
            // the live state a skin reads.)
            ("box_width".into(), self.rect.w.into()),
            ("box_height".into(), self.rect.h.into()),
            ("angle".into(), self.angle.into()),
            ("active".into(), active.into()),
            ("handle".into(), handle.into()),
            ("maximized".into(), self.maximized.into()),
            ("minimized".into(), self.minimized.into()),
        ]
    }

    fn changing(&mut self, change: impl FnOnce(&mut Self, &mut Effects)) -> Effects {
        let before = self.fields();
        let (rect, angle) = (self.rect, self.angle);
        let mut inner = Effects::default();
        change(self, &mut inner);
        let mut out = Effects::default();
        for (name, value) in self.fields() {
            if before.iter().find(|(n, _)| *n == name).map(|(_, v)| v) != Some(&value) {
                out.set(&name, value);
            }
        }
        if self.rect != rect || self.angle != angle {
            out.raise("changed", self.box_args());
        }
        out.signals.extend(inner.signals);
        out.handled = inner.handled;
        out
    }

    fn box_args(&self) -> Vec<IpcValue> {
        vec![self.rect.x.into(), self.rect.y.into(), self.rect.w.into(), self.rect.h.into(), self.angle.into()]
    }

    fn snapped(&self, v: f64) -> f64 {
        if self.snap > 0.0 { (v / self.snap).round() * self.snap } else { v }
    }

    /// The box kept to its limits and inside its bounds.
    fn fit(&self, mut r: Rect) -> Rect {
        r.w = r.w.clamp(self.min[0], self.max[0].max(self.min[0]));
        r.h = r.h.clamp(self.min[1], self.max[1].max(self.min[1]));
        if let Some(b) = self.bounds {
            r.w = r.w.min((b[2] - b[0]).max(self.min[0]));
            r.h = r.h.min((b[3] - b[1]).max(self.min[1]));
            r.x = r.x.clamp(b[0], (b[2] - r.w).max(b[0]));
            r.y = r.y.clamp(b[1], (b[3] - r.h).max(b[1]));
        }
        r
    }

    fn resize(&self, handle: &str, start: Rect, d: [f64; 2], keep: f64, centred: bool) -> Rect {
        let (west, east) = (handle.contains('w'), handle.contains('e'));
        let (north, south) = (handle.starts_with('n'), handle.starts_with('s'));
        let k = if centred { 2.0 } else { 1.0 };
        let mut w = start.w + if east { d[0] * k } else if west { -d[0] * k } else { 0.0 };
        let mut h = start.h + if south { d[1] * k } else if north { -d[1] * k } else { 0.0 };
        w = self.snapped(w).clamp(self.min[0], self.max[0].max(self.min[0]));
        h = self.snapped(h).clamp(self.min[1], self.max[1].max(self.min[1]));
        if keep > 0.0 {
            // An edge drives the other side; a corner, whichever moved more.
            let by_width = if (east || west) && !(north || south) {
                true
            } else if (north || south) && !(east || west) {
                false
            } else {
                (w / start.w.max(1e-9)) >= (h / start.h.max(1e-9))
            };
            if by_width { h = w / keep } else { w = h * keep }
        }
        let (mut x, mut y) = (start.x, start.y);
        if centred {
            x = start.x + (start.w - w) / 2.0;
            y = start.y + (start.h - h) / 2.0;
        } else {
            if west {
                x = start.x + start.w - w;
            } else if !east {
                x = start.x + (start.w - w) / 2.0;
            }
            if north {
                y = start.y + start.h - h;
            } else if !south {
                y = start.y + (start.h - h) / 2.0;
            }
        }
        self.fit(Rect { x, y, w, h })
    }

    fn drag(&mut self, at: [f64; 2], modifiers: &str) {
        let Some((handle, from, start, start_angle)) = self.gesture.clone() else { return };
        let d = [at[0] - from[0], at[1] - from[1]];
        let shift = modifiers.contains("shift");
        match handle.as_str() {
            "body" => {
                if self.movable {
                    let mut r = start;
                    r.x = self.snapped(start.x + d[0]);
                    r.y = self.snapped(start.y + d[1]);
                    self.rect = self.fit(r);
                }
            }
            "rotate" => {
                let centre = self.centre;
                let a0 = (from[0] - centre[0]).atan2(centre[1] - from[1]).to_degrees();
                let a1 = (at[0] - centre[0]).atan2(centre[1] - at[1]).to_degrees();
                let mut angle = start_angle + a1 - a0;
                if shift {
                    angle = (angle / 15.0).round() * 15.0;
                }
                self.angle = angle.rem_euclid(360.0);
            }
            h => {
                if self.resizable {
                    let keep = if self.aspect > 0.0 {
                        self.aspect
                    } else if shift {
                        start.w / start.h.max(1e-9)
                    } else {
                        0.0
                    };
                    self.rect = self.resize(h, start, d, keep, modifiers.contains("alt"));
                }
            }
        }
    }

    fn maximize(&mut self, on: bool, effects: &mut Effects) {
        if on == self.maximized {
            return;
        }
        if on {
            if !self.minimized {
                self.saved = Some(self.rect);
            }
            self.minimized = false;
            self.rect = Rect { x: 0.0, y: 0.0, w: self.container[0].max(self.min[0]), h: self.container[1].max(self.min[1]) };
        } else if let Some(saved) = self.saved.take() {
            self.rect = saved;
        }
        self.maximized = on;
        effects.raise("maximized", vec![on.into()]);
    }

    fn minimize(&mut self, on: bool, effects: &mut Effects) {
        if on == self.minimized {
            return;
        }
        if on {
            if !self.maximized {
                self.saved = Some(self.rect);
            }
            self.maximized = false;
        } else if let Some(saved) = self.saved.take() {
            self.rect = saved;
        }
        self.minimized = on;
        effects.raise("minimized", vec![on.into()]);
    }
}

#[allow(dead_code)]
fn list(values: Vec<IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::List(values)))
}

#[cfg(test)]
mod tests;
