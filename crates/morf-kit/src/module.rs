//! `morf.kit.native`: the archetypes as a Lua module.
//!
//! - `new(archetype, settings) -> id, state`: a control's behaviour.
//! - `send(id, event, ...) -> effects`: an event (`"pressed"`, `"key"`, ...).
//! - `configure(id, field, value) -> effects`: a setting written.
//! - `state(id) -> state`, `drop(id)`.
//! - `slots(archetype) -> { names }`, `archetypes() -> { names }`.
//! - `implicit_size(bw, bh, cw, ch, padding, insets) -> w, h`.
//! - `merge_tokens(parent, overrides) -> tokens`.
//!
//! `effects` is `{ state = { field = value }, signals = { { name, ... } } }`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use morf_lua::{HostFunction, IpcTable, IpcValue, Runtime};

use crate::control::{Control, implicit_size};
use crate::slots::{ARCHETYPES, slots_of};
use crate::tokens::merge_tokens;
use crate::value::{expect_number, number, text};
use crate::{Archetype, Effects};

#[derive(Default)]
struct Registry {
    next: i64,
    controls: HashMap<i64, Box<dyn Archetype>>,
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
    ])
}

/// Makes an archetype's behaviour by name.
fn make(archetype: &str) -> Result<Box<dyn Archetype>, String> {
    match archetype {
        "Control" => Ok(Box::new(Control::default())),
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
                for (field, value) in settings {
                    effects.extend(control.configure(field, value)?);
                }
            }
            let state = table(control.state());
            let mut registry = r.borrow_mut();
            registry.next += 1;
            let id = registry.next;
            registry.controls.insert(id, control);
            Ok(vec![IpcValue::Integer(id), state])
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
            Ok(vec![effects_value(
                control.handle(&event, &arguments[2..])?,
            )])
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
            Ok(vec![effects_value(control.configure(&field, &value)?)])
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
