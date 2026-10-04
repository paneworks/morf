//! The keyboard: nudging, walking the items, and the keys a canvas answers.

use crate::Effects;

use super::{Canvas, ids};

impl Canvas {
    fn nudge(&mut self, direction: [f64; 2], shift: bool, effects: &mut Effects) {
        let times = if shift { 10.0 } else { 1.0 };
        if !self.selection.is_empty() && self.movable {
            let step = if self.grid > 0.0 {
                self.grid
            } else {
                1.0 / self.zoom[0]
            };
            let d = [direction[0] * step * times, direction[1] * step * times];
            effects.raise(
                "moved",
                vec![ids(&self.selection), d[0].into(), d[1].into()],
            );
        } else {
            let part = if shift { 0.5 } else { 0.1 };
            self.pan_by([
                direction[0] * self.size[0] * part,
                direction[1] * self.size[1] * part,
            ]);
        }
    }

    fn cycle(&mut self, back: bool) -> bool {
        let order: Vec<String> = self
            .items
            .iter()
            .filter(|i| i.selectable)
            .map(|i| i.id.clone())
            .collect();
        if order.is_empty() {
            return false;
        }
        let at = self
            .selection
            .last()
            .and_then(|id| order.iter().position(|o| o == id));
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
    #[allow(clippy::needless_range_loop)] // one index across three arrays
    fn reveal(&mut self, id: &str) {
        let Some(item) = self.items.iter().find(|i| i.id == id) else {
            return;
        };
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

    pub(super) fn key(&mut self, name: &str, modifiers: &str, effects: &mut Effects) -> bool {
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
                self.selection = self
                    .items
                    .iter()
                    .filter(|i| i.selectable)
                    .map(|i| i.id.clone())
                    .collect();
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
                let b = self
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .map(|i| self.bbox(i))
                    .unwrap_or([0.0; 4]);
                effects.raise(
                    "activated",
                    vec![
                        id.into(),
                        ((b[0] + b[2]) / 2.0).into(),
                        ((b[1] + b[3]) / 2.0).into(),
                    ],
                );
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
