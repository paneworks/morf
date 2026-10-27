//! Acting on the compositor: other people's windows, and its own key
//! bindings held off while the shell has focus.

use luna::{Callback, CallbackReturn, Closure, Context, Table};
use std::rc::Rc;

use crate::{api::ui::scene::HostError, runtime::state::*, vm::types::ToplevelRequest};

/// Installs `morf.toplevels`' actions and `morf.shortcuts`.
pub(crate) fn install_compositor_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    // Acting on somebody else's window. Named for the protocol rather than
    // called `morf.window`, which is this shell's own surface constructor and a
    // different thing entirely.
    let toplevel = crate::api::namespace::table(ctx, morf, "toplevels");
    for (name, takes_value) in [
        ("activate", false),
        ("close", false),
        ("set_maximized", true),
        ("set_minimized", true),
        ("set_fullscreen", true),
    ] {
        let request_state = Rc::clone(&state);
        let action = name.to_owned();
        let entry = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (identifier, value): (String, Option<bool>) = stack.consume(ctx)?;
            let mut state = request_state.borrow_mut();
            state
                .requests_mut()
                .queue_toplevel(ToplevelRequest {
                    identifier,
                    action: action.clone(),
                    // The setters default to turning the state on, so
                    // `set_maximized(id)` reads the way it looks.
                    value: if takes_value {
                        value.unwrap_or(true)
                    } else {
                        true
                    },
                    rect: None,
                })
                .map_err(HostError)?;
            Ok(CallbackReturn::Return)
        });
        toplevel.set_field(ctx, name, entry);
    }
    // Where a window's entry is, so a compositor that animates minimize has
    // somewhere to send it. Coordinates on the shell's own surface.
    let target_state = Rc::clone(&state);
    let set_minimize_target = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (identifier, x, y, width, height): (String, i64, i64, i64, i64) = stack.consume(ctx)?;
        let narrow = |value: i64| {
            i32::try_from(value)
                .map_err(|_| HostError(format!("`{value}` does not fit a surface coordinate")))
        };
        let mut state = target_state.borrow_mut();
        state
            .requests_mut()
            .queue_toplevel(ToplevelRequest {
                identifier,
                action: "set_minimize_target".to_owned(),
                value: true,
                rect: Some((narrow(x)?, narrow(y)?, narrow(width)?, narrow(height)?)),
            })
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    toplevel.set_field(ctx, "set_minimize_target", set_minimize_target);

    // The compositor's own key bindings, held off the shell while it has
    // focus. `active` is the compositor's answer, which is not always yes.
    let shortcuts_state = Rc::clone(&state);
    let shortcuts_inhibit = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let inhibited: bool = stack.consume(ctx)?;
        shortcuts_state
            .borrow_mut()
            .requests_mut()
            .set_shortcuts_inhibited(inhibited);
        Ok(CallbackReturn::Return)
    });
    // The compositor's answer, delivered rather than polled: a binding that
    // read a plain function would never re-run when the answer changed.
    let subscribe_state = Rc::clone(&state);
    let shortcuts_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        let mut state = subscribe_state.borrow_mut();
        if state.requests().shortcuts_callbacks.len() >= 64 {
            return Err(HostError("shortcuts callback limit reached".into()).into());
        }
        state
            .requests_mut()
            .shortcuts_callbacks
            .push(crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    let shortcuts = Table::new(&ctx);
    shortcuts.set_field(ctx, "inhibit", shortcuts_inhibit);
    shortcuts.set_field(ctx, "subscribe", shortcuts_subscribe);
    morf.set_field(ctx, "shortcuts", shortcuts);
}
