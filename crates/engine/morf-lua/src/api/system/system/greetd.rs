//! `morf.greetd`: logging in through greetd, as a blocking client or as a
//! conversation off the drawing thread.

use super::*;

/// Installs `morf.greetd`.
pub(super) fn install_greetd<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let greetd_create = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (greetd, username): (UserRef<GreetdToken>, String) = stack.consume(ctx)?;
        let response = greetd
            .client
            .borrow_mut()
            .create_session(&username)
            .map_err(|error| HostError(error.to_string()))?;
        stack.replace(ctx, greetd_response(ctx, response));
        Ok(CallbackReturn::Return)
    });
    let greetd_respond = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (greetd, response): (UserRef<GreetdToken>, Option<String>) = stack.consume(ctx)?;
        let response = greetd
            .client
            .borrow_mut()
            .respond(response.as_deref())
            .map_err(|error| HostError(error.to_string()))?;
        stack.replace(ctx, greetd_response(ctx, response));
        Ok(CallbackReturn::Return)
    });
    let greetd_start = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (greetd, command, environment): (UserRef<GreetdToken>, Table, Table) =
            stack.consume(ctx)?;
        let command = table_string_array(ctx, command, 64).map_err(HostError)?;
        let environment = table_string_array(ctx, environment, 256).map_err(HostError)?;
        let response = greetd
            .client
            .borrow_mut()
            .start_session(&command, &environment)
            .map_err(|error| HostError(error.to_string()))?;
        stack.replace(ctx, greetd_response(ctx, response));
        Ok(CallbackReturn::Return)
    });
    let greetd_cancel = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let greetd: UserRef<GreetdToken> = stack.consume(ctx)?;
        let response = greetd
            .client
            .borrow_mut()
            .cancel_session()
            .map_err(|error| HostError(error.to_string()))?;
        stack.replace(ctx, greetd_response(ctx, response));
        Ok(CallbackReturn::Return)
    });
    let greetd_methods = Table::new(&ctx);
    greetd_methods.set_field(ctx, "create_session", greetd_create);
    greetd_methods.set_field(ctx, "respond", greetd_respond);
    greetd_methods.set_field(ctx, "start_session", greetd_start);
    greetd_methods.set_field(ctx, "cancel_session", greetd_cancel);
    let greetd_metatable = Table::new(&ctx);
    greetd_metatable.set_field(ctx, "__index", greetd_methods);
    let greetd_metatable = ctx.stash(greetd_metatable);
    let greetd_connect = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let path: Option<String> = stack.consume(ctx)?;
        let timeout = Duration::from_secs(2);
        let client = match path {
            Some(path) => GreetdClient::connect(path, timeout),
            None => GreetdClient::connect_environment(timeout),
        }
        .map_err(|error| HostError(error.to_string()))?;
        let userdata = UserData::new_static(
            &ctx,
            GreetdToken {
                client: RefCell::new(client),
            },
        );
        userdata.set_metatable(ctx, Some(ctx.fetch(&greetd_metatable)));
        stack.replace(ctx, userdata);
        Ok(CallbackReturn::Return)
    });
    // The login as a conversation, off the drawing thread: `converse` asks
    // for a session and returns at once; what greetd says arrives through
    // `on_message`, and the answers go back through `respond`, `start` and
    // `cancel`. The blocking client above stays for a script that wants a
    // straight line; a greeter that draws while a reader waits wants this.
    let converse_state = Rc::clone(&state);
    let greetd_on_message = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (session, callback): (UserRef<GreetdSessionToken>, Closure) = stack.consume(ctx)?;
        let mut state = converse_state.borrow_mut();
        let entry = PendingGreetdSession {
            conversation: Rc::clone(&session.conversation),
            callback: crate::vm::handler_store::register(ctx.stash(callback)),
        };
        let existing = state
            .greetd_sessions
            .iter()
            .position(|entry| Rc::ptr_eq(&entry.conversation, &session.conversation));
        match existing {
            Some(index) => state.greetd_sessions[index] = entry,
            None => state.greetd_sessions.push(entry),
        }
        Ok(CallbackReturn::Return)
    });
    let greetd_session_respond = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (session, answer): (UserRef<GreetdSessionToken>, Option<String>) =
            stack.consume(ctx)?;
        let sent = session.conversation.borrow().respond(answer);
        stack.replace(ctx, sent);
        Ok(CallbackReturn::Return)
    });
    let greetd_session_start = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (session, command, environment): (UserRef<GreetdSessionToken>, Table, Table) =
            stack.consume(ctx)?;
        let command = table_string_array(ctx, command, 64).map_err(HostError)?;
        let environment = table_string_array(ctx, environment, 256).map_err(HostError)?;
        let sent = session
            .conversation
            .borrow_mut()
            .start_session(command, environment);
        stack.replace(ctx, sent);
        Ok(CallbackReturn::Return)
    });
    let greetd_session_cancel = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let session: UserRef<GreetdSessionToken> = stack.consume(ctx)?;
        let sent = session.conversation.borrow_mut().cancel();
        stack.replace(ctx, sent);
        Ok(CallbackReturn::Return)
    });
    let session_methods = Table::new(&ctx);
    session_methods.set_field(ctx, "on_message", greetd_on_message);
    session_methods.set_field(ctx, "respond", greetd_session_respond);
    session_methods.set_field(ctx, "start", greetd_session_start);
    session_methods.set_field(ctx, "cancel", greetd_session_cancel);
    let session_metatable = Table::new(&ctx);
    session_metatable.set_field(ctx, "__index", session_methods);
    let session_metatable = ctx.stash(session_metatable);
    let greetd_converse = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (username, path): (String, Option<String>) = stack.consume(ctx)?;
        let conversation = GreetdConversation::begin(path.map(std::path::PathBuf::from), username);
        let userdata = UserData::new_static(
            &ctx,
            GreetdSessionToken {
                conversation: Rc::new(RefCell::new(conversation)),
            },
        );
        userdata.set_metatable(ctx, Some(ctx.fetch(&session_metatable)));
        stack.replace(ctx, userdata);
        Ok(CallbackReturn::Return)
    });
    let greetd = Table::new(&ctx);
    greetd.set_field(ctx, "connect", greetd_connect);
    greetd.set_field(ctx, "converse", greetd_converse);
    morf.set_field(ctx, "greetd", greetd);
}
