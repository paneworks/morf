//! Owning a bus name from a configuration.
//!
//! The client half of D-Bus has been reachable from Lua for a long time:
//! `morf.dbus.proxy` calls anything on the bus. This is the other half, and it
//! is the difference between reading the session and being part of it. A
//! notification server, a tray watcher, a polkit agent, an MPRIS player — every
//! one of those is a name plus a handful of methods, and none of them can be
//! written while a configuration can only make calls.
//!
//! The engine already had all of it. `morf_io::DbusService` owns a name, hands
//! out arriving calls and answers them; it was written, tested, and reachable
//! from nowhere. What was missing was this file.

use luna::{
    Callback, CallbackReturn, Closure, Context, Table, UserData, UserRef, Value as LuaValue,
};
use morf_io::{DbusService, bus_named, call_id, remember_name};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    scene_bindings::HostError,
    serialization::{dbus_value_to_lua, lua_to_dbus},
    state::*,
};

/// Installs `morf.dbus.serve` and the methods on what it returns.
///
/// `dbus` is the table `morf.dbus`, so this adds to the client API rather than
/// standing beside it: one table, both halves.
pub(crate) fn install_dbus_serve_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    dbus: Table<'gc>,
) {
    let service_name = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let service: UserRef<DbusServiceToken> = stack.consume(ctx)?;
        let name = service.service.borrow().name().to_owned();
        stack.replace(ctx, name);
        Ok(CallbackReturn::Return)
    });
    // Replying is a separate call rather than the handler's return value,
    // because a method need not be answered on the turn it arrives. A
    // configuration that has to read a file or wait for a user before it can
    // answer holds the id and replies later; the caller waits, which is what it
    // was going to do anyway.
    let service_reply = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (service, id, value): (UserRef<DbusServiceToken>, i64, LuaValue) =
            stack.consume(ctx)?;
        let value = lua_to_dbus(ctx, value, 0).map_err(HostError)?;
        service
            .service
            .borrow_mut()
            .reply(call_id(id).map_err(HostError)?, &value)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let service_reply_error = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (service, id, name, message): (UserRef<DbusServiceToken>, i64, String, String) =
            stack.consume(ctx)?;
        service
            .service
            .borrow_mut()
            .reply_error(call_id(id).map_err(HostError)?, &name, &message)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let service_emit = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (service, path, interface, member, value): (
            UserRef<DbusServiceToken>,
            String,
            String,
            String,
            LuaValue,
        ) = stack.consume(ctx)?;
        let value = lua_to_dbus(ctx, value, 0).map_err(HostError)?;
        service
            .service
            .borrow()
            .emit(&path, &interface, &member, &value)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    // `service:call(destination, path, interface, member, args)`: a call
    // made from the service's own connection, for the services that remember
    // who registered with them and call that name back.
    let service_call = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (service, destination, path, interface, member, value): (
            UserRef<DbusServiceToken>,
            String,
            String,
            String,
            String,
            LuaValue,
        ) = stack.consume(ctx)?;
        let value = lua_to_dbus(ctx, value, 0).map_err(HostError)?;
        let reply = service
            .service
            .borrow()
            .call(&destination, &path, &interface, &member, &value)
            .map_err(HostError)?;
        let reply = dbus_value_to_lua(ctx, reply).map_err(HostError)?;
        stack.replace(ctx, reply);
        Ok(CallbackReturn::Return)
    });
    let on_call_state = Rc::clone(&state);
    let service_on_call = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (service, callback): (UserRef<DbusServiceToken>, Closure) = stack.consume(ctx)?;
        let mut state = on_call_state.borrow_mut();
        // One handler per service; a second replaces the first.
        let callback = crate::vm::handler_store::register(ctx.stash(callback));
        state
            .dbus_services
            .set(&service.service, callback)
            .map_err(HostError)?;
        Ok(CallbackReturn::Return)
    });
    let close_state = Rc::clone(&state);
    let service_close = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let service: UserRef<DbusServiceToken> = stack.consume(ctx)?;
        service.service.borrow_mut().release();
        // Dropping the registration is what releases the name: the entry here
        // and the token hold the only two references, and `DbusService::drop`
        // hands the name to whoever is queued behind us.
        close_state
            .borrow_mut()
            .dbus_services
            .remove(&service.service);
        Ok(CallbackReturn::Return)
    });

    let service_methods = Table::new(&ctx);
    service_methods.set_field(ctx, "name", service_name);
    service_methods.set_field(ctx, "on_call", service_on_call);
    service_methods.set_field(ctx, "reply", service_reply);
    service_methods.set_field(ctx, "reply_error", service_reply_error);
    service_methods.set_field(ctx, "emit", service_emit);
    service_methods.set_field(ctx, "call", service_call);
    service_methods.set_field(ctx, "close", service_close);
    let service_metatable = Table::new(&ctx);
    service_metatable.set_field(ctx, "__index", service_methods);
    let service_metatable = ctx.stash(service_metatable);

    let serve_state = Rc::clone(&state);
    let serve = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (bus, name, path, replace): (String, String, String, Option<bool>) =
            stack.consume(ctx)?;
        let bus = bus_named(&bus).map_err(HostError)?;
        // Replacing by default. A shell that cannot be restarted without the
        // user first killing whatever holds its name is a shell nobody
        // restarts, and every one of these names is held by a shell.
        let (service, outcome) = DbusService::own(bus, &name, &path, replace.unwrap_or(true))
            .map_err(|error| HostError(error.to_string()))?;
        let service = Rc::new(RefCell::new(service));
        // Remembered weakly: the runtime gives every name back when it ends
        // (`Runtime::release_bus_names`), not whenever the collector gets to
        // the handle -- which is what makes a handover of the primary
        // runtime's duties clean.
        remember_name(&mut serve_state.borrow_mut().owned_bus_names, &service);
        let userdata = UserData::new_static(&ctx, DbusServiceToken { service });
        userdata.set_metatable(ctx, Some(ctx.fetch(&service_metatable)));
        // Two values, and the second is the one that matters. Taking a name is
        // allowed to fail without being an error — somebody else runs the
        // notification server — and a configuration that ignores this reads as
        // working right up until nothing is ever sent to it.
        stack.replace(ctx, (userdata, outcome.name()));
        Ok(CallbackReturn::Return)
    });
    dbus.set_field(ctx, "serve", serve);
}
