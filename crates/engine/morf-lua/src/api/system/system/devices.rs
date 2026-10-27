//! `morf.udev`, `morf.status_notifier` and `morf.xkb`: device events, the
//! tray's items, and keymaps.

use super::*;

/// Installs `morf.udev`.
pub(super) fn install_udev<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let udev_state = Rc::clone(&state);
    let udev_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (subsystem, callback): (Option<String>, Closure) = stack.consume(ctx)?;
        let monitor = UdevMonitor::new(subsystem).map_err(|error| HostError(error.to_string()))?;
        let callback = crate::vm::handler_store::register(ctx.stash(callback));
        udev_state
            .borrow_mut()
            .feeds_mut()
            .watch_devices(monitor, callback);
        Ok(CallbackReturn::Return)
    });
    let udev = Table::new(&ctx);
    udev.set_field(ctx, "subscribe", udev_subscribe);
    morf.set_field(ctx, "udev", udev);
}

/// Installs `morf.status_notifier`.
pub(super) fn install_status_notifier<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let status_notifier_state = Rc::clone(&state);
    let status_notifier_subscribe = gated(
        ctx,
        &state,
        "dbus",
        "morf.status_notifier.subscribe",
        move |ctx, _, mut stack| {
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
                return Err(
                    HostError("status notifier needs at least one watcher name".into()).into(),
                );
            }
            let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
            let host = StatusNotifierHost::connect_to(&borrowed)
                .map_err(|error| HostError(error.to_string()))?;
            let mut state = status_notifier_state.borrow_mut();
            if state.feeds().trays_full() {
                return Err(HostError("status notifier subscription limit reached".into()).into());
            }
            let callback = crate::vm::handler_store::register(ctx.stash(callback));
            state.feeds_mut().watch_tray(host, callback);
            Ok(CallbackReturn::Return)
        },
    );
    let status_notifier = Table::new(&ctx);
    status_notifier.set_field(ctx, "subscribe", status_notifier_subscribe);
    morf.set_field(ctx, "status_notifier", status_notifier);
}

/// Installs `morf.xkb`.
pub(super) fn install_xkb<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
    let xkb_include_paths = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let paths = Table::new(&ctx);
        for (index, path) in morf_system::xkb_include_paths().into_iter().enumerate() {
            paths.set(ctx, index as i64 + 1, ctx.intern(path.as_bytes()))?;
        }
        stack.replace(ctx, paths);
        Ok(CallbackReturn::Return)
    });
    let xkb = Table::new(&ctx);
    xkb.set_field(ctx, "include_paths", xkb_include_paths);
    morf.set_field(ctx, "xkb", xkb);
}
