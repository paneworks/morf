//! `morf.virtual_keyboard`, `morf.input_method` and `morf.text_input`:
//! typing into other clients, and being typed into through the compositor.

use super::*;

/// Installs the virtual keyboard, the input method and the text input.
pub(super) fn install_text_entry<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let virtual_key_state = Rc::clone(&state);
    let virtual_key = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (keycode, pressed): (i64, bool) = stack.consume(ctx)?;
        let keycode = u32::try_from(keycode)
            .map_err(|_| HostError("virtual keycode must fit an unsigned 32-bit value".into()))?;
        let mut state = virtual_key_state.borrow_mut();
        state
            .requests
            .queue_virtual_keyboard(VirtualKeyboardRequest::Key { keycode, pressed })
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let virtual_modifiers_state = Rc::clone(&state);
    let virtual_modifiers = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let values: (i64, i64, i64, i64) = stack.consume(ctx)?;
        let request = VirtualKeyboardRequest::Modifiers {
            depressed: u32::try_from(values.0)
                .map_err(|_| HostError("depressed modifiers must fit u32".into()))?,
            latched: u32::try_from(values.1)
                .map_err(|_| HostError("latched modifiers must fit u32".into()))?,
            locked: u32::try_from(values.2)
                .map_err(|_| HostError("locked modifiers must fit u32".into()))?,
            group: u32::try_from(values.3)
                .map_err(|_| HostError("keyboard group must fit u32".into()))?,
        };
        let mut state = virtual_modifiers_state.borrow_mut();
        state
            .requests
            .queue_virtual_keyboard(request)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let virtual_keyboard = Table::new(&ctx);
    virtual_keyboard.set_field(ctx, "key", virtual_key);
    virtual_keyboard.set_field(ctx, "modifiers", virtual_modifiers);
    morf.set_field(ctx, "virtual_keyboard", virtual_keyboard);
    let input_method_subscribe_state = Rc::clone(&state);
    let input_method_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        let callback = crate::vm::handler_store::register(ctx.stash(callback));
        input_method_subscribe_state
            .borrow_mut()
            .requests
            .subscribe_input_method(callback)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let input_method_commit_state = Rc::clone(&state);
    let input_method_commit = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let text: String = stack.consume(ctx)?;
        if text.len() > 4_000 {
            return Err(HostError("input method text limit reached".into()).into());
        }
        let mut state = input_method_commit_state.borrow_mut();
        state
            .requests
            .queue_input_method(InputMethodRequest::Commit(text))
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let input_method_preedit_state = Rc::clone(&state);
    let input_method_preedit = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (text, begin, end): (String, i64, i64) = stack.consume(ctx)?;
        if text.len() > 4_000 {
            return Err(HostError("input method text limit reached".into()).into());
        }
        let begin = i32::try_from(begin)
            .map_err(|_| HostError("preedit cursor start must fit i32".into()))?;
        let end =
            i32::try_from(end).map_err(|_| HostError("preedit cursor end must fit i32".into()))?;
        let mut state = input_method_preedit_state.borrow_mut();
        state
            .requests
            .queue_input_method(InputMethodRequest::Preedit { text, begin, end })
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let input_method_delete_state = Rc::clone(&state);
    let input_method_delete = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (before, after): (i64, i64) = stack.consume(ctx)?;
        let before = u32::try_from(before)
            .map_err(|_| HostError("delete before length must fit u32".into()))?;
        let after = u32::try_from(after)
            .map_err(|_| HostError("delete after length must fit u32".into()))?;
        let mut state = input_method_delete_state.borrow_mut();
        state
            .requests
            .queue_input_method(InputMethodRequest::Delete { before, after })
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let input_method = Table::new(&ctx);
    input_method.set_field(ctx, "subscribe", input_method_subscribe);
    input_method.set_field(ctx, "commit", input_method_commit);
    input_method.set_field(ctx, "preedit", input_method_preedit);
    input_method.set_field(ctx, "delete", input_method_delete);
    morf.set_field(ctx, "input_method", input_method);
    let text_input_subscribe_state = Rc::clone(&state);
    let text_input_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        let callback = crate::vm::handler_store::register(ctx.stash(callback));
        text_input_subscribe_state
            .borrow_mut()
            .requests
            .subscribe_text_input(callback)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let text_input_disable_state = Rc::clone(&state);
    let text_input_disable = Callback::from_fn(&ctx, move |_, _, _| {
        let mut state = text_input_disable_state.borrow_mut();
        state
            .requests
            .queue_text_input(TextInputRequest::Disable)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let text_input_surrounding_state = Rc::clone(&state);
    let text_input_surrounding = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (text, cursor, anchor): (String, i64, i64) = stack.consume(ctx)?;
        if text.len() > 4_000 {
            return Err(HostError("text input text limit reached".into()).into());
        }
        let cursor = i32::try_from(cursor)
            .map_err(|_| HostError("text input cursor must fit i32".into()))?;
        let anchor = i32::try_from(anchor)
            .map_err(|_| HostError("text input anchor must fit i32".into()))?;
        let mut state = text_input_surrounding_state.borrow_mut();
        state
            .requests
            .queue_text_input(TextInputRequest::Surrounding {
                text,
                cursor,
                anchor,
            })
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let text_input_content_state = Rc::clone(&state);
    let text_input_content = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (hints, purpose): (i64, i64) = stack.consume(ctx)?;
        let hints =
            u32::try_from(hints).map_err(|_| HostError("text input hints must fit u32".into()))?;
        let purpose = u32::try_from(purpose)
            .map_err(|_| HostError("text input purpose must fit u32".into()))?;
        let mut state = text_input_content_state.borrow_mut();
        state
            .requests
            .queue_text_input(TextInputRequest::ContentType { hints, purpose })
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let text_input_rect_state = Rc::clone(&state);
    let text_input_rect = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let values: (i64, i64, i64, i64) = stack.consume(ctx)?;
        let request = TextInputRequest::CursorRect {
            x: i32::try_from(values.0).map_err(|_| HostError("cursor x must fit i32".into()))?,
            y: i32::try_from(values.1).map_err(|_| HostError("cursor y must fit i32".into()))?,
            width: i32::try_from(values.2)
                .map_err(|_| HostError("cursor width must fit i32".into()))?,
            height: i32::try_from(values.3)
                .map_err(|_| HostError("cursor height must fit i32".into()))?,
        };
        let mut state = text_input_rect_state.borrow_mut();
        state
            .requests
            .queue_text_input(request)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let text_input = Table::new(&ctx);
    text_input.set_field(ctx, "subscribe", text_input_subscribe);
    text_input.set_field(ctx, "disable", text_input_disable);
    text_input.set_field(ctx, "surrounding", text_input_surrounding);
    text_input.set_field(ctx, "content_type", text_input_content);
    text_input.set_field(ctx, "cursor_rect", text_input_rect);
    morf.set_field(ctx, "text_input", text_input);
}
