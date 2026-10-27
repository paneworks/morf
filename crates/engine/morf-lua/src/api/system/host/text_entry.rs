//! `morf.virtual_keyboard` and `morf.input_method`: typing into other
//! clients, and being typed into through the compositor.

use super::*;

/// Installs the virtual keyboard and the input method.
pub(super) fn install_text_entry<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let virtual_key_state = Rc::clone(&state);
    let virtual_key = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (keycode, pressed): (i64, bool) = stack.consume(ctx)?;
        let keycode = u32::try_from(keycode)
            .map_err(|_| HostError("virtual keycode must fit an unsigned 32-bit value".into()))?;
        let mut state = virtual_key_state.borrow_mut();
        state
            .requests_mut()
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
            .requests_mut()
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
            .requests_mut()
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
            .requests_mut()
            .queue_input_method(InputMethodRequest::Commit(text))
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
            .requests_mut()
            .queue_input_method(InputMethodRequest::Delete { before, after })
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let input_method = Table::new(&ctx);
    input_method.set_field(ctx, "subscribe", input_method_subscribe);
    input_method.set_field(ctx, "commit", input_method_commit);
    input_method.set_field(ctx, "delete", input_method_delete);
    morf.set_field(ctx, "input_method", input_method);
}
