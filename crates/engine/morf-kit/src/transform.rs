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

use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::control::ControlState;
use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

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

fn numbers(value: &IpcValue) -> Vec<f64> {
    match value {
        IpcValue::Table(t) => match t.as_ref() {
            IpcTable::List(items) => items.iter().filter_map(|v| number(Some(v))).collect(),
            IpcTable::Map(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

impl Archetype for Transform {
    fn name(&self) -> &'static str {
        "Transform"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let n = |i: usize| number(arguments.get(i)).unwrap_or(0.0);
        Ok(match event {
            "pressed" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                if !self.base.enabled || self.maximized {
                    return Ok(effects);
                }
                let handle = handle_name(text(arguments.get(2)).unwrap_or("body")).unwrap_or("body").to_owned();
                let allowed = match handle.as_str() {
                    "body" => self.movable,
                    "rotate" => self.rotatable,
                    _ => self.resizable,
                };
                if allowed {
                    let at = [n(0), n(1)];
                    let centre = match (number(arguments.get(4)), number(arguments.get(5))) {
                        (Some(x), Some(y)) => [x, y],
                        _ => [at[0], at[1] + self.rect.h / 2.0],
                    };
                    effects.extend(self.changing(|s, _| {
                        s.centre = centre;
                        s.gesture = Some((handle, at, s.rect, s.angle))
                    }));
                }
                effects
            }
            "dragged" => {
                let at = [n(0), n(1)];
                let modifiers = text(arguments.get(2)).unwrap_or("").to_owned();
                self.changing(|s, _| s.drag(at, &modifiers))
            }
            "released" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                let was = self.gesture.is_some();
                effects.extend(self.changing(|s, e| {
                    s.gesture = None;
                    if was {
                        e.raise("committed", s.box_args());
                    }
                }));
                effects
            }
            "canceled" => {
                let mut effects = self.base.handle(event, arguments).unwrap_or_default();
                effects.extend(self.changing(|s, _| {
                    if let Some((_, _, start, angle)) = s.gesture.take() {
                        s.rect = start;
                        s.angle = angle;
                    }
                }));
                effects
            }
            "container" => {
                let (w, h) = (n(0).max(0.0), n(1).max(0.0));
                self.changing(|s, _| {
                    s.container = [w, h];
                    if s.maximized {
                        s.rect = Rect { x: 0.0, y: 0.0, w: w.max(s.min[0]), h: h.max(s.min[1]) };
                    }
                })
            }
            "maximize" => self.changing(|s, e| {
                let on = !s.maximized;
                s.maximize(on, e)
            }),
            "minimize" => self.changing(|s, e| {
                let on = !s.minimized;
                s.minimize(on, e)
            }),
            "restore" => self.changing(|s, e| {
                s.maximize(false, e);
                s.minimize(false, e);
            }),
            "set" => {
                let r = Rect { x: n(0), y: n(1), w: n(2), h: n(3) };
                self.changing(|s, _| s.rect = s.fit(r))
            }
            "key" if self.base.enabled => {
                let name = text(arguments.first()).unwrap_or("").to_owned();
                let modifiers = text(arguments.get(1)).unwrap_or("").to_owned();
                let step = if modifiers.contains("shift") { 10.0 } else { 1.0 };
                let dir = match name.as_str() {
                    "Left" => Some([-1.0, 0.0]),
                    "Right" => Some([1.0, 0.0]),
                    "Up" => Some([0.0, -1.0]),
                    "Down" => Some([0.0, 1.0]),
                    _ => None,
                };
                let mirrored = self.base.mirrored;
                let mut used = true;
                let mut effects = self.changing(|s, e| {
                    match (dir, name.as_str()) {
                        (Some(mut d), _) => {
                            if mirrored {
                                d[0] = -d[0];
                            }
                            if modifiers.contains("alt") && s.rotatable {
                                s.angle = (s.angle + d[0] * if step > 1.0 { 15.0 } else { 1.0 }).rem_euclid(360.0);
                            } else if modifiers.contains("ctrl") && s.resizable {
                                let r = Rect { w: s.rect.w + d[0] * step, h: s.rect.h + d[1] * step, ..s.rect };
                                s.rect = s.fit(r);
                            } else if s.movable && !s.maximized {
                                let r = Rect { x: s.rect.x + d[0] * step, y: s.rect.y + d[1] * step, ..s.rect };
                                s.rect = s.fit(r);
                            } else {
                                used = false;
                            }
                            if used {
                                e.raise("committed", s.box_args());
                            }
                        }
                        (None, "Return") | (None, "KP_Enter") => {
                            let on = !s.maximized;
                            s.maximize(on, e);
                        }
                        (None, "Escape") if s.gesture.is_some() => {
                            if let Some((_, _, start, angle)) = s.gesture.take() {
                                s.rect = start;
                                s.angle = angle;
                            }
                        }
                        _ => used = false,
                    }
                });
                effects.handled = used;
                effects
            }
            "clicked" | "key" | "drag_started" | "drag_finished" | "long_pressed" | "double_clicked" => Effects::default(),
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
            "x" | "y" | "width" | "height" => {
                let v = n()?;
                self.changing(|s, _| {
                    match field {
                        "x" => s.rect.x = v,
                        "y" => s.rect.y = v,
                        "width" => s.rect.w = v,
                        _ => s.rect.h = v,
                    }
                    s.rect = s.fit(s.rect);
                })
            }
            "angle" => {
                let v = n()?;
                self.changing(|s, _| s.angle = v.rem_euclid(360.0))
            }
            "min_width" => {
                self.min[0] = n()?.max(0.0);
                Effects::default()
            }
            "min_height" => {
                self.min[1] = n()?.max(0.0);
                Effects::default()
            }
            "max_width" => {
                self.max[0] = n()?.max(0.0);
                Effects::default()
            }
            "max_height" => {
                self.max[1] = n()?.max(0.0);
                Effects::default()
            }
            "aspect" => {
                self.aspect = n()?.max(0.0);
                Effects::default()
            }
            "bounds" => {
                let b = numbers(value);
                let bounds = (b.len() == 4).then(|| [b[0], b[1], b[2], b[3]]);
                self.changing(|s, _| {
                    s.bounds = bounds;
                    s.rect = s.fit(s.rect);
                })
            }
            "snap" => {
                self.snap = n()?.max(0.0);
                Effects::default()
            }
            "movable" => {
                self.movable = on()?;
                Effects::default()
            }
            "resizable" => {
                self.resizable = on()?;
                Effects::default()
            }
            "rotatable" => {
                self.rotatable = on()?;
                Effects::default()
            }
            "handles" => {
                self.handles = text(Some(value)).unwrap_or("all").to_owned();
                Effects::default()
            }
            _ => return Err(format!("Transform has no setting `{field}`")),
        })
    }
}

#[allow(dead_code)]
fn list(values: Vec<IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::List(values)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed() -> Transform {
        let mut t = Transform::new();
        for (f, v) in [("x", 100.0), ("y", 100.0), ("width", 200.0), ("height", 100.0)] {
            t.configure(f, &v.into()).unwrap();
        }
        t
    }

    #[test]
    fn the_body_moves_and_a_corner_resizes_keeping_the_aspect_with_shift() {
        let mut t = boxed();
        t.handle("pressed", &[150.0.into(), 150.0.into(), "body".into(), "".into()]).unwrap();
        t.handle("dragged", &[170.0.into(), 160.0.into(), "".into()]).unwrap();
        let e = t.handle("released", &[]).unwrap();
        assert!(e.signals.iter().any(|(n, _)| n == "committed"));
        assert_eq!((t.rect.x, t.rect.y), (120.0, 110.0));
        t.handle("pressed", &[320.0.into(), 210.0.into(), "se".into(), "".into()]).unwrap();
        t.handle("dragged", &[420.0.into(), 220.0.into(), "shift".into()]).unwrap();
        assert_eq!((t.rect.w, t.rect.h), (300.0, 150.0));
        // A west edge keeps the east one where it was.
        t.handle("released", &[]).unwrap();
        let east = t.rect.x + t.rect.w;
        t.handle("pressed", &[120.0.into(), 180.0.into(), "w".into(), "".into()]).unwrap();
        t.handle("dragged", &[160.0.into(), 180.0.into(), "".into()]).unwrap();
        assert_eq!(t.rect.x + t.rect.w, east);
    }

    #[test]
    fn bounds_and_minimum_hold_and_maximize_round_trips() {
        let mut t = boxed();
        t.configure("bounds", &list(vec![0.0.into(), 0.0.into(), 400.0.into(), 300.0.into()])).unwrap();
        t.handle("pressed", &[150.0.into(), 150.0.into(), "body".into(), "".into()]).unwrap();
        t.handle("dragged", &[950.0.into(), 950.0.into(), "".into()]).unwrap();
        assert_eq!((t.rect.x, t.rect.y), (200.0, 200.0));
        t.handle("released", &[]).unwrap();
        t.handle("pressed", &[300.0.into(), 300.0.into(), "se".into(), "".into()]).unwrap();
        t.handle("dragged", &[0.0.into(), 0.0.into(), "".into()]).unwrap();
        assert_eq!((t.rect.w, t.rect.h), (16.0, 16.0));
        t.handle("released", &[]).unwrap();
        t.handle("container", &[800.0.into(), 600.0.into()]).unwrap();
        let before = t.rect;
        t.handle("key", &["Return".into(), "".into()]).unwrap();
        assert!(t.maximized && t.rect.w == 800.0);
        t.handle("restore", &[]).unwrap();
        assert_eq!(t.rect, before);
    }

    #[test]
    fn keys_move_resize_and_turn() {
        let mut t = boxed();
        t.configure("rotatable", &true.into()).unwrap();
        t.handle("key", &["Right".into(), "shift".into()]).unwrap();
        assert_eq!(t.rect.x, 110.0);
        t.handle("key", &["Down".into(), "ctrl".into()]).unwrap();
        assert_eq!(t.rect.h, 101.0);
        t.handle("key", &["Right".into(), "alt+shift".into()]).unwrap();
        assert_eq!(t.angle, 15.0);
    }
}
