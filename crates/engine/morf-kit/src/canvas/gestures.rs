//! The pointer's gestures: a press, the drag that follows it, and its
//! release.

use crate::Effects;

use super::{Canvas, Gesture, Tool, flat, ids, spanned};

impl Canvas {
    pub(super) fn press(&mut self, screen: [f64; 2], button: &str, modifiers: &str, effects: &mut Effects) {
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

    pub(super) fn drag(&mut self, screen: [f64; 2], modifiers: &str, effects: &mut Effects) {
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

    pub(super) fn release(&mut self, effects: &mut Effects) {
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
}
