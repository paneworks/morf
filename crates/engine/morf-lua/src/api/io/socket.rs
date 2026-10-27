use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use morf_io::{ServerView, SocketView};
use std::cell::RefCell;

use std::rc::Rc;

use crate::{api::ui::scene::*, runtime::state::*, value::lua::*, vm::permissions::gated};

pub(crate) fn install_socket_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let socket_send = gated(ctx, &state, "socket", "socket:send", |ctx, _, mut stack| {
        let (socket, bytes): (UserRef<SocketToken>, String) = stack.consume(ctx)?;
        socket
            .state
            .borrow_mut()
            .send(bytes.as_bytes())
            .map_err(HostError::from)?;
        Ok(CallbackReturn::Return)
    });
    let socket_flush = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let socket: UserRef<SocketToken> = stack.consume(ctx)?;
        socket.state.borrow_mut().flush().map_err(HostError::from)?;
        Ok(CallbackReturn::Return)
    });
    let socket_connected = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let socket: UserRef<SocketToken> = stack.consume(ctx)?;
        stack.replace(ctx, socket.state.borrow().connected());
        Ok(CallbackReturn::Return)
    });
    let socket_set_connected = gated(
        ctx,
        &state,
        "socket",
        "socket:set_connected",
        |ctx, _, mut stack| {
            let (socket, connected): (UserRef<SocketToken>, bool) = stack.consume(ctx)?;
            let connected = socket
                .state
                .borrow_mut()
                .set_connected(connected)
                .map_err(HostError::from)?;
            stack.replace(ctx, connected);
            Ok(CallbackReturn::Return)
        },
    );
    let socket_close = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let socket: UserRef<SocketToken> = stack.consume(ctx)?;
        socket.state.borrow_mut().close();
        Ok(CallbackReturn::Return)
    });
    let socket_path = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let socket: UserRef<SocketToken> = stack.consume(ctx)?;
        stack.replace(ctx, socket.state.borrow().path());
        Ok(CallbackReturn::Return)
    });
    let socket_set_path = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (socket, path): (UserRef<SocketToken>, String) = stack.consume(ctx)?;
        let changed = socket.state.borrow_mut().set_path(path);
        stack.replace(ctx, changed);
        Ok(CallbackReturn::Return)
    });
    let socket_receive = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (socket, maximum, timeout_ms): (UserRef<SocketToken>, i64, i64) = stack.consume(ctx)?;
        let maximum = morf_io::receive_limit(maximum).map_err(HostError::from)?;
        let timeout = bounded_timeout(timeout_ms).map_err(HostError::from)?;
        let received = socket
            .state
            .borrow_mut()
            .receive(maximum, timeout)
            .map_err(HostError::from)?;
        match received {
            Some(bytes) => stack.replace(ctx, String::from_utf8_lossy(&bytes).as_ref()),
            None => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    let socket_methods = Table::new(&ctx);
    socket_methods.set_field(ctx, "send", socket_send);
    socket_methods.set_field(ctx, "flush", socket_flush);
    socket_methods.set_field(ctx, "receive", socket_receive);
    socket_methods.set_field(ctx, "connected", socket_connected);
    socket_methods.set_field(ctx, "set_connected", socket_set_connected);
    socket_methods.set_field(ctx, "close", socket_close);
    socket_methods.set_field(ctx, "path", socket_path);
    socket_methods.set_field(ctx, "set_path", socket_set_path);
    let socket_metatable = Table::new(&ctx);
    socket_metatable.set_field(ctx, "__index", socket_methods);
    let socket_metatable = ctx.stash(socket_metatable);
    let accepted_socket_metatable = socket_metatable.clone();
    let server_accept = gated(
        ctx,
        &state,
        "socket",
        "socket_server:accept",
        move |ctx, _, mut stack| {
            let server: UserRef<SocketServerToken> = stack.consume(ctx)?;
            let Some(socket) = server.state.borrow().accept().map_err(HostError::from)? else {
                stack.replace(ctx, LuaValue::Nil);
                return Ok(CallbackReturn::Return);
            };
            let userdata = UserData::new_static(
                &ctx,
                SocketToken {
                    state: RefCell::new(socket),
                },
            );
            userdata.set_metatable(ctx, Some(ctx.fetch(&accepted_socket_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        },
    );
    let server_active = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let server: UserRef<SocketServerToken> = stack.consume(ctx)?;
        stack.replace(ctx, server.state.borrow().active());
        Ok(CallbackReturn::Return)
    });
    let server_set_active = gated(
        ctx,
        &state,
        "socket",
        "socket_server:set_active",
        |ctx, _, mut stack| {
            let (server, active): (UserRef<SocketServerToken>, bool) = stack.consume(ctx)?;
            let active = server
                .state
                .borrow_mut()
                .set_active(active)
                .map_err(HostError::from)?;
            stack.replace(ctx, active);
            Ok(CallbackReturn::Return)
        },
    );
    let server_close = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let server: UserRef<SocketServerToken> = stack.consume(ctx)?;
        server.state.borrow_mut().close();
        Ok(CallbackReturn::Return)
    });
    let server_path = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let server: UserRef<SocketServerToken> = stack.consume(ctx)?;
        stack.replace(ctx, server.state.borrow().path());
        Ok(CallbackReturn::Return)
    });
    let server_set_path = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (server, path): (UserRef<SocketServerToken>, String) = stack.consume(ctx)?;
        let changed = server.state.borrow_mut().set_path(path);
        stack.replace(ctx, changed);
        Ok(CallbackReturn::Return)
    });
    let server_methods = Table::new(&ctx);
    server_methods.set_field(ctx, "accept", server_accept);
    server_methods.set_field(ctx, "active", server_active);
    server_methods.set_field(ctx, "set_active", server_set_active);
    server_methods.set_field(ctx, "close", server_close);
    server_methods.set_field(ctx, "path", server_path);
    server_methods.set_field(ctx, "set_path", server_set_path);
    let server_metatable = Table::new(&ctx);
    server_metatable.set_field(ctx, "__index", server_methods);
    let server_metatable = ctx.stash(server_metatable);
    let socket_server = gated(
        ctx,
        &state,
        "socket",
        "morf.socket.server",
        move |ctx, _, mut stack| {
            let path: String = stack.consume(ctx)?;
            let server = ServerView::bind(path).map_err(HostError::from)?;
            let userdata = UserData::new_static(
                &ctx,
                SocketServerToken {
                    state: RefCell::new(server),
                },
            );
            userdata.set_metatable(ctx, Some(ctx.fetch(&server_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        },
    );
    crate::api::namespace::table(ctx, morf, "socket").set_field(ctx, "server", socket_server);
    let socket = gated(
        ctx,
        &state,
        "socket",
        "morf.socket.open",
        move |ctx, _, mut stack| {
            let path: String = stack.consume(ctx)?;
            let socket = SocketView::connect(path).map_err(HostError::from)?;
            let userdata = UserData::new_static(
                &ctx,
                SocketToken {
                    state: RefCell::new(socket),
                },
            );
            userdata.set_metatable(ctx, Some(ctx.fetch(&socket_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        },
    );
    crate::api::namespace::table(ctx, morf, "socket").set_field(ctx, "open", socket);
}
