//! The window handle's metatable: its methods -- open, close, destroy, the
//! toplevel and popup settings, system moves and resizes, event handlers --
//! and, on a layer surface, the layer settings read and written through it.

use super::*;

/// The metatable every window handle shares.
pub(super) fn window_metatable<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    limits: crate::Limits,
) -> luna::StashedTable {
    let window_visible = Callback::from_fn(&ctx, {
        let state = Rc::clone(state);
        move |ctx, _, mut stack| {
            let surface: UserRef<WindowSurfaceToken> = stack.consume(ctx)?;
            let visible = state
                .borrow()
                .windows
                .window_surfaces
                .get(&surface.id)
                .map(|surface| surface.visible)
                .ok_or_else(|| HostError("window destroyed".into()))?;
            stack.replace(ctx, visible);
            Ok(CallbackReturn::Return)
        }
    });
    let window_open = Callback::from_fn(&ctx, {
        let state = Rc::clone(state);
        move |ctx, _, mut stack| {
            let surface: UserRef<WindowSurfaceToken> = stack.consume(ctx)?;
            let mut state = state.borrow_mut();
            let surface = state
                .windows
                .window_surfaces
                .get_mut(&surface.id)
                .ok_or_else(|| HostError("window destroyed".into()))?;
            if !surface.visible {
                surface.visible = true;
                state.windows.window_surfaces_changed = true;
            }
            Ok(CallbackReturn::Return)
        }
    });
    let window_close = Callback::from_fn(&ctx, {
        let state = Rc::clone(state);
        move |ctx, _, mut stack| {
            let surface: UserRef<WindowSurfaceToken> = stack.consume(ctx)?;
            let mut state = state.borrow_mut();
            let surface = state
                .windows
                .window_surfaces
                .get_mut(&surface.id)
                .ok_or_else(|| HostError("window destroyed".into()))?;
            if surface.visible {
                surface.visible = false;
                state.windows.window_surfaces_changed = true;
            }
            Ok(CallbackReturn::Return)
        }
    });
    // `win:destroy()`: the surface goes, its root and everything under it
    // are removed the way `ui.destroy` removes a node, and every later call
    // on the handle fails with "window destroyed". A window on screen hears
    // `on_closed` once, after it is gone; one already closed has heard it.
    // Destroying twice is nothing. Child windows parented to it are not
    // destroyed with it: with their parent gone they are never shown.
    let window_destroy = Callback::from_fn(&ctx, {
        let state = Rc::clone(state);
        move |ctx, _, mut stack| {
            let surface: UserRef<WindowSurfaceToken> = stack.consume(ctx)?;
            let on_closed =
                crate::window_events::destroy_window_surface(&mut state.borrow_mut(), surface.id);
            crate::reactive_bindings::run_destroyed_hooks(&state, ctx, limits);
            if let Some(callback) = on_closed {
                state.borrow_mut().handler_depth += 1;
                let result =
                    crate::reactive_execute::execute_handler_args(ctx, &callback, &[], limits);
                let mut guard = state.borrow_mut();
                guard.handler_depth = guard.handler_depth.saturating_sub(1);
                if let Err(message) = result {
                    guard.log(
                        LogLevel::Warn,
                        format!("window {} on_closed: {message}", surface.id),
                    );
                }
                let flush = guard.handler_depth == 0 && std::mem::take(&mut guard.flush_pending);
                drop(guard);
                if flush {
                    let _ = crate::reactive_bindings::flush_reactive(&state, ctx, limits);
                }
            }
            Ok(CallbackReturn::Return)
        }
    });
    let window_kind = Callback::from_fn(&ctx, {
        let state = Rc::clone(state);
        move |ctx, _, mut stack| {
            let surface: UserRef<WindowSurfaceToken> = stack.consume(ctx)?;
            let kind = state
                .borrow()
                .windows
                .window_surfaces
                .get(&surface.id)
                .map(|surface| match surface.kind {
                    WindowSurfaceKind::Popup(_) => "popup",
                    WindowSurfaceKind::Toplevel(_) => "toplevel",
                    WindowSurfaceKind::Layer(_) => "layer",
                })
                .ok_or_else(|| HostError("window destroyed".into()))?;
            stack.replace(ctx, kind);
            Ok(CallbackReturn::Return)
        }
    });
    let window_methods = Table::new(&ctx);
    window_methods.set_field(ctx, "visible", window_visible);
    // One spelling. `set_visible(node, bool)` said exactly what these two say
    // and wrote the same field; a configuration should not have to know which
    // of two names the engine prefers.
    window_methods.set_field(ctx, "open", window_open);
    window_methods.set_field(ctx, "close", window_close);
    window_methods.set_field(ctx, "kind", window_kind);
    window_methods.set_field(ctx, "destroy", window_destroy);
    window_methods.set_field(
        ctx,
        "updates_enabled",
        window_updates_enabled_method(ctx, Rc::clone(state)),
    );
    for property in ["minimized", "maximized", "fullscreen"] {
        window_methods.set_field(
            ctx,
            property,
            floating_state_method(ctx, Rc::clone(state), property),
        );
    }
    for property in ["title", "app_id"] {
        window_methods.set_field(
            ctx,
            property,
            floating_string_method(ctx, Rc::clone(state), property),
        );
    }
    window_methods.set_field(ctx, "size", window_size_method(ctx, Rc::clone(state)));
    for property in ["minimum_size", "maximum_size"] {
        window_methods.set_field(
            ctx,
            property,
            floating_size_method(ctx, Rc::clone(state), property),
        );
    }
    window_methods.set_field(
        ctx,
        "grab_focus",
        popup_bool_method(ctx, Rc::clone(state), "grab_focus"),
    );
    for property in ["anchor_edge", "gravity"] {
        window_methods.set_field(
            ctx,
            property,
            popup_string_method(ctx, Rc::clone(state), property),
        );
    }
    window_methods.set_field(
        ctx,
        "anchor_rect",
        popup_anchor_rect_method(ctx, Rc::clone(state)),
    );
    window_methods.set_field(ctx, "offset", popup_offset_method(ctx, Rc::clone(state)));
    window_methods.set_field(
        ctx,
        "constraints",
        popup_constraints_method(ctx, Rc::clone(state)),
    );
    window_methods.set_field(
        ctx,
        "parent_id",
        window_parent_id_method(ctx, Rc::clone(state)),
    );
    window_methods.set_field(
        ctx,
        "set_parent",
        window_set_parent_method(ctx, Rc::clone(state)),
    );
    window_methods.set_field(
        ctx,
        "item_position",
        window_item_position_method(ctx, Rc::clone(state)),
    );
    window_methods.set_field(
        ctx,
        "item_rect",
        window_item_rect_method(ctx, Rc::clone(state)),
    );
    window_methods.set_field(
        ctx,
        "map_from_item",
        window_map_from_item_method(ctx, Rc::clone(state)),
    );
    window_methods.set_field(
        ctx,
        "map_rect_from_item",
        window_map_rect_from_item_method(ctx, Rc::clone(state)),
    );
    let move_state = Rc::clone(state);
    let start_system_move = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let surface: UserRef<WindowSurfaceToken> = stack.consume(ctx)?;
        let mut state = move_state.borrow_mut();
        let valid = state
            .windows
            .window_surfaces
            .get(&surface.id)
            .is_some_and(|surface| {
                surface.visible && matches!(surface.kind, WindowSurfaceKind::Toplevel(_))
            });
        if valid {
            state
                .windows
                .window_surface_actions
                .push(WindowSurfaceAction::Move { id: surface.id });
        }
        stack.replace(ctx, valid);
        Ok(CallbackReturn::Return)
    });
    window_methods.set_field(ctx, "start_system_move", start_system_move);
    let resize_state = Rc::clone(state);
    let start_system_resize = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (surface, edge): (UserRef<WindowSurfaceToken>, String) = stack.consume(ctx)?;
        if !matches!(
            edge.as_str(),
            "top"
                | "bottom"
                | "left"
                | "right"
                | "top_left"
                | "top_right"
                | "bottom_left"
                | "bottom_right"
        ) {
            return Err(HostError("invalid floating resize edge".into()).into());
        }
        let mut state = resize_state.borrow_mut();
        let valid = state
            .windows
            .window_surfaces
            .get(&surface.id)
            .is_some_and(|surface| {
                surface.visible && matches!(surface.kind, WindowSurfaceKind::Toplevel(_))
            });
        if valid {
            state
                .windows
                .window_surface_actions
                .push(WindowSurfaceAction::Resize {
                    id: surface.id,
                    edge,
                });
        }
        stack.replace(ctx, valid);
        Ok(CallbackReturn::Return)
    });
    window_methods.set_field(ctx, "start_system_resize", start_system_resize);
    window_methods.set_field(
        ctx,
        "configure",
        window_configure_method(ctx, Rc::clone(state)),
    );
    for event in crate::window_events::WindowEvent::ALL {
        window_methods.set_field(
            ctx,
            event.method(),
            crate::window_events::window_handler_method(ctx, Rc::clone(state), event),
        );
    }
    // Methods first; on a layer surface any other name reads that layer
    // setting, and assigning one changes it on the live surface, exactly as
    // `morf.surface.<key>` does for the shell's own.
    let window_method_table = ctx.stash(window_methods);
    let window_index = Callback::from_fn(&ctx, {
        let state = Rc::clone(state);
        move |ctx, _, mut stack| {
            let (surface, key): (UserRef<WindowSurfaceToken>, LuaValue) = stack.consume(ctx)?;
            let method = ctx.fetch(&window_method_table).get_value(ctx, key);
            if !method.is_nil() {
                stack.replace(ctx, method);
                return Ok(CallbackReturn::Return);
            }
            let LuaValue::String(key) = key else {
                stack.replace(ctx, LuaValue::Nil);
                return Ok(CallbackReturn::Return);
            };
            let key = key.display_lossy().to_string();
            let mut state = state.borrow_mut();
            // A popup's or floating window's `width`/`height` is the size the
            // compositor configured it to, tracked by the binding reading it.
            if let Some(value) =
                crate::window_events::window_size_field(&mut state, surface.id, &key)
            {
                stack.replace(ctx, value);
                return Ok(CallbackReturn::Return);
            }
            let value = match state
                .windows
                .window_surfaces
                .get(&surface.id)
                .map(|s| &s.kind)
            {
                Some(WindowSurfaceKind::Layer(config)) => layer_setting_to_lua(ctx, config, &key),
                _ => LuaValue::Nil,
            };
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let window_new_index = Callback::from_fn(&ctx, {
        let state = Rc::clone(state);
        move |ctx, _, mut stack| {
            let (surface, key, value): (UserRef<WindowSurfaceToken>, String, LuaValue) =
                stack.consume(ctx)?;
            set_window_layer_setting(ctx, &mut state.borrow_mut(), surface.id, &key, value)?;
            Ok(CallbackReturn::Return)
        }
    });
    let window_metatable = Table::new(&ctx);
    window_metatable.set_field(ctx, "__index", window_index);
    window_metatable.set_field(ctx, "__newindex", window_new_index);
    ctx.stash(window_metatable)
}
