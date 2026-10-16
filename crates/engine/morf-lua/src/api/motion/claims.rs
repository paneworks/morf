//! `ui.claims(node, { ... })`: choreography claims (PLAN item 5).
//!
//! Each named entry claims the properties in its `set` for as long as
//! its condition holds:
//!
//! ```lua
//! local claims = ui.claims(panel, {
//!   open = { when = opened, class = "state", spring = { stiffness = 260 },
//!            set = { x = 0, opacity = 1 } },
//!   peek = { class = "asked", duration = 200,
//!            set = { x = -40 } },
//! })
//! claims.open.presence:get()   -- 0..1, an ordinary signal
//! claims.peek.ask()            -- raise the asked claim by hand
//! claims.peek.release()
//! ```
//!
//! `when` is a signal (or a boolean); without one the claim is asked
//! and released by hand. `class` is one of ambient, posture, reflex,
//! asked, state (highest last, and the winner). Motion is the
//! property's own: `spring = { ... }` or `duration = <ms>` (a tween),
//! installed as the claimed properties' behaviour, and a winner change
//! retargets that motion -- claims never integrate motion themselves.
//! Writing a claimed property directly is refused while claims own it,
//! as a write to a read-only property is.

use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::rc::Rc;

use morf_runtime::animation::behaviors;
use morf_runtime::animation::claims::{Claim, ClaimMotion, Priority};
use morf_value::IpcValue;

use crate::api::ui::bindings::lua_to_scene;
use crate::api::ui::scene::HostError;
use crate::runtime::state::*;
use crate::value::lua::parse_easing;
use crate::value::table::table_number;
use crate::value::tokens::{NodeToken, SignalToken};

pub(crate) fn install_claims_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    ui: Table<'gc>,
) {
    let claims = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (node, spec): (UserRef<NodeToken>, Table) = stack.consume(ctx)?;
            let node = node.handle;
            let mut entries: Vec<(String, Table)> = Vec::new();
            for (key, value) in spec.iter(ctx) {
                let LuaValue::String(name) = key else {
                    return Err(HostError("ui.claims keys must be claim names".into()).into());
                };
                let LuaValue::Table(entry) = value else {
                    return Err(HostError("each claim must be a table".into()).into());
                };
                entries.push((name.display_lossy().to_string(), entry));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));

            let handles = Table::new(&ctx);
            for (index, (name, entry)) in entries.iter().enumerate() {
                let class = match entry.get_value(ctx, "class") {
                    LuaValue::Nil => Priority::Ambient,
                    LuaValue::String(class) => {
                        Priority::parse(&class.display_lossy().to_string()).map_err(HostError)?
                    }
                    _ => return Err(HostError("claim class must be a string".into()).into()),
                };
                let order = match entry.get_value(ctx, "order") {
                    LuaValue::Integer(order) => order as u32,
                    LuaValue::Number(order) => order as u32,
                    _ => index as u32,
                };
                let motion = claim_motion(ctx, entry)?;
                let condition = match entry.get_value(ctx, "when") {
                    LuaValue::Nil | LuaValue::Boolean(_) => None,
                    LuaValue::UserData(data) => Some(
                        data.downcast_static::<SignalToken>()
                            .map_err(|_| HostError("claim `when` must be a signal".into()))?
                            .id,
                    ),
                    _ => {
                        return Err(
                            HostError("claim `when` must be a signal or a boolean".into()).into(),
                        );
                    }
                };
                // No condition, or a constant true one: the claim
                // holds from the start (the resting posture), and
                // ask/release are how a handler moves it from there.
                let always = matches!(
                    entry.get_value(ctx, "when"),
                    LuaValue::Nil | LuaValue::Boolean(true)
                );
                let LuaValue::Table(set) = entry.get_value(ctx, "set") else {
                    return Err(HostError(format!("claim `{name}` needs a `set` table")).into());
                };
                let presence_id = {
                    let mut state = state.borrow_mut();
                    state
                        .reactive_mut()
                        .signal(format!("claims/{name}/presence"), IpcValue::Number(0.0))
                        .ok_or_else(|| HostError("reactive graph is running".into()))?
                };
                let handle = Table::new(&ctx);
                let mut ids = Vec::new();
                for (property, value) in set.iter(ctx) {
                    let LuaValue::String(property) = property else {
                        return Err(
                            HostError("claim `set` keys must be property names".into()).into()
                        );
                    };
                    let property = property.display_lossy().to_string();
                    let target = lua_to_scene(ctx, value, 0).map_err(HostError)?;
                    let mut claim =
                        Claim::new(name.clone(), node, property, target, class, order, motion);
                    claim.active = always;
                    claim.manual = always;
                    claim.condition = condition;
                    claim.presence_signal = Some(presence_id);
                    let id = {
                        let mut state = state.borrow_mut();
                        let mut claims = std::mem::take(&mut state.animation_mut().claims);
                        let added = claims.add(state.scene_mut(), claim);
                        state.animation_mut().claims = claims;
                        added.map_err(|error| HostError(error.to_string()))?
                    };
                    ids.push(id);
                }
                // One handle per claim name, however many properties it
                // claims: ask and release move them together.
                register_handle(ctx, &state, &handle, ids);
                let metatable = state
                    .borrow()
                    .metatables()
                    .signal()
                    .ok_or_else(|| HostError("signals are not installed".into()))?;
                let presence = UserData::new_static(&ctx, SignalToken { id: presence_id });
                presence.set_metatable(ctx, Some(ctx.fetch(&metatable)));
                handle.set_field(ctx, "presence", presence);
                handles.set(ctx, ctx.intern(name.as_bytes()), handle)?;
            }
            stack.replace(ctx, handles);
            Ok(CallbackReturn::Return)
        }
    });
    ui.set_field(ctx, "claims", claims);
}

fn register_handle<'gc>(
    ctx: Context<'gc>,
    state: &Rc<StateCell<ReactiveState>>,
    handle: &Table<'gc>,
    ids: Vec<usize>,
) {
    for (field, manual) in [("ask", true), ("release", false)] {
        let state = Rc::clone(state);
        let ids = ids.clone();
        let callback = Callback::from_fn(&ctx, move |_, _, _| {
            let mut state = state.borrow_mut();
            for id in &ids {
                state.animation_mut().claims.set_manual(*id, manual);
            }
            Ok(CallbackReturn::Return)
        });
        let _ = handle.set_field(ctx, field, callback);
    }
}

fn claim_motion<'gc>(ctx: Context<'gc>, entry: &Table<'gc>) -> Result<ClaimMotion, HostError> {
    match entry.get_value(ctx, "spring") {
        LuaValue::Nil => {}
        LuaValue::Boolean(true) => {
            return Ok(ClaimMotion::Spring(behaviors::spring(
                1.0, 18.0, 180.0, 0.001,
            )));
        }
        LuaValue::Table(spring) => {
            return Ok(ClaimMotion::Spring(behaviors::spring(
                table_number(ctx, spring, "mass", 1.0).map_err(HostError)?,
                table_number(ctx, spring, "damping", 18.0).map_err(HostError)?,
                table_number(ctx, spring, "stiffness", 180.0).map_err(HostError)?,
                table_number(ctx, spring, "epsilon", 0.001).map_err(HostError)?,
            )));
        }
        _ => return Err(HostError("claim `spring` must be a table or true".into())),
    }
    let milliseconds = match entry.get_value(ctx, "duration") {
        LuaValue::Nil => return Ok(ClaimMotion::None),
        LuaValue::Integer(milliseconds) => milliseconds as f64,
        LuaValue::Number(milliseconds) => milliseconds,
        _ => return Err(HostError("claim `duration` must be milliseconds".into())),
    };
    let duration = behaviors::duration(milliseconds).map_err(HostError)?;
    let easing = parse_easing(ctx, entry.get_value(ctx, "easing")).map_err(HostError)?;
    Ok(ClaimMotion::Tween(morf_scene::Behavior::timed(
        duration, easing,
    )))
}
