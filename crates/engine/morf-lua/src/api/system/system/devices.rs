//! `morf.udev`, `morf.status_notifier` and `morf.xkb`: device events, the
//! tray's items, and keymaps.

use super::*;

/// Installs `morf.udev`.
pub(super) fn install_udev<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
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
}

/// Installs `morf.status_notifier`.
pub(super) fn install_status_notifier<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
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
}

/// Installs `morf.xkb`.
pub(super) fn install_xkb<'gc>(ctx: Context<'gc>, morf: Table<'gc>) {
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
