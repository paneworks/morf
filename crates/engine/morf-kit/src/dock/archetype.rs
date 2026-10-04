//! `Dock` as an archetype: its state, its events, and its settings.

use morf_value::IpcValue;

use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

use super::{Dock, Floating, Node, Zone, entries, field, words};

impl Archetype for Dock {
    fn name(&self) -> &'static str {
        "Dock"
    }

    fn state(&self) -> Vec<(String, IpcValue)> {
        let mut fields = self.base.fields();
        fields.extend(self.fields());
        fields
    }

    fn handle(&mut self, event: &str, arguments: &[IpcValue]) -> Result<Effects, String> {
        let word = |i: usize| text(arguments.get(i)).unwrap_or("").to_owned();
        let n = |i: usize| number(arguments.get(i)).unwrap_or(0.0);
        Ok(match event {
            "activate" => {
                let panel = word(0);
                self.changing(|s, e| s.activate(&panel, e))
            }
            "close" => {
                let panel = word(0);
                self.changing(|s, e| s.close(&panel, e))
            }
            "drag_start" => {
                let panel = word(0);
                self.changing(|s, _| {
                    s.dragging = panel;
                    s.drop = None;
                })
            }
            "drag_over" => {
                let stack = word(0);
                let (x, y, w, h) = (n(1), n(2), n(3), n(4));
                self.changing(|s, _| {
                    if !s.dragging.is_empty() {
                        let zone = s.mirror(s.zone_at(x, y, w, h));
                        s.drop = Some((stack, zone));
                    }
                })
            }
            "drag_outside" => self.changing(|s, _| {
                if !s.dragging.is_empty() {
                    s.drop = Some(("float".into(), Zone::Center));
                }
            }),
            "drop" => {
                let rect = [n(0), n(1), n(2).max(120.0), n(3).max(80.0)];
                self.changing(|s, e| {
                    let panel = std::mem::take(&mut s.dragging);
                    match s.drop.take() {
                        Some((target, _)) if target == "float" => {
                            s.take(&panel);
                            s.floating.push(Floating { panel: panel.clone(), rect });
                        }
                        Some((target, zone)) if !panel.is_empty() => s.dock(&panel, &target, zone, e),
                        _ => {}
                    }
                })
            }
            "drag_cancel" => self.changing(|s, _| {
                s.dragging.clear();
                s.drop = None;
            }),
            "resize" => {
                let (split, index, position) = (word(0), n(1).max(0.0) as usize, n(2).clamp(0.0, 1.0));
                let min = self.min_ratio;
                self.changing(|s, _| {
                    if let Some(Node::Split { ratios, .. }) = s.root.as_mut().and_then(|r| r.find_mut(&split))
                        && index + 1 < ratios.len()
                    {
                        let start: f64 = ratios[..index].iter().sum();
                        let pair = ratios[index] + ratios[index + 1];
                        let first = (position - start).clamp(min.min(pair / 2.0), (pair - min).max(pair / 2.0));
                        ratios[index] = first;
                        ratios[index + 1] = pair - first;
                    }
                })
            }
            "maximize" => {
                let panel = word(0);
                self.changing(|s, e| {
                    if s.maximized == panel {
                        s.maximized.clear();
                    } else {
                        s.activate(&panel, e);
                        s.maximized = panel;
                    }
                })
            }
            "float" => {
                let panel = word(0);
                let rect = [n(1), n(2), n(3).max(120.0), n(4).max(80.0)];
                self.changing(|s, _| {
                    if s.take(&panel) {
                        s.floating.push(Floating { panel, rect });
                    }
                })
            }
            "move_floating" => {
                let panel = word(0);
                let rect = [n(1), n(2), n(3).max(120.0), n(4).max(80.0)];
                self.changing(|s, _| {
                    if let Some(f) = s.floating.iter_mut().find(|f| f.panel == panel) {
                        f.rect = rect;
                    }
                })
            }
            "dock" => {
                let (panel, stack) = (word(0), word(1));
                let zone = Zone::parse(&word(2)).unwrap_or(Zone::Center);
                self.changing(|s, e| s.dock(&panel, &stack, zone, e))
            }
            "focus_stack" => {
                let stack = word(0);
                self.changing(|s, _| s.focused = stack)
            }
            "key" if self.base.enabled => {
                let (name, modifiers) = (word(0), word(1));
                let ctrl = modifiers.contains("ctrl");
                let shift = modifiers.contains("shift");
                let mut used = true;
                let mut effects = self.changing(|s, e| {
                    used = match name.as_str() {
                        "Page_Down" if ctrl => s.walk_tabs(false),
                        "Page_Up" if ctrl => s.walk_tabs(true),
                        "w" | "W" if ctrl => {
                            let panel = s.current_of(&s.focused);
                            s.close(&panel, e);
                            !panel.is_empty()
                        }
                        "m" | "M" if ctrl && shift => {
                            let panel = s.current_of(&s.focused);
                            if s.maximized == panel {
                                s.maximized.clear();
                            } else {
                                s.maximized = panel;
                            }
                            true
                        }
                        "F6" => s.cycle_stacks(shift),
                        "Escape" if !s.dragging.is_empty() => {
                            s.dragging.clear();
                            s.drop = None;
                            true
                        }
                        "Escape" if !s.maximized.is_empty() => {
                            s.maximized.clear();
                            true
                        }
                        _ => false,
                    };
                });
                effects.handled = used;
                effects
            }
            "clicked" | "key" => Effects::default(),
            _ => self.base.handle(event, arguments).unwrap_or_default(),
        })
    }

    fn configure(&mut self, field_name: &str, value: &IpcValue) -> Result<Effects, String> {
        if let Some(result) = self.base.configure(field_name, value) {
            return result;
        }
        Ok(match field_name {
            "layout" => {
                let root = self.parse(value);
                self.changing(|s, _| s.root = root.and_then(Node::tidy))
            }
            "floating" => {
                let floating: Vec<Floating> = entries(Some(value))
                    .iter()
                    .filter_map(|f| {
                        let g = |k: &str| number(field(f, k)).unwrap_or(0.0);
                        Some(Floating {
                            panel: text(field(f, "panel"))?.to_owned(),
                            rect: [g("x"), g("y"), g("w").max(120.0), g("h").max(80.0)],
                        })
                    })
                    .collect();
                self.changing(|s, _| s.floating = floating)
            }
            "fixed" => {
                self.fixed = words(Some(value));
                Effects::default()
            }
            "edge" => {
                self.edge = expect_number(Some(value), field_name)?.clamp(0.05, 0.45);
                Effects::default()
            }
            "min_ratio" => {
                self.min_ratio = expect_number(Some(value), field_name)?.clamp(0.0, 0.45);
                Effects::default()
            }
            _ => return Err(format!("Dock has no setting `{field_name}`")),
        })
    }
}
