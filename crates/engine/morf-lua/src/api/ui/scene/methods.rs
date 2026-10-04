//! The methods particular kinds of node answer: a terminal's and a text
//! input's.

use super::*;

/// A terminal's methods, called as `term:write("ls\r")`.
pub(super) fn terminal_method<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    name: &str,
) -> Option<Callback<'gc>> {
    use crate::terminals;
    let state = Rc::clone(state);
    let busy = || HostError("terminals cannot be used from inside a layout function".to_owned());
    Some(match name {
        // Bytes to the program, as if typed: `true`, or `false, why`.
        "write" | "paste" => {
            let paste = name == "paste";
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let (node, data): (UserRef<NodeToken>, luna::String) = stack.consume(ctx)?;
                if data.as_bytes().len() > morf_io::MAX_OUTGOING {
                    return Err(HostError("a terminal write is at most 4 MiB".into()).into());
                }
                let mut state = state.try_borrow_mut().map_err(|_| busy())?;
                let result = if paste {
                    terminals::paste(&mut state, node.handle, &data.display_lossy().to_string())
                } else {
                    terminals::write(&mut state, node.handle, data.as_bytes().to_vec())
                };
                match result {
                    Ok(()) => stack.replace(ctx, true),
                    Err(why) => stack.replace(ctx, (false, why.as_str())),
                }
                Ok(CallbackReturn::Return)
            })
        }
        // A signal to the program: `"TERM"` unless named; whether it ran.
        "kill" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, signal): (UserRef<NodeToken>, LuaValue) = stack.consume(ctx)?;
            let signal = match signal {
                LuaValue::Nil => morf_io::signal_number("TERM"),
                LuaValue::Integer(number) => morf_io::signal_number(&number.to_string()),
                LuaValue::String(name) => morf_io::signal_number(&name.display_lossy().to_string()),
                _ => None,
            }
            .ok_or_else(|| HostError("kill takes a signal name or number".into()))?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            stack.replace(ctx, terminals::kill(&mut state, node.handle, signal));
            Ok(CallbackReturn::Return)
        }),
        // Through the history: up by `lines`, down by a negative number,
        // back to the bottom with none. Whether the view moved.
        "scroll" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, lines): (UserRef<NodeToken>, Option<i64>) = stack.consume(ctx)?;
            let lines = lines.unwrap_or(0).clamp(-100_000, 100_000) as i32;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            stack.replace(ctx, terminals::scroll(&mut state, node.handle, lines));
            Ok(CallbackReturn::Return)
        }),
        // What the screen shows, as text: one line per row.
        "text" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let state = state.try_borrow().map_err(|_| busy())?;
            let text = terminals::text(&state, node.handle).unwrap_or_default();
            stack.replace(ctx, luna::String::from_slice(&ctx, text.as_bytes()));
            Ok(CallbackReturn::Return)
        }),
        // The text selected with the pointer, or nil.
        "selection" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let state = state.try_borrow().map_err(|_| busy())?;
            match terminals::selection(&state, node.handle) {
                Some(text) => stack.replace(ctx, luna::String::from_slice(&ctx, text.as_bytes())),
                None => stack.replace(ctx, LuaValue::Nil),
            }
            Ok(CallbackReturn::Return)
        }),
        "clear_selection" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            stack.replace(ctx, terminals::clear_selection(&mut state, node.handle));
            Ok(CallbackReturn::Return)
        }),
        "pid" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let state = state.try_borrow().map_err(|_| busy())?;
            match terminals::pid(&state, node.handle) {
                Some(pid) => stack.replace(ctx, i64::from(pid)),
                None => stack.replace(ctx, LuaValue::Nil),
            }
            Ok(CallbackReturn::Return)
        }),
        _ => return None,
    })
}

/// A text input's methods, called as `input:select(0, 4)`.
///
/// Offsets are bytes, as `cursor_position` is: the number of bytes before the
/// place meant, so `text:sub(1, n)` is what lies before offset `n`.
pub(super) fn text_input_method<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    name: &str,
) -> Option<Callback<'gc>> {
    use crate::text_inputs;
    let state = Rc::clone(state);
    let offset = |value: i64| value.max(0) as usize;
    Some(match name {
        "select" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, start, end): (UserRef<NodeToken>, i64, i64) = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            text_inputs::select(&mut state, node.handle, offset(start), offset(end));
            Ok(CallbackReturn::Return)
        }),
        "select_all" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            text_inputs::select_all(&mut state, node.handle);
            Ok(CallbackReturn::Return)
        }),
        "deselect" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            let cursor = state
                .scene
                .number(node.handle, "cursor_position")
                .unwrap_or(0.0);
            text_inputs::select(&mut state, node.handle, cursor as usize, cursor as usize);
            Ok(CallbackReturn::Return)
        }),
        "insert" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let (node, text): (UserRef<NodeToken>, String) = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            let edited = text_inputs::insert(&mut state, node.handle, &text);
            stack.replace(ctx, edited);
            Ok(CallbackReturn::Return)
        }),
        "selected_text" => Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let node: UserRef<NodeToken> = stack.consume(ctx)?;
            let mut state = state.try_borrow_mut().map_err(|_| busy())?;
            let text = text_inputs::selected_text(&mut state, node.handle);
            stack.replace(ctx, luna::String::from_slice(&ctx, text.as_bytes()));
            Ok(CallbackReturn::Return)
        }),
        "undo" | "redo" => {
            let redo = name == "redo";
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let node: UserRef<NodeToken> = stack.consume(ctx)?;
                let mut state = state.try_borrow_mut().map_err(|_| busy())?;
                let changed = text_inputs::history(&mut state, node.handle, redo);
                stack.replace(ctx, changed);
                Ok(CallbackReturn::Return)
            })
        }
        _ => return None,
    })
}
