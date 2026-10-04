//! `morf.core`, `morf.io` and `morf.window`: the native modules a
//! configuration requires, and the window constructors.

use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    LogLevel, layer_parse::*, process_helpers::*, runtime_helpers::*, scene_bindings::*, state::*,
    surface_types::*, table_menu::*, window_geometry::*, window_methods::*, window_parse::*,
};

mod window;

pub(crate) fn install_module_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: crate::Limits,
) -> (Table<'gc>, Table<'gc>, Table<'gc>) {
    let core = Table::new(&ctx);
    for name in [
        "env",
        "font_families",
        "process_id",
        "executable",
        "args",
        "options",
        "operands",
        "version",
        "instance_id",
        "shell_id",
        "app_id",
        "launch_time_ms",
        "shell_dir",
        "shell_path",
        "config_path",
        "data_dir",
        "data_path",
        "state_dir",
        "state_path",
        "cache_dir",
        "cache_path",
        "has_version",
        "reload",
        "on_reload_completed",
        "on_reload_failed",
        "watch_files",
        "working_directory",
        "elapsed_timer",
        "system_clock",
        "easing_curve",
        "color_quantizer",
        "geometry",
        "icon_path",
        "has_icon",
        "exec_detached",
        "signal",
        "theme",
        "prefers",
        "reloadable",
        "persistent",
        "scope",
        "retainable",
        "retain_lock",
        "transform_watcher",
        "effect",
        "clock",
        "timer",
        "screens",
        "primary",
        "on_primary",
        "variants",
        "list_model",
        "virtual_list",
        "sync_view",
        "flickable",
        "transition_parent",
        "desktop_entries",
        "session_paths",
        "menu",
    ] {
        core.set(ctx, name, morf.get_value(ctx, name))
            .expect("core module accepts native fields");
    }
    let io = Table::new(&ctx);
    for name in [
        "process",
        "process_view",
        "file",
        "file_view",
        "socket_server",
        "socket",
        "line_parser",
        "split_parser",
        "stream_collector",
        "json",
        "spawn",
        "run",
        "kill",
        "connect",
        "request_socket",
    ] {
        io.set(ctx, name, morf.get_value(ctx, name))
            .expect("IO module accepts native fields");
    }
    let region = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let options: Table = stack.consume(ctx)?;
        let region = parse_region(ctx, options, 0).map_err(HostError)?;
        stack.replace(ctx, region_to_lua(ctx, &region));
        Ok(CallbackReturn::Return)
    });
    let window_metatable = window::window_metatable(ctx, &state, limits);
    let popup_surface = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let window_metatable = window_metatable.clone();
        move |ctx, _, mut stack| {
            let options: Table = stack.consume(ctx)?;
            let updates_enabled =
                table_bool(ctx, options, "updates_enabled", true).map_err(HostError)?;
            let (root, visible, config, node_anchor) =
                parse_popup_surface(ctx, options).map_err(HostError)?;
            {
                let state = state.borrow();
                state
                    .scene
                    .element(root)
                    .map_err(|error| HostError(error.to_string()))?;
                if let Some(parent) = config.parent {
                    let parent = state
                        .windows
                        .window_surfaces
                        .get(&parent)
                        .ok_or_else(|| HostError("popup parent is stale".into()))?;
                    if !matches!(parent.kind, WindowSurfaceKind::Toplevel(_)) {
                        return Err(HostError("popup parent must be a toplevel".into()).into());
                    }
                    if let Some(anchor) = &node_anchor
                        && !scene_node_in_subtree(&state.scene, parent.root, anchor.node)
                    {
                        return Err(HostError(
                            "popup anchor node must belong to its parent surface".into(),
                        )
                        .into());
                    }
                }
            }
            let id = {
                let mut state = state.borrow_mut();
                let id = register_window_surface(
                    &mut state,
                    root,
                    visible,
                    updates_enabled,
                    WindowSurfaceKind::Popup(config),
                );
                if let Some(anchor) = node_anchor {
                    state
                        .scene
                        .element(anchor.node)
                        .map_err(|error| HostError(error.to_string()))?;
                    state.windows.popup_node_anchors.insert(id, anchor);
                }
                crate::window_events::register_window_size(&mut state, id);
                crate::window_events::window_handlers_from_options(ctx, &mut state, id, options)?;
                id
            };
            let userdata = UserData::new_static(&ctx, WindowSurfaceToken { id });
            userdata.set_metatable(ctx, Some(ctx.fetch(&window_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });
    let floating_surface = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let window_metatable = window_metatable.clone();
        move |ctx, _, mut stack| {
            let options: Table = stack.consume(ctx)?;
            let updates_enabled =
                table_bool(ctx, options, "updates_enabled", true).map_err(HostError)?;
            let (root, visible, config) =
                parse_floating_surface(ctx, options).map_err(HostError)?;
            {
                let state = state.borrow();
                state
                    .scene
                    .element(root)
                    .map_err(|error| HostError(error.to_string()))?;
                if let Some(parent) = config.parent {
                    let parent = state
                        .windows
                        .window_surfaces
                        .get(&parent)
                        .ok_or_else(|| HostError("floating parent is stale".into()))?;
                    if !matches!(parent.kind, WindowSurfaceKind::Toplevel(_)) {
                        return Err(HostError("toplevel parent must be a toplevel".into()).into());
                    }
                }
            }
            let id = {
                let mut state = state.borrow_mut();
                let id = register_window_surface(
                    &mut state,
                    root,
                    visible,
                    updates_enabled,
                    WindowSurfaceKind::Toplevel(config),
                );
                crate::window_events::register_window_size(&mut state, id);
                crate::window_events::window_handlers_from_options(ctx, &mut state, id, options)?;
                id
            };
            let userdata = UserData::new_static(&ctx, WindowSurfaceToken { id });
            userdata.set_metatable(ctx, Some(ctx.fetch(&window_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });
    let layer_surface = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let window_metatable = window_metatable.clone();
        move |ctx, _, mut stack| {
            let options: Table = stack.consume(ctx)?;
            let updates_enabled =
                table_bool(ctx, options, "updates_enabled", true).map_err(HostError)?;
            let (root, visible, config) = parse_layer_surface(ctx, options).map_err(HostError)?;
            {
                let state = state.borrow();
                state
                    .scene
                    .element(root)
                    .map_err(|error| HostError(error.to_string()))?;
            }
            let id = {
                let mut state = state.borrow_mut();
                let id = register_window_surface(
                    &mut state,
                    root,
                    visible,
                    updates_enabled,
                    WindowSurfaceKind::Layer(config),
                );
                crate::window_events::window_handlers_from_options(ctx, &mut state, id, options)?;
                id
            };
            let userdata = UserData::new_static(&ctx, WindowSurfaceToken { id });
            userdata.set_metatable(ctx, Some(ctx.fetch(&window_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });
    let window = Table::new(&ctx);
    window.set_field(ctx, "layer_surface", morf.get_value(ctx, "surface"));
    window.set_field(ctx, "region", region);
    window.set_field(ctx, "popup", popup_surface);
    window.set_field(ctx, "toplevel", floating_surface);
    // The old name, kept for one release: the compositor, not morf, decides
    // whether a toplevel floats.
    window.set_field(ctx, "floating", floating_surface);
    window.set_field(ctx, "layer", layer_surface);
    morf.set_field(ctx, "core", core);
    morf.set_field(ctx, "io", io);
    morf.set_field(ctx, "window", window);
    (core, io, window)
}
