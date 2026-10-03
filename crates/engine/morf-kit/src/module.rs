//! `morf.kit.native`: the archetypes as a Lua module.
//!
//! - `new(archetype, settings) -> id, state, effects`: a control's
//!   behaviour, and what making it did to others (its group).
//! - `send(id, event, ...) -> effects`: an event (`"pressed"`, `"key"`, ...).
//! - `configure(id, field, value) -> effects`: a setting written.
//! - `state(id) -> state`, `drop(id)`.
//! - `slots(archetype) -> { names }`, `archetypes() -> { names }`.
//! - `implicit_size(bw, bh, cw, ch, padding, insets) -> w, h`.
//! - `merge_tokens(parent, overrides) -> tokens`.
//!
//! `effects` is `{ state = { field = value }, signals = { { name, ... } },
//! handled = bool, others = { { id, effects } } }`: `handled` says whether
//! a key was used, and `others` what the event did to other controls.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use morf_lua::{HostFunction, Runtime};
use morf_value::{IpcTable, IpcValue};

use crate::collection::Collection;
use crate::disclosure::Disclosure;
use crate::drag::Drag;
use crate::navigation::Navigation;
use crate::control::{Control, implicit_size};
use crate::group::arrow_step;
use crate::plane::Plane;
use crate::popup::Popup;
use crate::press::Press;
use crate::range::Range;
use crate::scroll::Scroll;
use crate::selection::Selection;
use crate::slots::{ARCHETYPES, slots_of};
use crate::text_field::TextField;
use crate::tokens::merge_tokens;
use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Default)]
struct Registry {
    next: i64,
    controls: HashMap<i64, Box<dyn Archetype>>,
}

impl Registry {
    /// The members of `id`'s exclusive group, in the order they were made.
    fn members(&self, id: i64) -> Vec<i64> {
        let Some(group) = self
            .controls
            .get(&id)
            .and_then(|c| c.group())
            .filter(|g| g.exclusive)
        else {
            return Vec::new();
        };
        let mut members: Vec<i64> = self
            .controls
            .iter()
            .filter(|(_, c)| {
                c.group()
                    .is_some_and(|g| g.exclusive && g.name == group.name)
            })
            .map(|(id, _)| *id)
            .collect();
        members.sort_unstable();
        members
    }

    /// After `id` changed: if it became checked, the rest of its exclusive
    /// group is unchecked.
    fn settle_group(&mut self, id: i64, effects: &mut Effects) {
        let checked_now = effects
            .changed
            .iter()
            .any(|(f, v)| f == "checked" && *v == IpcValue::Boolean(true));
        if !checked_now {
            return;
        }
        for other in self.members(id).into_iter().filter(|other| *other != id) {
            if let Some(member) = self.controls.get_mut(&other).and_then(|c| c.as_member())
                && member.checked()
            {
                let change = member.set_checked_by_group(false);
                effects.others.push((other, change));
            }
        }
    }

    /// An arrow on a member of an exclusive group: the next enabled member
    /// along takes the check and the focus.
    fn arrow_in_group(&mut self, id: i64, name: &str, effects: &mut Effects) {
        let mirrored = self.controls.get(&id).is_some_and(|c| {
            c.state()
                .iter()
                .any(|(f, v)| f == "mirrored" && *v == IpcValue::Boolean(true))
        });
        let Some(step) = arrow_step(name, mirrored) else {
            return;
        };
        let members = self.members(id);
        let Some(at) = members.iter().position(|m| *m == id) else {
            return;
        };
        let count = members.len() as i64;
        for offset in 1..count {
            let next = members[(at as i64 + step * offset).rem_euclid(count) as usize];
            let Some(member) = self.controls.get_mut(&next).and_then(|c| c.as_member()) else {
                continue;
            };
            if !member.enabled() {
                continue;
            }
            let mut change = member.set_checked_by_group(true);
            change.raise("focus_request", Vec::new());
            change.raise("clicked", Vec::new());
            effects.others.push((next, change));
            if let Some(this) = self.controls.get_mut(&id).and_then(|c| c.as_member()) {
                effects.extend(this.set_checked_by_group(false));
            }
            effects.handled = true;
            return;
        }
    }
}

fn table(entries: Vec<(String, IpcValue)>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::Map(
        entries.into_iter().collect::<BTreeMap<_, _>>(),
    )))
}

fn list(values: Vec<IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::List(values)))
}

fn effects_value(effects: Effects) -> IpcValue {
    let others = effects
        .others
        .into_iter()
        .map(|(id, effects)| list(vec![IpcValue::Integer(id), effects_value(effects)]))
        .collect();
    let handled = effects.handled;
    let signals = effects
        .signals
        .into_iter()
        .map(|(name, mut arguments)| {
            arguments.insert(0, IpcValue::String(name));
            list(arguments)
        })
        .collect();
    table(vec![
        ("state".into(), table(effects.changed)),
        ("signals".into(), list(signals)),
        ("handled".into(), handled.into()),
        ("others".into(), list(others)),
    ])
}

/// Makes an archetype's behaviour by name.
fn make(archetype: &str) -> Result<Box<dyn Archetype>, String> {
    match archetype {
        "Control" => Ok(Box::new(Control::default())),
        "Press" => Ok(Box::new(Press::new())),
        "Range" => Ok(Box::new(Range::new())),
        "Plane" => Ok(Box::new(Plane::new())),
        "Popup" => Ok(Box::new(Popup::new())),
        "TextField" => Ok(Box::new(TextField::new())),
        "Scroll" => Ok(Box::new(Scroll::new())),
        "Collection" => Ok(Box::new(Collection::new())),
        "Disclosure" => Ok(Box::new(Disclosure::new())),
        "Drag" => Ok(Box::new(Drag::new())),
        "Navigation" => Ok(Box::new(Navigation::new())),
        "Selection" => Ok(Box::new(Selection::new())),
        "Shell" => Ok(Box::new(crate::shell::Shell::new())),
        "Canvas" => Ok(Box::new(crate::canvas::Canvas::new())),
        "Dock" => Ok(Box::new(crate::dock::Dock::new())),
        "Transform" => Ok(Box::new(crate::transform::Transform::new())),
        "Sheet" => Ok(Box::new(crate::sheet::Sheet::new())),
        "Roving" => Ok(Box::new(crate::roving::Roving::new())),
        "Form" => Ok(Box::new(crate::form::Form::new())),
        "Overflow" => Ok(Box::new(crate::overflow::Overflow::new())),
        other if ARCHETYPES.contains(&other) => {
            Err(format!("archetype {other} has not arrived yet"))
        }
        other => Err(format!("no archetype {other}")),
    }
}

fn sides(value: Option<&IpcValue>) -> [f64; 4] {
    match value {
        Some(IpcValue::Table(t)) => match t.as_ref() {
            IpcTable::List(items) if items.len() == 4 => {
                let n = |i: usize| number(items.get(i)).unwrap_or(0.0);
                [n(0), n(1), n(2), n(3)]
            }
            _ => [0.0; 4],
        },
        other => [number(other).unwrap_or(0.0); 4],
    }
}

/// Adds `morf.kit.native` to a runtime.
pub fn install(runtime: &mut Runtime) {
    let registry = Rc::new(RefCell::new(Registry::default()));
    let id_of = |arguments: &[IpcValue]| match arguments.first() {
        Some(IpcValue::Integer(id)) => Ok(*id),
        Some(IpcValue::Number(id)) => Ok(*id as i64),
        _ => Err("expected a control id".to_owned()),
    };
    let mut functions: Vec<(&'static str, HostFunction)> = Vec::new();
    let r = Rc::clone(&registry);
    functions.push((
        "new",
        Rc::new(move |arguments| {
            let name = text(arguments.first()).ok_or("kit.new wants an archetype name")?;
            let mut control = make(name)?;
            let mut effects = Effects::default();
            if let Some(IpcValue::Table(settings)) = arguments.get(1)
                && let IpcTable::Map(settings) = settings.as_ref()
            {
                // Bounds and modes before the values they bound.
                let rank = |field: &str| match field {
                    "from" | "to" | "range" | "orientation" | "logarithmic" | "wrap" | "step"
                    | "tristate" | "checkable" | "group" | "exclusive" | "allow_none" => 0,
                    "mode" | "axis" | "pages" | "extent" | "minimum" | "maximum" | "layout" | "columns_spec" | "tree_rows" | "count" | "labels" | "columns"
                    | "x_from" | "x_to" | "y_from" | "y_to" | "disabled" => 0,
                    "value" | "first" | "second" | "checked" | "partial" | "current"
                    | "selected" | "x" | "y" => 2,
                    _ => 1,
                };
                let mut ordered: Vec<_> = settings.iter().collect();
                ordered.sort_by_key(|(field, _)| rank(field));
                for (field, value) in ordered {
                    effects.extend(control.configure(field, value)?);
                }
            }
            let state = table(control.state());
            let mut registry = r.borrow_mut();
            registry.next += 1;
            let id = registry.next;
            registry.controls.insert(id, control);
            // Made checked into an exclusive group: the rest let go.
            registry.settle_group(id, &mut effects);
            Ok(vec![IpcValue::Integer(id), state, effects_value(effects)])
        }),
    ));
    let r = Rc::clone(&registry);
    functions.push((
        "send",
        Rc::new(move |arguments| {
            let id = id_of(&arguments)?;
            let event = text(arguments.get(1))
                .ok_or("kit.send wants an event name")?
                .to_owned();
            let mut registry = r.borrow_mut();
            let control = registry.controls.get_mut(&id).ok_or("no such control")?;
            let mut effects = control.handle(&event, &arguments[2..])?;
            if event == "key" && !effects.handled {
                let name = text(arguments.get(2)).unwrap_or("").to_owned();
                registry.arrow_in_group(id, &name, &mut effects);
            }
            registry.settle_group(id, &mut effects);
            Ok(vec![effects_value(effects)])
        }),
    ));
    let r = Rc::clone(&registry);
    functions.push((
        "configure",
        Rc::new(move |arguments| {
            let id = id_of(&arguments)?;
            let field = text(arguments.get(1))
                .ok_or("kit.configure wants a field name")?
                .to_owned();
            let value = arguments.get(2).cloned().unwrap_or(IpcValue::Nil);
            let mut registry = r.borrow_mut();
            let control = registry.controls.get_mut(&id).ok_or("no such control")?;
            let mut effects = control.configure(&field, &value)?;
            registry.settle_group(id, &mut effects);
            Ok(vec![effects_value(effects)])
        }),
    ));
    let r = Rc::clone(&registry);
    functions.push((
        "state",
        Rc::new(move |arguments| {
            let id = id_of(&arguments)?;
            let registry = r.borrow();
            let control = registry.controls.get(&id).ok_or("no such control")?;
            Ok(vec![table(control.state())])
        }),
    ));
    let r = Rc::clone(&registry);
    functions.push((
        "drop",
        Rc::new(move |arguments| {
            let id = id_of(&arguments)?;
            r.borrow_mut().controls.remove(&id);
            Ok(Vec::new())
        }),
    ));
    let r = Rc::clone(&registry);
    functions.push((
        "count",
        Rc::new(move |_| Ok(vec![IpcValue::Integer(r.borrow().controls.len() as i64)])),
    ));
    functions.push((
        "role",
        Rc::new(|arguments| {
            let archetype = text(arguments.first()).ok_or("kit.role wants an archetype name")?;
            let widget = text(arguments.get(1)).unwrap_or("");
            let checkable = matches!(arguments.get(2), Some(IpcValue::Boolean(true)));
            let item = crate::access::item_role(archetype, widget).map(IpcValue::from).unwrap_or(IpcValue::Nil);
            Ok(vec![crate::access::role_of(archetype, widget, checkable).into(), item])
        }),
    ));
    let r = Rc::clone(&registry);
    functions.push((
        "accessible",
        Rc::new(move |arguments| {
            // A control's accessible states, for the role it was given.
            let id = id_of(&arguments)?;
            let role = text(arguments.get(1)).unwrap_or("group").to_owned();
            let registry = r.borrow();
            let control = registry.controls.get(&id).ok_or("no such control")?;
            Ok(vec![table(crate::access::states_of(control.name(), &role, &control.state()))])
        }),
    ));
    functions.push((
        "slots",
        Rc::new(|arguments| {
            let name = text(arguments.first()).ok_or("kit.slots wants an archetype name")?;
            let slots = slots_of(name).ok_or_else(|| format!("no archetype {name}"))?;
            Ok(vec![list(
                slots.iter().map(|s| IpcValue::from(*s)).collect(),
            )])
        }),
    ));
    functions.push((
        "archetypes",
        Rc::new(|_| {
            Ok(vec![list(
                ARCHETYPES.iter().map(|s| IpcValue::from(*s)).collect(),
            )])
        }),
    ));
    functions.push((
        "implicit_size",
        Rc::new(|arguments| {
            let n = |i: usize, what: &str| expect_number(arguments.get(i), what);
            let (w, h) = implicit_size(
                (n(0, "background width")?, n(1, "background height")?),
                (n(2, "content width")?, n(3, "content height")?),
                sides(arguments.get(4)),
                sides(arguments.get(5)),
            );
            Ok(vec![w.into(), h.into()])
        }),
    ));
    functions.push((
        "merge_tokens",
        Rc::new(|arguments| {
            let parent = arguments.first().cloned().unwrap_or(IpcValue::Nil);
            let overrides = arguments.get(1).cloned().unwrap_or(IpcValue::Nil);
            Ok(vec![merge_tokens(&parent, &overrides)])
        }),
    ));
    runtime.add_native_module("morf.kit.native", functions);
}
