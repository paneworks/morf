//! `Canvas` as an archetype: its state, its events, and its settings.

use morf_value::IpcValue;

use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

use super::{Axes, Canvas, Item, Tool, entries, id_list, item_from, port_from};

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
        let point = |i: usize| -> Option<[f64; 2]> {
            Some([number(arguments.get(i))?, number(arguments.get(i + 1))?])
        };
        Ok(match event {
            "resize" => {
                let (w, h) = (
                    number(arguments.first()).unwrap_or(0.0),
                    number(arguments.get(1)).unwrap_or(0.0),
                );
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
                let Some(at) = point(0) else {
                    return Ok(Effects::default());
                };
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
                let Some(at) = point(0) else {
                    return Ok(Effects::default());
                };
                self.changing(|s, e| {
                    if !s.draft.is_empty() && matches!(s.tool, Tool::Polyline | Tool::Polygon) {
                        // The double click's first press added a point twice.
                        if s.draft.len() >= 2
                            && s.draft[s.draft.len() - 1] == s.draft[s.draft.len() - 2]
                        {
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
                let steps = [
                    number(arguments.first()).unwrap_or(0.0),
                    number(arguments.get(1)).unwrap_or(0.0),
                ];
                let pixels = [
                    number(arguments.get(2)).unwrap_or(0.0),
                    number(arguments.get(3)).unwrap_or(0.0),
                ];
                let at = point(4).unwrap_or_else(|| self.centre());
                let modifiers = text(arguments.get(6)).unwrap_or("").to_owned();
                let mut effects = self.changing(|s, _| {
                    if s.wheel_zooms != modifiers.contains("ctrl") {
                        let notches = if steps[1] != 0.0 {
                            steps[1]
                        } else {
                            pixels[1] / 40.0
                        };
                        s.zoom_about(s.zoom_step.powf(-notches), at);
                    } else {
                        let mut d = if pixels != [0.0, 0.0] {
                            pixels
                        } else {
                            [steps[0] * 40.0, steps[1] * 40.0]
                        };
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
                    #[allow(clippy::needless_range_loop)] // one index across four arrays
                    for axis in 0..2 {
                        s.origin[axis] = at[axis] - s.size[axis] / 2.0 / s.zoom[axis];
                    }
                    s.clamp_view();
                })
            }
            "clicked" | "key" | "long_pressed" | "drag_started" | "drag_finished" => {
                Effects::default()
            }
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
                let b: Vec<f64> = entries(value)
                    .iter()
                    .filter_map(|v| number(Some(v)))
                    .collect();
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
                    let kept: Vec<String> =
                        s.selection.iter().filter(|id| known(id)).cloned().collect();
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
