//! `morf.window`: the window constructors.

use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::rc::Rc;

use crate::{
    LogLevel, api::io::process_helpers::*, api::ui::scene::*, api::window::geometry::*,
    api::window::layer::*, api::window::methods::*, api::window::parse::*, runtime::helpers::*,
    runtime::host_types::*, runtime::state::*, value::table::*,
};

mod window;

pub(crate) fn install_module_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: crate::Limits,
) {
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
                    .scene()
                    .element(root)
                    .map_err(|error| HostError(error.to_string()))?;
                if let Some(parent) = config.parent {
                    let parent = state
                        .windows()
                        .window_surfaces
                        .get(&parent)
                        .ok_or_else(|| HostError("popup parent is stale".into()))?;
                    if !matches!(parent.kind, WindowSurfaceKind::Toplevel(_)) {
                        return Err(HostError("popup parent must be a toplevel".into()).into());
                    }
                    if let Some(anchor) = &node_anchor
                        && !scene_node_in_subtree(state.scene(), parent.root, anchor.node)
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
                        .scene()
                        .element(anchor.node)
                        .map_err(|error| HostError(error.to_string()))?;
                    state.windows_mut().popup_node_anchors.insert(id, anchor);
                }
                crate::api::window::events::register_window_size(&mut state, id);
                crate::api::window::events::window_handlers_from_options(
                    ctx, &mut state, id, options,
                )?;
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
                    .scene()
                    .element(root)
                    .map_err(|error| HostError(error.to_string()))?;
                if let Some(parent) = config.parent {
                    let parent = state
                        .windows()
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
                crate::api::window::events::register_window_size(&mut state, id);
                crate::api::window::events::window_handlers_from_options(
                    ctx, &mut state, id, options,
                )?;
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
                    .scene()
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
                crate::api::window::events::window_handlers_from_options(
                    ctx, &mut state, id, options,
                )?;
                id
            };
            let userdata = UserData::new_static(&ctx, WindowSurfaceToken { id });
            userdata.set_metatable(ctx, Some(ctx.fetch(&window_metatable)));
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });
    let window = crate::api::namespace::table(ctx, morf, "window");
    window.set_field(ctx, "region", region);
    window.set_field(ctx, "popup", popup_surface);
    window.set_field(ctx, "toplevel", floating_surface);
    window.set_field(ctx, "layer", layer_surface);
}
