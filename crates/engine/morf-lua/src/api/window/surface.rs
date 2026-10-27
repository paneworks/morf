use luna::{Callback, CallbackReturn, Closure, Context, Table, Value as LuaValue};
use std::rc::Rc;

use crate::{
    api::ui::scene::*, api::window::layer::*, runtime::state::*, value::table::*, vm::types::*,
};

/// The callbacks `morf.surface` holds rather than layer settings.
fn surface_event(key: &str) -> Option<crate::api::window::events::WindowEvent> {
    use crate::api::window::events::WindowEvent;
    match key {
        "on_focus_changed" => Some(WindowEvent::FocusChanged),
        "on_pointer_changed" => Some(WindowEvent::PointerChanged),
        _ => None,
    }
}

pub(crate) fn install_shell_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let surface_read_state = Rc::clone(&state);
    let surface_index = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (_surface, key): (Table, String) = stack.consume(ctx)?;
        if let Some(event) = surface_event(&key) {
            let handler = surface_read_state
                .borrow()
                .windows()
                .surface_handlers
                .get(&event)
                .cloned();
            match handler {
                Some(handler) => stack.replace(
                    ctx,
                    ctx.fetch(&crate::vm::handler_store::stashed(&handler).map_err(HostError)?),
                ),
                None => stack.replace(ctx, LuaValue::Nil),
            }
            return Ok(CallbackReturn::Return);
        }
        let value = layer_setting_to_lua(
            ctx,
            &surface_read_state.borrow().windows().layer_surface,
            &key,
        );
        stack.replace(ctx, value);
        Ok(CallbackReturn::Return)
    });
    let surface_write_state = Rc::clone(&state);
    let surface_new_index = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (_surface, key, value): (Table, String, LuaValue) = stack.consume(ctx)?;
        let mut state = surface_write_state.borrow_mut();
        if let Some(event) = surface_event(&key) {
            match value {
                LuaValue::Nil => {
                    state.windows_mut().surface_handlers.remove(&event);
                }
                LuaValue::Function(luna::Function::Closure(callback)) => {
                    state.windows_mut().surface_handlers.insert(
                        event,
                        crate::vm::handler_store::register(ctx.stash(callback)),
                    );
                }
                _ => {
                    return Err(HostError(format!("morf.surface.{key} must be a function")).into());
                }
            }
            return Ok(CallbackReturn::Return);
        }
        let changed = apply_layer_setting(ctx, &mut state.windows_mut().layer_surface, &key, value)
            .map_err(HostError)?;
        state.windows_mut().layer_surface_changed |= changed;
        Ok(CallbackReturn::Return)
    });
    let surface_metatable = Table::new(&ctx);
    surface_metatable.set_field(ctx, "__index", surface_index);
    surface_metatable.set_field(ctx, "__newindex", surface_new_index);
    let surface = Table::new(&ctx);
    surface.set_metatable(ctx, Some(surface_metatable));
    morf.set_field(ctx, "surface", surface);
    // `morf.window.density("compositor")`, `morf.window.density(1.25)`,
    // `morf.window.density({ ppi = 160 })` or `morf.window.density({ zoom = 0.8 })` (the
    // compositor's scale times 0.8): how big one of morf's pixels is, at any
    // time (a scale slider); nil is the default, the compositor's.
    let density_state = Rc::clone(&state);
    let density = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        use morf_value::density::{Density, REFERENCE_PPI};
        let value: LuaValue = stack.consume(ctx)?;
        let number = |value: LuaValue| match value {
            LuaValue::Integer(n) => Some(n as f64),
            LuaValue::Number(n) => Some(n),
            _ => None,
        };
        let parsed = match value {
            LuaValue::Nil => Density::default(),
            LuaValue::String(name) if name.as_bytes() == b"compositor" => Density::Compositor,
            LuaValue::String(name) if name.as_bytes() == b"ppi" => Density::Ppi(REFERENCE_PPI),
            LuaValue::Table(table) => {
                if let Some(zoom) = number(table.get_value(ctx, "zoom")) {
                    if !(0.25..=4.0).contains(&zoom) {
                        return Err(HostError(
                            "morf.window.density{ zoom = f } wants f between 0.25 and 4".into(),
                        )
                        .into());
                    }
                    Density::Relative(zoom)
                } else {
                    match number(table.get_value(ctx, "ppi")) {
                        Some(ppi) if (40.0..=600.0).contains(&ppi) => Density::Ppi(ppi),
                        _ => {
                            return Err(HostError(
                                "morf.window.density takes { ppi = n } (40 to 600) or { zoom = f } (0.25 to 4)".into(),
                            )
                            .into());
                        }
                    }
                }
            }
            other => match number(other) {
                Some(scale) if (0.25..=10.0).contains(&scale) => Density::Scale(scale),
                _ => {
                    return Err(HostError(
                        "morf.window.density takes \"compositor\", \"ppi\", a scale or { ppi = n }"
                            .into(),
                    )
                    .into());
                }
            },
        };
        {
            let mut state = density_state.borrow_mut();
            if state.display_mut().set_density(parsed) {
                // What reads the screens' size reads it again.
                state.touch_screens();
            }
        }
        // The screens already described are described again in the new unit.
        if let Ok(morf) = ctx.get_global::<Table>("morf")
            && let LuaValue::Table(screens) = morf.get_value(ctx, "screens")
        {
            let mut index = 1;
            while let LuaValue::Table(entry) = screens.get_value(ctx, index) {
                crate::api::system::host::apply_density(ctx, entry, parsed);
                index += 1;
            }
        }
        Ok(CallbackReturn::Return)
    });
    crate::api::namespace::table(ctx, morf, "window").set_field(ctx, "density", density);
    let env = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let name: String = stack.consume(ctx)?;
        if name.is_empty() || name.len() > 256 || name.as_bytes().contains(&0) {
            return Err(HostError("environment variable name is invalid".into()).into());
        }
        match std::env::var_os(name) {
            Some(value) => stack.replace(ctx, value.to_string_lossy().as_ref()),
            None => stack.replace(ctx, LuaValue::Nil),
        }
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "env", env);
    // What faces this machine has, so a configuration that offers a choice of
    // font can offer the real ones rather than a list of names guessed by
    // whoever wrote it. A call rather than a table: working the answer out
    // means scanning the font directories, and most configurations never ask.
    let font_families = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let names = morf_text::installed_families();
        let table = Table::new(&ctx);
        for (index, name) in names.iter().enumerate() {
            table.set(ctx, index as i64 + 1, name.as_str())?;
        }
        stack.replace(ctx, table);
        Ok(CallbackReturn::Return)
    });
    crate::api::namespace::table(ctx, morf, "text").set_field(ctx, "families", font_families);
    // The variation axes an installed family defines, `{ tag, min, default,
    // max }` each: what a text node's `axes` can move, and how far.
    let font_axes = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let family: String = stack.consume(ctx)?;
        let table = Table::new(&ctx);
        for (index, axis) in morf_text::family_axes(&family).iter().enumerate() {
            let entry = Table::new(&ctx);
            entry.set(ctx, "tag", axis.name().as_str())?;
            entry.set(ctx, "min", f64::from(axis.min))?;
            entry.set(ctx, "default", f64::from(axis.default))?;
            entry.set(ctx, "max", f64::from(axis.max))?;
            table.set(ctx, index as i64 + 1, entry)?;
        }
        stack.replace(ctx, table);
        Ok(CallbackReturn::Return)
    });
    crate::api::namespace::table(ctx, morf, "text").set_field(ctx, "axes", font_axes);
    // What this configuration was started with. Three views of the same words:
    // `args` is what was typed, in order and unaltered; `options` is the flags
    // resolved into names and values; `operands` is what was left over. A
    // configuration that wants to read the line itself has the first, and one
    // that wants an answer has the other two.
    set_argument_fields(ctx, morf, crate::arguments::given());
    morf.set_field(ctx, "process_id", i64::from(std::process::id()));
    // The binary that is running, so a configuration can start another of
    // itself. `"morf"` only works when morf is on `PATH`, which it is not when
    // it is being run out of a build directory — and a greeter that cannot open
    // its on-screen keyboard because of that is a machine nobody can log into.
    if let Ok(executable) = std::env::current_exe() {
        morf.set_field(ctx, "executable", executable.to_string_lossy().as_ref());
    }
    morf.set_field(ctx, "version", env!("CARGO_PKG_VERSION"));
    let launched = launch_time_ms();
    morf.set_field(
        ctx,
        "launch_time_ms",
        i64::try_from(launched).unwrap_or(i64::MAX),
    );
    morf.set_field(
        ctx,
        "instance_id",
        format!("{}-{launched}", std::process::id()),
    );
    morf.set_field(
        ctx,
        "app_id",
        std::env::var("MORF_APP_ID").unwrap_or_else(|_| "morf".to_owned()),
    );
    let shell_id_state = Rc::clone(&state);
    let shell_id = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let id = shell_storage_key(shell_id_state.borrow().shell_root());
        stack.replace(ctx, id);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "shell_id", shell_id);
    // `morf.paths.shell(rel)`, `cache(rel)`, `state(rel)`: a path under the
    // shell's directory or its own cache or state directory; with no `rel`,
    // the directory itself.
    let paths = crate::api::namespace::table(ctx, morf, "paths");
    for (name, kind) in [
        ("shell", None),
        ("state", Some(StorageKind::State)),
        ("cache", Some(StorageKind::Cache)),
    ] {
        let path_state = Rc::clone(&state);
        let path = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let relative: Option<String> = stack.consume(ctx)?;
            let shell_root = path_state.borrow().shell_root().to_path_buf();
            let root = match kind {
                Some(kind) => shell_storage_dir(&shell_root, kind).map_err(HostError)?,
                None => shell_root,
            };
            let path = match relative {
                Some(relative) => rooted_path(&root, &relative).map_err(HostError)?,
                None => root,
            };
            stack.replace(ctx, path.to_string_lossy().as_ref());
            Ok(CallbackReturn::Return)
        });
        paths.set_field(ctx, name, path);
    }
    // `morf.satisfies(">=0.4")`: whether this engine meets a requirement
    // written as a shell's morf.lua writes one.
    let satisfies = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let requirement: String = stack.consume(ctx)?;
        let met = crate::vm::requirement::satisfies(&requirement, crate::vm::requirement::ENGINE)
            .map_err(HostError)?;
        stack.replace(ctx, met);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "satisfies", satisfies);
    let reload_state = Rc::clone(&state);
    // `morf.reload(hard)`, or `morf.reload { hard = ..., output = ... }`:
    // `output`, only this output's runtime.
    let reload = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let asked = match stack.consume::<LuaValue>(ctx)? {
            LuaValue::Table(options) => {
                let field = |name| table_bool(ctx, options, name, false);
                ReloadRequest {
                    hard: field("hard").map_err(HostError)?,
                    output: field("output").map_err(HostError)?,
                }
            }
            LuaValue::Nil => ReloadRequest::default(),
            LuaValue::Boolean(hard) => ReloadRequest {
                hard,
                output: false,
            },
            _ => return Err(HostError("morf.reload takes a boolean or a table".into()).into()),
        };
        reload_state.borrow_mut().lifecycle_mut().ask_reload(asked);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "reload", reload);
    // `morf.on_reload(fn(ok, err))`: told of every reload, `ok` false and
    // `err` the error when it failed; a handler that returns true takes the
    // failure, and the engine's band is not shown.
    let reload_state = Rc::clone(&state);
    let on_reload = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        reload_state
            .borrow_mut()
            .lifecycle_mut()
            .on_reload(crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "on_reload", on_reload);
    let lock = crate::api::namespace::table(ctx, morf, "lock");
    // A lock screen built per output: `fn(screen)` returns that output's
    // root, and bindings in it close over its own screen. Without one, the
    // file's single root is shared by every output.
    let builder_state = Rc::clone(&state);
    let lock_surface = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let builder: Option<Closure> = stack.consume(ctx)?;
        builder_state
            .borrow_mut()
            .session_mut()
            .lock_surface_builder =
            builder.map(|builder| crate::vm::handler_store::register(ctx.stash(builder)));
        Ok(CallbackReturn::Return)
    });
    lock.set_field(ctx, "surface", lock_surface);
    // Two ways to hear the compositor about the lock: every change, with the
    // new state, or only the one a lock screen usually waits for -- the
    // compositor confirming that it has hidden the session.
    for (name, locked_only) in [("on_state", false), ("on_locked", true)] {
        let callbacks_state = Rc::clone(&state);
        let register = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let callback: Closure = stack.consume(ctx)?;
            let mut state = callbacks_state.borrow_mut();
            if state.session().session_lock_callbacks.len() >= 64 {
                return Err(HostError("session lock callback limit reached".into()).into());
            }
            state.session_mut().session_lock_callbacks.push((
                crate::vm::handler_store::register(ctx.stash(callback)),
                locked_only,
            ));
            Ok(CallbackReturn::Return)
        });
        lock.set_field(ctx, name, register);
    }
    let quit_state = Rc::clone(&state);
    // Asking to stop, rather than stopping. The call returns and the rest of
    // the handler runs; the shell goes down at the top of the next frame, once
    // the supervisor has seen the request and taken every output down with it.
    // Exiting from inside a Lua callback would unwind the runtime that is
    // running the callback.
    let quit = Callback::from_fn(&ctx, move |_, _, _| {
        quit_state.borrow_mut().lifecycle_mut().quit();
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "quit", quit);
}

/// Writes `morf.args`, `morf.options` and `morf.operands` from one command
/// line: the process's at startup, or the one a test runner loads a
/// configuration with (`Runtime::set_arguments`).
pub(crate) fn set_argument_fields<'gc>(
    ctx: Context<'gc>,
    morf: Table<'gc>,
    given: &crate::arguments::Arguments,
) {
    let list_of = |items: &[String]| {
        let table = Table::new(&ctx);
        for (index, word) in items.iter().enumerate() {
            table
                .set(ctx, index as i64 + 1, word.as_str())
                .expect("a table accepts integer keys");
        }
        table
    };
    let words = list_of(given.words());
    morf.set_field(ctx, "args", words);
    let options = Table::new(&ctx);
    for (name, values) in given.options() {
        // One value is that value; several are a list, because a repeated
        // option keeps what it was given and only the configuration knows
        // whether the first, the last or all of them was meant.
        if let [only] = values.as_slice() {
            let key = ctx.intern(name.as_bytes());
            let _ = match only.text() {
                Some(text) => options.set(ctx, key, text),
                None => options.set(ctx, key, true),
            };
            continue;
        }
        let list = Table::new(&ctx);
        for (index, value) in values.iter().enumerate() {
            let slot = index as i64 + 1;
            match value.text() {
                Some(text) => list.set(ctx, slot, text),
                None => list.set(ctx, slot, true),
            }
            .expect("a table accepts integer keys");
        }
        let _ = options.set(ctx, ctx.intern(name.as_bytes()), list);
    }
    morf.set_field(ctx, "options", options);
    morf.set_field(ctx, "operands", list_of(given.operands()));
}
