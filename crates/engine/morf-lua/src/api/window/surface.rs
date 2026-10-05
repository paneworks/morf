use luna::{Callback, CallbackReturn, Closure, Context, Table, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{layer_parse::*, scene_bindings::*, state::*, table_menu::*, types::*};

/// The callbacks `morf.surface` holds rather than layer settings.
fn surface_event(key: &str) -> Option<crate::window_events::WindowEvent> {
    use crate::window_events::WindowEvent;
    match key {
        "on_focus_changed" => Some(WindowEvent::FocusChanged),
        "on_pointer_changed" => Some(WindowEvent::PointerChanged),
        _ => None,
    }
}

pub(crate) fn install_shell_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let surface_read_state = Rc::clone(&state);
    let surface_index = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (_surface, key): (Table, String) = stack.consume(ctx)?;
        if let Some(event) = surface_event(&key) {
            let handler = surface_read_state
                .borrow()
                .windows
                .surface_handlers
                .get(&event)
                .cloned();
            match handler {
                Some(handler) => {
                    stack.replace(ctx, ctx.fetch(&crate::vm::handler_store::stashed(&handler)))
                }
                None => stack.replace(ctx, LuaValue::Nil),
            }
            return Ok(CallbackReturn::Return);
        }
        let value = layer_setting_to_lua(
            ctx,
            &surface_read_state.borrow().windows.layer_surface,
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
                    state.windows.surface_handlers.remove(&event);
                }
                LuaValue::Function(luna::Function::Closure(callback)) => {
                    state.windows.surface_handlers.insert(
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
        let changed = apply_layer_setting(ctx, &mut state.windows.layer_surface, &key, value)
            .map_err(HostError)?;
        state.windows.layer_surface_changed |= changed;
        Ok(CallbackReturn::Return)
    });
    let surface_metatable = Table::new(&ctx);
    surface_metatable.set_field(ctx, "__index", surface_index);
    surface_metatable.set_field(ctx, "__newindex", surface_new_index);
    let surface = Table::new(&ctx);
    surface.set_metatable(ctx, Some(surface_metatable));
    morf.set_field(ctx, "surface", surface);
    // `morf.density("compositor")`, `morf.density(1.25)` or
    // `morf.density({ ppi = 160 })`: how big one of morf's pixels is. Read
    // when the windows open, so a configuration says it while loading; nil
    // is the default, one device pixel to one of morf's.
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
            LuaValue::Table(table) => match number(table.get_value(ctx, "ppi")) {
                Some(ppi) if (40.0..=600.0).contains(&ppi) => Density::Ppi(ppi),
                _ => {
                    return Err(HostError(
                        "morf.density{ ppi = n } wants n between 40 and 600".into(),
                    )
                    .into());
                }
            },
            other => match number(other) {
                Some(scale) if (0.25..=10.0).contains(&scale) => Density::Scale(scale),
                _ => {
                    return Err(HostError(
                        "morf.density takes \"compositor\", \"ppi\", a scale or { ppi = n }".into(),
                    )
                    .into());
                }
            },
        };
        density_state.borrow_mut().density = parsed;
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
    morf.set_field(ctx, "density", density);
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
    morf.set_field(ctx, "font_families", font_families);
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
    morf.set_field(ctx, "font_axes", font_axes);
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
        let id = shell_storage_key(&shell_id_state.borrow().shell_root);
        stack.replace(ctx, id);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "shell_id", shell_id);
    let shell_dir_state = Rc::clone(&state);
    let shell_dir = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let root = shell_dir_state.borrow().shell_root.clone();
        stack.replace(ctx, root.to_string_lossy().as_ref());
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "shell_dir", shell_dir);
    let shell_root_state = Rc::clone(&state);
    let shell_path = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let relative: String = stack.consume(ctx)?;
        let path =
            rooted_path(&shell_root_state.borrow().shell_root, &relative).map_err(HostError)?;
        stack.replace(ctx, path.to_string_lossy().as_ref());
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "shell_path", shell_path);
    morf.set_field(ctx, "config_path", morf.get_value(ctx, "shell_path"));
    for (directory_name, path_name, kind) in [
        ("data_dir", "data_path", StorageKind::Data),
        ("state_dir", "state_path", StorageKind::State),
        ("cache_dir", "cache_path", StorageKind::Cache),
    ] {
        let directory_state = Rc::clone(&state);
        let directory = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let root =
                shell_storage_dir(&directory_state.borrow().shell_root, kind).map_err(HostError)?;
            stack.replace(ctx, root.to_string_lossy().as_ref());
            Ok(CallbackReturn::Return)
        });
        morf.set(ctx, directory_name, directory)
            .expect("core path directory accepts a native callback");
        let path_state = Rc::clone(&state);
        let path = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let relative: String = stack.consume(ctx)?;
            let root =
                shell_storage_dir(&path_state.borrow().shell_root, kind).map_err(HostError)?;
            let path = rooted_path(&root, &relative).map_err(HostError)?;
            stack.replace(ctx, path.to_string_lossy().as_ref());
            Ok(CallbackReturn::Return)
        });
        morf.set(ctx, path_name, path)
            .expect("core path resolver accepts a native callback");
    }
    let has_version = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (major, minor, features): (i64, i64, Option<Table>) = stack.consume(ctx)?;
        let current = env!("CARGO_PKG_VERSION")
            .split('.')
            .take(2)
            .map(|part| part.parse::<i64>().unwrap_or(0))
            .collect::<Vec<_>>();
        let available = current.first().copied().unwrap_or(0) > major
            || current.first().copied().unwrap_or(0) == major
                && current.get(1).copied().unwrap_or(0) >= minor;
        let features_available = features.is_none_or(|features| {
            table_string_array(ctx, features, 64).is_ok_and(|features| {
                features.iter().all(|feature| {
                    matches!(
                        feature.as_str(),
                        "wayland"
                            | "vulkan"
                            | "gles"
                            | "lua"
                            | "ipc"
                            | "session-lock"
                            | "screencopy"
                            | "virtual-keyboard"
                            | "input-method"
                            | "text-input"
                    )
                })
            })
        });
        stack.replace(ctx, available && features_available);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "has_version", has_version);
    let reload_state = Rc::clone(&state);
    let reload = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let hard: Option<bool> = stack.consume(ctx)?;
        let mut state = reload_state.borrow_mut();
        state.lifecycle.reload_request =
            Some(state.lifecycle.reload_request.unwrap_or(false) || hard.unwrap_or(false));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "reload", reload);
    let completed_state = Rc::clone(&state);
    let on_reload_completed = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        completed_state
            .borrow_mut()
            .lifecycle
            .reload_completed_callbacks
            .push(crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "on_reload_completed", on_reload_completed);
    let failed_state = Rc::clone(&state);
    let on_reload_failed = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        failed_state
            .borrow_mut()
            .lifecycle
            .reload_failed_callbacks
            .push(crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "on_reload_failed", on_reload_failed);
    let watch_state = Rc::clone(&state);
    let watch_files = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let value: Option<bool> = stack.consume(ctx)?;
        let mut state = watch_state.borrow_mut();
        if let Some(value) = value
            && state.lifecycle.watch_files != value
        {
            state.lifecycle.watch_files = value;
            state.lifecycle.watch_files_changed = true;
        }
        stack.replace(ctx, state.lifecycle.watch_files);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "watch_files", watch_files);
    let lock_state_state = Rc::clone(&state);
    let session_lock_state = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let name = crate::runtime_session_lock::current(&lock_state_state.borrow()).name();
        stack.replace(ctx, name);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "session_lock_state", session_lock_state);
    // A lock screen built per output: `fn(screen)` returns that output's
    // root, and bindings in it close over its own screen. Without one, the
    // file's single root is shared by every output.
    let builder_state = Rc::clone(&state);
    let lock_surface = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let builder: Option<Closure> = stack.consume(ctx)?;
        builder_state.borrow_mut().session.lock_surface_builder =
            builder.map(|builder| crate::vm::handler_store::register(ctx.stash(builder)));
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "lock_surface", lock_surface);
    // Two ways to hear the compositor about the lock: every change, with the
    // new state, or only the one a lock screen usually waits for -- the
    // compositor confirming that it has hidden the session.
    for (name, locked_only) in [
        ("on_session_lock_state", false),
        ("on_session_locked", true),
    ] {
        let callbacks_state = Rc::clone(&state);
        let register = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let callback: Closure = stack.consume(ctx)?;
            let mut state = callbacks_state.borrow_mut();
            if state.session.session_lock_callbacks.len() >= 64 {
                return Err(HostError("session lock callback limit reached".into()).into());
            }
            state.session.session_lock_callbacks.push((
                crate::vm::handler_store::register(ctx.stash(callback)),
                locked_only,
            ));
            Ok(CallbackReturn::Return)
        });
        morf.set_field(ctx, name, register);
    }
    let quit_state = Rc::clone(&state);
    // Asking to stop, rather than stopping. The call returns and the rest of
    // the handler runs; the shell goes down at the top of the next frame, once
    // the supervisor has seen the request and taken every output down with it.
    // Exiting from inside a Lua callback would unwind the runtime that is
    // running the callback.
    let quit = Callback::from_fn(&ctx, move |_, _, _| {
        quit_state.borrow_mut().lifecycle.quit_requested = true;
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "quit", quit);
    let working_directory = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let path: Option<String> = stack.consume(ctx)?;
        if let Some(path) = path {
            if path.is_empty() || path.len() > 4_096 || path.as_bytes().contains(&0) {
                return Err(HostError("working directory path is invalid".into()).into());
            }
            std::env::set_current_dir(&path).map_err(|error| HostError(error.to_string()))?;
        }
        let current = std::env::current_dir().map_err(|error| HostError(error.to_string()))?;
        stack.replace(ctx, current.to_string_lossy().as_ref());
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "working_directory", working_directory);
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
