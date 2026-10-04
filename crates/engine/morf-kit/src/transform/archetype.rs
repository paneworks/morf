//! `Transform` as an archetype: its state, its events, and its settings.

use morf_value::{IpcTable, IpcValue};

use crate::value::{expect_boolean, expect_number, number, text};
use crate::{Archetype, Effects};

use super::{Rect, Transform, handle_name};

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
