//! `morf.audio`'s commands -- volume, channel volumes, mute, the default
//! device, moving a stream -- and `on_changed`.

use super::*;

/// Installs the commands and `on_changed` on `audio`.
pub(super) fn install_commands<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    audio: Table<'gc>,
) {
    let function =
        |name: &'static str, callback: Callback<'gc>| audio.set_field(ctx, name, callback);

    function(
        "set_volume",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (id, volume): (LuaValue, f64) = stack.consume(ctx)?;
                let id = object_id(id, "set_volume target")?;
                if !volume.is_finite() {
                    return Err(HostError("volume must be a finite number".into()).into());
                }
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_volume(id, volume as f32);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "set_channel_volumes",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (id, volumes): (LuaValue, Table) = stack.consume(ctx)?;
                let id = object_id(id, "set_channel_volumes target")?;
                let mut list = Vec::new();
                for index in 1..=volumes.length(&ctx) {
                    let volume = match volumes.get_value(ctx, index) {
                        LuaValue::Number(value) => value,
                        LuaValue::Integer(value) => value as f64,
                        _ => {
                            return Err(HostError(
                                "set_channel_volumes takes a list of numbers".into(),
                            )
                            .into());
                        }
                    };
                    if !volume.is_finite() {
                        return Err(HostError("volume must be a finite number".into()).into());
                    }
                    list.push(volume as f32);
                }
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_channel_volumes(id, &list);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "set_mute",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (id, muted): (LuaValue, bool) = stack.consume(ctx)?;
                let id = object_id(id, "set_mute target")?;
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_mute(id, muted);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "set_default",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let id = object_id(stack.consume(ctx)?, "set_default target")?;
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_default(id);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "move_stream",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (stream, device): (LuaValue, LuaValue) = stack.consume(ctx)?;
                let stream = object_id(stream, "stream")?;
                let device = object_id(device, "device")?;
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().move_stream(stream, device);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "on_changed",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let callback: Closure = stack.consume(ctx)?;
                let id = {
                    let mut state = state.borrow_mut();
                    let host = host(&mut state);
                    if host.session.listeners.len() >= morf_audio::session::MAX_LISTENERS {
                        return Err(
                            HostError("too many morf.audio.on_changed handlers".into()).into()
                        );
                    }
                    host.session
                        .listen(crate::vm::handler_store::register(ctx.stash(callback)))
                        .map_err(HostError)?
                };
                let stop = Callback::from_fn(&ctx, {
                    let state = Rc::clone(&state);
                    move |_, _, _| {
                        host(&mut state.borrow_mut()).session.unlisten(id);
                        Ok(CallbackReturn::Return)
                    }
                });
                let handle = Table::new(&ctx);
                handle.set_field(ctx, "stop", stop);
                stack.replace(ctx, handle);
                Ok(CallbackReturn::Return)
            }
        }),
    );
}
