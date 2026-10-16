//! `ui.claims(node, { ... })`: who owns each of a node's properties.
//!
//! ```lua
//! local claims = ui.claims(panel, {
//!   motion = { duration = 200, easing = "out_cubic" },
//!   open = { when = function() return opened:get() end, motion = ui.spring { stiffness = 260 },
//!            set = { x = 0, opacity = 1 } },
//!   peek = { class = "asked", set = { x = -40 } },
//!   rest = { set = { x = -288, opacity = 0 } },
//! })
//! claims.open.presence:get()   -- 0..1, a read-only signal
//! claims.peek.ask()            -- an asked claim, raised by hand
//! claims.peek.release()
//! claims.peek.remove()
//! ```
//!
//! Each named entry claims the properties in its `set` while it is
//! active. `class` is one of state, asked, reflex, posture, ambient
//! (state by default with a `when`, ambient without). `when` is a
//! function read like a binding, a signal, or a boolean; with none the
//! claim always holds, except an asked one, which holds while asked. A
//! target is a value, a function or a signal. `motion` is a behaviour
//! table, for the whole set or one claim; without one a property moves
//! by its own `behavior`.

use luna::{
    Callback, CallbackReturn, Closure, Context, Executor, Table, UserData, UserRef,
    Value as LuaValue,
};
use std::rc::Rc;

use morf_runtime::animation::claims::{ClaimId, ClaimSpec, Priority};
use morf_scene::{Motion, NodeHandle};

use crate::api::ui::bindings::{flush_reactive, lua_to_scene};
use crate::api::ui::configure::parse_motion;
use crate::api::ui::scene::{HostError, refuse_runtime_owned};
use crate::runtime::host_types::IpcValue;
use crate::runtime::state::*;
use crate::value::tokens::{NodeToken, SignalToken};
use crate::vm::execute::drive_executor;
use crate::vm::types::Limits;

/// A condition or target given as a function or a signal: read by a
/// binding, so the claim follows it.
type Dynamic<'gc> = LuaValue<'gc>;

struct Declared<'gc> {
    spec: ClaimSpec,
    when: Option<Dynamic<'gc>>,
    targets: Vec<(String, Dynamic<'gc>)>,
}

pub(crate) fn install_claims_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    ui: Table<'gc>,
) {
    let claims = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, spec): (UserRef<NodeToken>, Table) = stack.consume(ctx)?;
        let node = node.handle;
        let handles = register(ctx, &state, node, spec).map_err(HostError)?;
        stack.replace(ctx, handles);
        Ok(CallbackReturn::Return)
    });
    ui.set_field(ctx, "claims", claims);
}

fn register<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    node: NodeHandle,
    spec: Table<'gc>,
) -> Result<Table<'gc>, String> {
    let shared = match spec.get_value(ctx, "motion") {
        LuaValue::Nil => None,
        LuaValue::Table(motion) => {
            Some(parse_motion(ctx, motion).map_err(|e| format!("motion: {e}"))?)
        }
        _ => return Err("ui.claims `motion` must be a behaviour table".to_owned()),
    };
    let mut declared = Vec::new();
    for (key, value) in spec.iter(ctx) {
        let LuaValue::String(name) = key else {
            return Err("ui.claims keys must be claim names".to_owned());
        };
        let name = name.display_lossy().to_string();
        if name == "motion" {
            continue;
        }
        let LuaValue::Table(entry) = value else {
            return Err(format!("claim `{name}` must be a table"));
        };
        declared.push(declare(ctx, name, entry, shared)?);
    }
    if declared.is_empty() {
        return Err("ui.claims needs at least one claim".to_owned());
    }
    {
        let state = state.borrow();
        for claim in &declared {
            for (property, _) in &claim.spec.targets {
                refuse_other_owner(&state, node, property)
                    .map_err(|error| format!("claim `{}`: {error}", claim.spec.name))?;
            }
        }
    }
    // Everything that can fail is done before the claims are registered.
    let limits = *state.borrow().limits();
    let reader = declared
        .iter()
        .any(|claim| claim.when.is_some() || !claim.targets.is_empty())
        .then(|| reader_factory(ctx))
        .transpose()?;
    let mut inputs = Vec::with_capacity(declared.len());
    for claim in &declared {
        let mut readers = Vec::new();
        if let (Some(when), Some(reader)) = (claim.when, reader) {
            let closure = read_closure(ctx, reader, when, true, limits)?;
            readers.push((ClaimInput::Condition, closure));
        }
        for (property, target) in &claim.targets {
            let reader = reader.ok_or("no reader for a bound target")?;
            let closure = read_closure(ctx, reader, *target, false, limits)?;
            readers.push((ClaimInput::Target(property.clone()), closure));
        }
        inputs.push(readers);
    }
    let metatable = state
        .borrow()
        .metatables()
        .read_only_signal()
        .ok_or("signals are not installed")?;
    let mut presences = Vec::with_capacity(declared.len());
    for claim in &declared {
        let name = format!("{node:?}.claims.{}.presence", claim.spec.name);
        let made = state
            .borrow_mut()
            .reactive_mut()
            .signal(name, IpcValue::Number(0.0));
        presences.extend(made);
    }
    let specs = declared.iter().map(|claim| claim.spec.clone()).collect();
    let registered = if presences.len() == declared.len() {
        state.borrow_mut().register_claims(node, specs)
    } else {
        Err("ui.claims cannot run while the reactive graph is away".to_owned())
    };
    let ids = match registered {
        Ok(ids) => ids,
        Err(error) => {
            let made = presences.into_iter().collect();
            state.borrow_mut().reactive_mut().forget_signals(made);
            return Err(error);
        }
    };
    let handles = Table::new(&ctx);
    let claims = ids.into_iter().zip(&declared).zip(inputs).zip(presences);
    for (((id, claim), readers), presence) in claims {
        let effects = readers
            .into_iter()
            .map(|(input, closure)| add_input(ctx, state, node, id, input, closure))
            .collect();
        state
            .borrow_mut()
            .animation_mut()
            .claims
            .attach(id, Some(presence), effects);
        let handle = claim_handle(ctx, state, id, claim.spec.class, presence, &metatable);
        let _ = handles.set(ctx, ctx.intern(claim.spec.name.as_bytes()), handle);
    }
    // The inputs' first runs, then the winners' first values at once.
    let _ = flush_reactive(state, ctx, limits);
    run_claims(state, ctx, limits, ClaimsPass::Settle(node));
    Ok(handles)
}

/// One entry of the table, checked for its shape.
fn declare<'gc>(
    ctx: Context<'gc>,
    name: String,
    entry: Table<'gc>,
    shared: Option<Motion>,
) -> Result<Declared<'gc>, String> {
    let fail = |message: &str| format!("claim `{name}`: {message}");
    for (key, _) in entry.iter(ctx) {
        let known = matches!(&key, LuaValue::String(key)
            if matches!(key.as_bytes(), b"class" | b"when" | b"set" | b"motion"));
        if !known {
            return Err(fail(&format!(
                "unknown field `{}` (class, when, set, motion)",
                key.display()
            )));
        }
    }
    let when_value = entry.get_value(ctx, "when");
    let class = match entry.get_value(ctx, "class") {
        LuaValue::Nil if when_value.is_nil() => Priority::Ambient,
        LuaValue::Nil => Priority::State,
        LuaValue::String(class) => {
            Priority::parse(&class.display_lossy().to_string()).map_err(|e| fail(&e))?
        }
        _ => return Err(fail("`class` must be a string")),
    };
    let (holds, when) = match when_value {
        LuaValue::Nil => (class != Priority::Asked, None),
        LuaValue::Boolean(holds) => (holds, None),
        value if is_dynamic(value) => (false, Some(value)),
        _ => return Err(fail("`when` must be a function, a signal or a boolean")),
    };
    let motion = match entry.get_value(ctx, "motion") {
        LuaValue::Nil => shared,
        LuaValue::Table(motion) => Some(parse_motion(ctx, motion).map_err(|e| fail(&e))?),
        _ => return Err(fail("`motion` must be a behaviour table")),
    };
    let LuaValue::Table(set) = entry.get_value(ctx, "set") else {
        return Err(fail("it needs a `set` table of the properties it claims"));
    };
    let mut targets = Vec::new();
    let mut dynamic = Vec::new();
    for (property, value) in set.iter(ctx) {
        let LuaValue::String(property) = property else {
            return Err(fail("`set` keys must be property names"));
        };
        let property = property.display_lossy().to_string();
        if is_dynamic(value) {
            targets.push((property.clone(), None));
            dynamic.push((property, value));
        } else {
            let value =
                lua_to_scene(ctx, value, 0).map_err(|e| fail(&format!("`{property}`: {e}")))?;
            targets.push((property, Some(value)));
        }
    }
    Ok(Declared {
        spec: ClaimSpec {
            name,
            class,
            motion,
            holds,
            targets,
        },
        when,
        targets: dynamic,
    })
}

/// A function, or a signal: what a claim follows rather than holds.
fn is_dynamic(value: LuaValue<'_>) -> bool {
    match value {
        LuaValue::Function(_) => true,
        LuaValue::UserData(data) => data.downcast_static::<SignalToken>().is_ok(),
        _ => false,
    }
}

/// A claimed property takes its value from its claims and nothing else:
/// a binding, a follow, a loop or a state that writes it would fight them.
fn refuse_other_owner(
    state: &ReactiveState,
    node: NodeHandle,
    property: &str,
) -> Result<(), String> {
    refuse_runtime_owned(state, node, property)?;
    let bound = state.reactive().effects().any(|(_, effect)| {
        matches!(&effect.sink, Some(EffectSink::Property(sink))
            if sink.node == node && sink.property == property)
    });
    if bound {
        return Err(format!(
            "`{property}` has a binding; give the function to the claim's `set` instead"
        ));
    }
    let animation = state.animation();
    if animation
        .follows
        .iter()
        .any(|follow| follow.target == node && follow.property == property)
    {
        return Err(format!("`{property}` follows another node (ui.follow)"));
    }
    if animation
        .loops
        .get(&node)
        .is_some_and(|loops| loops.contains_key(property))
    {
        return Err(format!("`{property}` runs a loop"));
    }
    let stated = state.states().get(node).is_some_and(|set| {
        set.definitions
            .values()
            .any(|state| state.properties.iter().any(|(name, _)| name == property))
    });
    if stated {
        return Err(format!("`{property}` changes with the node's `states`"));
    }
    Ok(())
}

/// A function that turns a condition or target into the function a
/// binding runs: a signal read, and a condition taken by Lua's truth.
fn reader_factory<'gc>(ctx: Context<'gc>) -> Result<Closure<'gc>, String> {
    let source = br#"
        local value, truth = ...
        local read = value
        if type(value) ~= "function" then read = function() return value:get() end end
        if truth then return function() return not not read() end end
        return function() return read() end
    "#;
    Closure::load(ctx, Some("claim input"), &source[..]).map_err(|error| error.to_string())
}

fn read_closure<'gc>(
    ctx: Context<'gc>,
    factory: Closure<'gc>,
    value: LuaValue<'gc>,
    truth: bool,
    limits: Limits,
) -> Result<Closure<'gc>, String> {
    let executor = Executor::start(ctx, factory.into(), (value, truth));
    drive_executor(ctx, executor, limits, limits.effect_fuel, "claim input")?;
    match executor.take_result::<Closure>(ctx) {
        Ok(Ok(closure)) => Ok(closure),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Declares the binding that feeds one input of a claim. It goes with
/// the node, or with the claim when the claim is removed.
fn add_input<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    node: NodeHandle,
    claim: ClaimId,
    input: ClaimInput,
    closure: Closure<'gc>,
) -> u64 {
    let mut state = state.borrow_mut();
    let token = state.tracking_mut().next_effect_token();
    state.reactive_mut().add_effect(
        token,
        LuaEffect {
            handler: crate::vm::handler_store::register(ctx.stash(closure)),
            sink: Some(EffectSink::Claim(ClaimSink { node, claim, input })),
            owner: Some(node),
        },
    );
    state.register_external_effect(token, format!("{node:?}.claims"));
    token
}

/// What a configuration holds of one claim: its presence, `remove`, and
/// for an asked claim `ask` and `release`.
fn claim_handle<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    id: ClaimId,
    class: Priority,
    presence: morf_scene::reactive::SignalId,
    metatable: &luna::StashedTable,
) -> Table<'gc> {
    let handle = Table::new(&ctx);
    let presence = UserData::new_static(&ctx, SignalToken { id: presence });
    presence.set_metatable(ctx, Some(ctx.fetch(metatable)));
    handle.set_field(ctx, "presence", presence);
    if class == Priority::Asked {
        for (field, asked) in [("ask", true), ("release", false)] {
            let state = Rc::clone(state);
            let callback = Callback::from_fn(&ctx, move |_, _, _| {
                let mut state = state.borrow_mut();
                state
                    .animation_mut()
                    .claims
                    .set_asked(id, asked)
                    .map_err(HostError)?;
                Ok(CallbackReturn::Return)
            });
            handle.set_field(ctx, field, callback);
        }
    }
    let state = Rc::clone(state);
    let remove = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let mut state = state.borrow_mut();
        let released = state.animation_mut().claims.remove(id);
        let removed = released.is_some();
        if let Some(released) = released {
            for token in released.effects {
                crate::api::ui::retention::dispose_effect(&mut state, token);
            }
            state.reactive_mut().forget_signals(released.signals);
        }
        stack.replace(ctx, removed);
        Ok(CallbackReturn::Return)
    });
    handle.set_field(ctx, "remove", remove);
    handle
}
