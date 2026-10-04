use luna::{
    Callback, CallbackReturn, Closure, Context, Table, UserData, UserRef, Value as LuaValue,
    Variadic,
};
use morf_io::{Bus, DbusProxy, DbusSignal};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use morf_system::{GreetdClient, GreetdConversation, StatusNotifierHost, UdevMonitor, XkbKeymap};

use crate::{lua_values::*, scene_bindings::*, serialization::*, state::*, table_menu::*};

/// How many subscriptions one configuration may hold.
///
/// Each is a match rule on the bus, and a stock system bus allows a connection
/// 512; this leaves room for everything else sharing the connection. A
/// configuration that reaches it is subscribing in a loop and never closing.
const MAX_DBUS_SUBSCRIPTIONS: usize = 384;

/// How many `call_async` answers one configuration may be waiting on.
const MAX_DBUS_REPLIES: usize = 64;

/// A call's arguments as passed from Lua: none, one (a list of them, or the
/// one argument), or several.
fn positional_from_lua<'gc>(
    ctx: Context<'gc>,
    mut values: Vec<LuaValue<'gc>>,
) -> Result<morf_io::DbusValue, String> {
    match values.len() {
        0 => Ok(morf_io::DbusValue::Nil),
        1 => lua_to_dbus(ctx, values.remove(0), 0),
        _ => values
            .into_iter()
            .map(|value| lua_to_dbus(ctx, value, 0))
            .collect::<Result<Vec<_>, _>>()
            .map(morf_io::DbusValue::List),
    }
}

/// Keeps a subscription for the runtime to poll, and names it for its handle.
fn register_subscription<'gc>(
    state: &RefCell<ReactiveState>,
    ctx: Context<'gc>,
    signal: DbusSignal,
    callback: Closure<'gc>,
    kind: DbusSignalKind,
) -> Result<u64, HostError> {
    let mut state = state.borrow_mut();
    if state.dbus_signals.len() >= MAX_DBUS_SUBSCRIPTIONS {
        return Err(HostError(format!(
            "D-Bus subscription limit reached ({MAX_DBUS_SUBSCRIPTIONS}); close the ones no longer needed"
        )));
    }
    state.next_dbus_signal_id += 1;
    let id = state.next_dbus_signal_id;
    state.dbus_signals.push(PendingDbusSignal {
        id,
        signal,
        callback: crate::vm::handler_store::register(ctx.stash(callback)),
        kind,
    });
    Ok(id)
}

/// `"session"` or `"system"`.
fn parse_bus(bus: &str) -> Result<Bus, HostError> {
    match bus {
        "session" => Ok(Bus::Session),
        "system" => Ok(Bus::System),
        _ => Err(HostError(format!("unknown D-Bus bus `{bus}`"))),
    }
}

pub(crate) fn install_system_service_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let dbus_get = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (proxy, property): (UserRef<DbusToken>, String) = stack.consume(ctx)?;
        let _span = crate::profile::span(|| format!("blocking D-Bus get {property}"));
        let value = proxy.proxy.get_value(&property).map_err(HostError)?;
        stack.replace(ctx, dbus_value_to_lua(ctx, value).map_err(HostError)?);
        Ok(CallbackReturn::Return)
    });
    // `proxy:call(method)` with no arguments, as it always was; given any, it
    // is `call_with` — arguments that used to be dropped without a word.
    let dbus_call = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (proxy, method, arguments): (UserRef<DbusToken>, String, Variadic<Vec<LuaValue>>) =
            stack.consume(ctx)?;
        let argument = positional_from_lua(ctx, arguments.0).map_err(HostError)?;
        let _span = crate::profile::span(|| format!("blocking D-Bus call {method}"));
        let value = proxy
            .proxy
            .call_value_with(&method, &argument)
            .map_err(HostError)?;
        stack.replace(ctx, dbus_value_to_lua(ctx, value).map_err(HostError)?);
        Ok(CallbackReturn::Return)
    });
    // `proxy:call_with(method, a, b, c)` sends three arguments;
    // `proxy:call_with(method, { a, b, c })` sends the same three, which is
    // the older spelling and stays. One value on its own is the one argument.
    let dbus_call_with = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (proxy, method, arguments): (UserRef<DbusToken>, String, Variadic<Vec<LuaValue>>) =
            stack.consume(ctx)?;
        let argument = positional_from_lua(ctx, arguments.0).map_err(HostError)?;
        let _span = crate::profile::span(|| format!("blocking D-Bus call {method}"));
        let value = proxy
            .proxy
            .call_value_with(&method, &argument)
            .map_err(HostError)?;
        stack.replace(ctx, dbus_value_to_lua(ctx, value).map_err(HostError)?);
        Ok(CallbackReturn::Return)
    });
    let dbus_set = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (proxy, property, value): (UserRef<DbusToken>, String, LuaValue) =
            stack.consume(ctx)?;
        let value = lua_to_dbus(ctx, value, 0).map_err(HostError)?;
        let _span = crate::profile::span(|| format!("blocking D-Bus set {property}"));
        proxy
            .proxy
            .set_value(&property, &value)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    // What `subscribe` and `on_name_owner_changed` hand back. Closing it
    // removes the route and the bus's match rule with it; letting it be
    // collected does not, because every subscription written before handles
    // existed drops the return value on the floor and expects to keep hearing.
    let close_state = Rc::clone(&state);
    let subscription_close = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let handle: UserRef<DbusSubscriptionToken> = stack.consume(ctx)?;
        let closed = {
            let mut state = close_state.borrow_mut();
            let before = state.dbus_signals.len();
            state.dbus_signals.retain(|entry| entry.id != handle.id);
            before != state.dbus_signals.len()
        };
        stack.replace(ctx, closed);
        Ok(CallbackReturn::Return)
    });
    let active_state = Rc::clone(&state);
    let subscription_active = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let handle: UserRef<DbusSubscriptionToken> = stack.consume(ctx)?;
        let active = active_state
            .borrow()
            .dbus_signals
            .iter()
            .any(|entry| entry.id == handle.id);
        stack.replace(ctx, active);
        Ok(CallbackReturn::Return)
    });
    let subscription_methods = Table::new(&ctx);
    subscription_methods.set_field(ctx, "close", subscription_close);
    subscription_methods.set_field(ctx, "unsubscribe", subscription_close);
    subscription_methods.set_field(ctx, "active", subscription_active);
    let subscription_metatable = Table::new(&ctx);
    subscription_metatable.set_field(ctx, "__index", subscription_methods);
    let subscription_metatable = ctx.stash(subscription_metatable);
    let subscribe_state = Rc::clone(&state);
    let subscribe_metatable = subscription_metatable.clone();
    let dbus_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (proxy, signal, callback): (UserRef<DbusToken>, String, Closure) =
            stack.consume(ctx)?;
        let signal = proxy
            .proxy
            .subscribe(signal)
            .map_err(|error| HostError(error.to_string()))?;
        let id = register_subscription(
            &subscribe_state,
            ctx,
            signal,
            callback,
            DbusSignalKind::Signal,
        )?;
        let handle = UserData::new_static(&ctx, DbusSubscriptionToken { id });
        handle.set_metatable(ctx, Some(ctx.fetch(&subscribe_metatable)));
        stack.replace(ctx, handle);
        Ok(CallbackReturn::Return)
    });
    // `proxy:call_async(method, args, callback)`: the call goes out now and
    // `callback(ok, reply_or_error)` runs on a later turn of the main loop.
    // Bounded by the proxy's own timeout, like `call`.
    let proxy_async_state = Rc::clone(&state);
    // The callback is the last argument; everything between it and the method
    // is the call's arguments, by the same rule as `call_with`.
    let dbus_call_async = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (proxy, method, mut arguments): (UserRef<DbusToken>, String, Variadic<Vec<LuaValue>>) =
            stack.consume(ctx)?;
        let Some(LuaValue::Function(luna::Function::Closure(callback))) = arguments.0.pop() else {
            return Err(HostError(
                "proxy:call_async wants (method, arguments..., callback)".into(),
            )
            .into());
        };
        let argument = positional_from_lua(ctx, arguments.0).map_err(HostError)?;
        let mut state = proxy_async_state.borrow_mut();
        if state.dbus_replies.len() >= MAX_DBUS_REPLIES {
            return Err(HostError(format!(
                "too many D-Bus calls waiting for an answer (the limit is {MAX_DBUS_REPLIES})"
            ))
            .into());
        }
        let reply = proxy
            .proxy
            .call_async(&method, argument)
            .map_err(HostError)?;
        state.dbus_replies.push(PendingDbusReply {
            reply,
            callback: crate::vm::handler_store::register(ctx.stash(callback)),
        });
        stack.replace(ctx, true);
        Ok(CallbackReturn::Return)
    });
    let dbus_introspect = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let proxy: UserRef<DbusToken> = stack.consume(ctx)?;
        let _span = crate::profile::span(|| "blocking D-Bus introspect".to_owned());
        let xml = proxy
            .proxy
            .introspect()
            .map_err(|error| HostError(error.to_string()))?;
        stack.replace(ctx, xml);
        Ok(CallbackReturn::Return)
    });
    let dbus_methods = Table::new(&ctx);
    dbus_methods.set_field(ctx, "get", dbus_get);
    dbus_methods.set_field(ctx, "call", dbus_call);
    dbus_methods.set_field(ctx, "call_with", dbus_call_with);
    dbus_methods.set_field(ctx, "set", dbus_set);
    dbus_methods.set_field(ctx, "subscribe", dbus_subscribe);
    dbus_methods.set_field(ctx, "call_async", dbus_call_async);
    dbus_methods.set_field(ctx, "introspect", dbus_introspect);
    let dbus_metatable = Table::new(&ctx);
    dbus_metatable.set_field(ctx, "__index", dbus_methods);
    let dbus_metatable = ctx.stash(dbus_metatable);
    let dbus_proxy = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (bus, destination, path, interface, timeout_ms): (
            String,
            String,
            String,
            String,
            Option<i64>,
        ) = stack.consume(ctx)?;
        let bus = parse_bus(&bus)?;
        let _span = crate::profile::span(|| format!("D-Bus connect {destination} {interface}"));
        // A second is right for reading a property and wrong for anything a
        // human is part of: BlueZ `Pair` does not return until the pairing
        // succeeds, fails, or times out well past it, and a caller with no way
        // to say so was left driving `bluetoothctl` instead.
        let proxy = match timeout_ms {
            Some(milliseconds) => {
                let milliseconds = u64::try_from(milliseconds).map_err(|_| {
                    HostError(format!("`{milliseconds}` is not a D-Bus call timeout"))
                })?;
                DbusProxy::connect_with_timeout(
                    bus,
                    destination,
                    path,
                    interface,
                    Duration::from_millis(milliseconds),
                )
            }
            None => DbusProxy::connect(bus, destination, path, interface),
        }
        .map_err(|error| HostError(error.to_string()))?;
        let userdata = UserData::new_static(&ctx, DbusToken { proxy });
        userdata.set_metatable(ctx, Some(ctx.fetch(&dbus_metatable)));
        stack.replace(ctx, userdata);
        Ok(CallbackReturn::Return)
    });
    // `morf.dbus.call_async(bus, destination, path, interface, method, args,
    // options, callback)`: a call from the process's shared connection, with
    // `options.timeout_ms` as its deadline (25 s when not given, at most
    // 120 s). `options` may be left out. The shared connection is what makes
    // this cheap — no proxy, no socket — and what makes a call that starts
    // something tied to its caller (a BlueZ discovery) last until it is
    // stopped from the same place.
    let module_async_state = Rc::clone(&state);
    let module_call_async = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (bus, destination, path, interface, method, argument, options, callback): (
            String,
            String,
            String,
            String,
            String,
            LuaValue,
            LuaValue,
            LuaValue,
        ) = stack.consume(ctx)?;
        let bus = parse_bus(&bus)?;
        // `options` is optional in the middle: a function in its place is the
        // callback.
        let (options, callback) = match (options, callback) {
            (LuaValue::Function(luna::Function::Closure(callback)), LuaValue::Nil) => {
                (None, callback)
            }
            (LuaValue::Nil, LuaValue::Function(luna::Function::Closure(callback))) => {
                (None, callback)
            }
            (LuaValue::Table(options), LuaValue::Function(luna::Function::Closure(callback))) => {
                (Some(options), callback)
            }
            _ => {
                return Err(HostError(
                    "morf.dbus.call_async wants (bus, destination, path, interface, method, args, [options], callback)".into(),
                )
                .into());
            }
        };
        let timeout = match options.map(|options| options.get_value(ctx, "timeout_ms")) {
            None | Some(LuaValue::Nil) => morf_io::DEFAULT_ASYNC_CALL_TIMEOUT,
            Some(LuaValue::Integer(milliseconds)) if milliseconds > 0 => {
                Duration::from_millis(milliseconds as u64)
            }
            Some(LuaValue::Number(milliseconds))
                if milliseconds.is_finite() && milliseconds > 0.0 =>
            {
                Duration::from_millis(milliseconds as u64)
            }
            Some(other) => {
                return Err(HostError(format!(
                    "`timeout_ms` must be a positive number of milliseconds, not {}",
                    other.type_name()
                ))
                .into());
            }
        };
        let argument = lua_to_dbus(ctx, argument, 0).map_err(HostError)?;
        let mut state = module_async_state.borrow_mut();
        if state.dbus_replies.len() >= MAX_DBUS_REPLIES {
            return Err(HostError(format!(
                "too many D-Bus calls waiting for an answer (the limit is {MAX_DBUS_REPLIES})"
            ))
            .into());
        }
        let reply = morf_io::call_async(
            bus,
            &destination,
            &path,
            &interface,
            &method,
            argument,
            timeout,
        )
        .map_err(HostError)?;
        state.dbus_replies.push(PendingDbusReply {
            reply,
            callback: crate::vm::handler_store::register(ctx.stash(callback)),
        });
        stack.replace(ctx, true);
        Ok(CallbackReturn::Return)
    });
    // Questions for the bus itself. None of them can activate a service,
    // which is what makes them the right way for a library to ask whether
    // one is running before it reads anything from it.
    let name_has_owner = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (bus, name): (String, String) = stack.consume(ctx)?;
        let owned = morf_io::name_has_owner(parse_bus(&bus)?, &name).map_err(HostError)?;
        stack.replace(ctx, owned);
        Ok(CallbackReturn::Return)
    });
    let name_owner = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (bus, name): (String, String) = stack.consume(ctx)?;
        let owner = morf_io::name_owner(parse_bus(&bus)?, &name).map_err(HostError)?;
        match owner {
            Some(owner) => stack.replace(ctx, owner),
            None => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    let list_names = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let bus: String = stack.consume(ctx)?;
        let names = morf_io::list_names(parse_bus(&bus)?).map_err(HostError)?;
        let table = Table::new(&ctx);
        for (index, name) in names.into_iter().enumerate() {
            table
                .set(ctx, index as i64 + 1, ctx.intern(name.as_bytes()))
                .map_err(|error| HostError(error.to_string()))?;
        }
        stack.replace(ctx, table);
        Ok(CallbackReturn::Return)
    });
    // `morf.dbus.on_name_owner_changed(bus, name, fn(old, new, name))`: one
    // name's comings and goings, filtered by the bus rather than in Lua.
    // `""` is nobody. Returns a handle, as `subscribe` does.
    let owner_state = Rc::clone(&state);
    let owner_metatable = subscription_metatable.clone();
    let on_name_owner_changed = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (bus, name, callback): (String, String, Closure) = stack.consume(ctx)?;
        let signal = morf_io::subscribe_name_owner_changed(parse_bus(&bus)?, &name)
            .map_err(|error| HostError(error.to_string()))?;
        let id = register_subscription(
            &owner_state,
            ctx,
            signal,
            callback,
            DbusSignalKind::OwnerChanged,
        )?;
        let handle = UserData::new_static(&ctx, DbusSubscriptionToken { id });
        handle.set_metatable(ctx, Some(ctx.fetch(&owner_metatable)));
        stack.replace(ctx, handle);
        Ok(CallbackReturn::Return)
    });
    let dbus = Table::new(&ctx);
    dbus.set_field(ctx, "proxy", dbus_proxy);
    dbus.set_field(ctx, "call_async", module_call_async);
    dbus.set_field(ctx, "name_has_owner", name_has_owner);
    dbus.set_field(ctx, "name_owner", name_owner);
    dbus.set_field(ctx, "list_names", list_names);
    dbus.set_field(ctx, "on_name_owner_changed", on_name_owner_changed);
    crate::api_dbus_serve::install_dbus_serve_api(ctx, Rc::clone(&state), dbus);
    morf.set_field(ctx, "dbus", dbus);

    let udev_state = Rc::clone(&state);
    let udev_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (subsystem, callback): (Option<String>, Closure) = stack.consume(ctx)?;
        let monitor = UdevMonitor::new(subsystem).map_err(|error| HostError(error.to_string()))?;
        udev_state.borrow_mut().udev_monitors.push(PendingUdev {
            monitor,
            callback: crate::vm::handler_store::register(ctx.stash(callback)),
        });
        Ok(CallbackReturn::Return)
    });
    let udev = Table::new(&ctx);
    udev.set_field(ctx, "subscribe", udev_subscribe);
    morf.set_field(ctx, "udev", udev);

    let status_notifier_state = Rc::clone(&state);
    let status_notifier_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        // `subscribe(handler)` uses the vendor-neutral watcher name;
        // `subscribe(handler, { "org.freedesktop", "..." })` names the ones this
        // session actually has. The engine ships no desktop environment's
        // prefix of its own — which watcher answers is a fact about the machine,
        // and the configuration is the thing that knows it.
        let (callback, watchers): (Closure, Option<Table>) = stack.consume(ctx)?;
        let names: Vec<String> = match watchers {
            Some(table) => (1..=table.length(&ctx))
                .filter_map(|index| match table.get_value(ctx, index) {
                    LuaValue::String(name) => Some(name.display_lossy().to_string()),
                    _ => None,
                })
                .collect(),
            None => StatusNotifierHost::DEFAULT_NAMESPACES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        };
        if names.is_empty() {
            return Err(HostError("status notifier needs at least one watcher name".into()).into());
        }
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        let host = StatusNotifierHost::connect_to(&borrowed)
            .map_err(|error| HostError(error.to_string()))?;
        let mut state = status_notifier_state.borrow_mut();
        if state.status_notifiers.len() >= 4 {
            return Err(HostError("status notifier subscription limit reached".into()).into());
        }
        state.status_notifiers.push(PendingStatusNotifier {
            host,
            callback: crate::vm::handler_store::register(ctx.stash(callback)),
        });
        Ok(CallbackReturn::Return)
    });
    let status_notifier = Table::new(&ctx);
    status_notifier.set_field(ctx, "subscribe", status_notifier_subscribe);
    morf.set_field(ctx, "status_notifier", status_notifier);

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

    crate::api_pam::install_pam_api(ctx, Rc::clone(&state), morf);

    let xkb_compile = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let options: Table = stack.consume(ctx)?;
        let rules = table_string(ctx, options, "rules", "").map_err(HostError)?;
        let model = table_string(ctx, options, "model", "pc105").map_err(HostError)?;
        let layout = table_string(ctx, options, "layout", "us").map_err(HostError)?;
        let variant = table_string(ctx, options, "variant", "").map_err(HostError)?;
        let xkb_options = match options.get_value(ctx, "options") {
            LuaValue::Nil => None,
            LuaValue::String(value) => Some(value.display_lossy().to_string()),
            _ => return Err(HostError("XKB options must be a string".into()).into()),
        };
        let keymap = XkbKeymap::compile(&rules, &model, &layout, &variant, xkb_options.as_deref())
            .map_err(|error| HostError(error.to_string()))?;
        stack.replace(ctx, xkb_keymap_to_lua(ctx, &keymap));
        Ok(CallbackReturn::Return)
    });
    let xkb = Table::new(&ctx);
    xkb.set_field(ctx, "compile", xkb_compile);
    morf.set_field(ctx, "xkb", xkb);
}
