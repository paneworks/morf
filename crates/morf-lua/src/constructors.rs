use luna::{Callback, CallbackReturn, Context, Function, Table, Value as LuaValue};
use morf_scene::{Element, VirtualList};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use crate::{configure::*, scene_bindings::*, state::*, table_menu::*, types::*, views::*};

pub(crate) fn element_constructor<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    limits: Limits,
    element: Element,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let mut properties: Table = stack.consume(ctx)?;
        // An image's `on_status` is the runtime's to call, not a property.
        let mut on_status = None;
        if element == Element::Image {
            on_status = optional_closure(ctx, properties, "on_status").map_err(HostError)?;
            let clean = Table::new(&ctx);
            for (key, value) in properties.iter(ctx) {
                if !matches!(key, LuaValue::String(name) if name.as_bytes() == b"on_status") {
                    clean.set(ctx, key, value)?;
                }
            }
            properties = clean;
        }
        let _span = crate::profile::span(|| format!("construct ui.{element:?}"));
        let node = create_node(&state, element);
        configure_element(&state, ctx, limits, node, properties).map_err(HostError)?;
        if element == Element::Image {
            let mut state = state.borrow_mut();
            state.images.register(node, on_status);
            // Until a paint has looked at it, a source is loading.
            if state
                .scene
                .string_value(node, "source")
                .is_ok_and(|source| !source.is_empty())
            {
                crate::scene_bindings::assign_scene_property(
                    &mut state,
                    node,
                    "status",
                    morf_scene::Value::String("loading".to_owned()),
                )
                .map_err(HostError)?;
            }
        }
        if element == Element::TextInput {
            crate::text_inputs::register(&mut state.borrow_mut(), node);
        }
        if element == Element::Inset
            && state
                .borrow()
                .scene
                .children(node)
                .map_err(|error| HostError(error.to_string()))?
                .len()
                > 1
        {
            return Err(HostError("Inset accepts at most one child".into()).into());
        }
        stack.replace(ctx, node_userdata(ctx, Rc::clone(&state), node));
        Ok(CallbackReturn::Return)
    })
}

pub(crate) fn loader_constructor<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    limits: Limits,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let properties: Table = stack.consume(ctx)?;
        let clean = Table::new(&ctx);
        let mut source = None;
        for (key, value) in properties.iter(ctx) {
            if matches!(key, LuaValue::String(name) if name.display_lossy().to_string() == "source")
            {
                let LuaValue::Function(Function::Closure(factory)) = value else {
                    return Err(HostError("Loader source must be a function".into()).into());
                };
                source = Some(ctx.stash(factory));
            } else {
                clean.set(ctx, key, value)?;
            }
        }
        let node = create_node(&state, Element::Loader);
        configure_element(&state, ctx, limits, node, clean).map_err(HostError)?;
        if let Some(source) = source.clone() {
            state.borrow_mut().loader_factories.insert(node, source);
        }
        if state
            .borrow()
            .scene
            .bool_value(node, "active")
            .map_err(|error| HostError(error.to_string()))?
            && let Some(source) = source
        {
            let child = execute_node_factory(ctx, &source, limits).map_err(HostError)?;
            state
                .borrow_mut()
                .scene
                .reparent(child, Some(node))
                .map_err(|error| HostError(error.to_string()))?;
            state.borrow_mut().loaded_loaders.insert(node);
        }
        stack.replace(ctx, node_userdata(ctx, Rc::clone(&state), node));
        Ok(CallbackReturn::Return)
    })
}

pub(crate) fn timer_constructor<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    limits: Limits,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, exec, mut stack| {
        let properties: Table = stack.consume(ctx)?;
        let clean = Table::new(&ctx);
        let mut callback = None;
        for (key, value) in properties.iter(ctx) {
            if matches!(key, LuaValue::String(name) if name.display_lossy().to_string() == "on_triggered")
            {
                let LuaValue::Function(Function::Closure(closure)) = value else {
                    return Err(HostError("Timer on_triggered must be a function".into()).into());
                };
                callback = Some(ctx.stash(closure));
            } else {
                clean.set(ctx, key, value)?;
            }
        }
        let node = create_node(&state, Element::Timer);
        configure_element(&state, ctx, limits, node, clean).map_err(HostError)?;
        let (interval, repeat, running) = {
            let state = state.borrow();
            let interval = state
                .scene
                .number(node, "interval")
                .map_err(|error| HostError(error.to_string()))?;
            let repeat = state
                .scene
                .bool_value(node, "repeat")
                .map_err(|error| HostError(error.to_string()))?;
            let running = state
                .scene
                .bool_value(node, "running")
                .map_err(|error| HostError(error.to_string()))?;
            (interval, repeat, running)
        };
        let origin: std::rc::Rc<str> = match exec.frame_at(0) {
            Some(frame) => format!(
                "ui.Timer at {}:{}",
                frame.chunk_name.display_lossy(),
                frame.current_line
            )
            .into(),
            None => format!("ui.Timer {node:?}").into(),
        };
        state
            .borrow_mut()
            .timer_origins
            .insert(node, std::rc::Rc::clone(&origin));
        if running {
            if !interval.is_finite() || interval <= 0.0 {
                return Err(HostError("Timer interval must be finite and positive".into()).into());
            }
            let callback =
                callback.ok_or_else(|| HostError("running Timer requires on_triggered".into()))?;
            let interval = Duration::from_secs_f64(interval / 1_000.0);
            let timer = state
                .borrow()
                .new_timer(interval)
                .map_err(|error| HostError(error.to_string()))?;
            let id = state.borrow_mut().next_timer_id();
            state.borrow_mut().timers.push(PendingTimer {
                id,
                timer,
                callback: callback.clone(),
                repeat,
                interval,
                node: Some(node),
                origin,
            });
            state.borrow_mut().timer_callbacks.insert(node, callback);
        } else if let Some(callback) = callback {
            state.borrow_mut().timer_callbacks.insert(node, callback);
        }
        stack.replace(ctx, node_userdata(ctx, Rc::clone(&state), node));
        Ok(CallbackReturn::Return)
    })
}

/// `ui.Terminal { command, cwd, env, scrollback, on_exit, on_title, on_bell,
/// on_clipboard, ... }`: the keys that say how to start the program are
/// taken off here; everything else is the node's own properties.
pub(crate) fn terminal_constructor<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    limits: Limits,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let properties: Table = stack.consume(ctx)?;
        const OWN: [&str; 9] = [
            "command",
            "cwd",
            "env",
            "scrollback",
            "on_exit",
            "on_title",
            "on_bell",
            "on_clipboard",
            "on_selection",
        ];
        let clean = Table::new(&ctx);
        for (key, value) in properties.iter(ctx) {
            let own = matches!(key, LuaValue::String(name)
                if OWN.contains(&name.display_lossy().to_string().as_str()));
            if !own {
                clean.set(ctx, key, value)?;
            }
        }
        let command = match properties.get_value(ctx, "command") {
            // The person's own shell, as any terminal opens with.
            LuaValue::Nil => vec![
                std::env::var("SHELL")
                    .ok()
                    .filter(|shell| !shell.is_empty())
                    .unwrap_or_else(|| "/bin/sh".to_owned()),
            ],
            LuaValue::Table(command) => table_string_array(ctx, command, 256).map_err(HostError)?,
            _ => {
                return Err(HostError(
                    "Terminal command must be an argv table, such as { \"btop\" }".into(),
                )
                .into());
            }
        };
        if command.is_empty() || command[0].is_empty() {
            return Err(HostError("Terminal command cannot be empty".into()).into());
        }
        let environment = match properties.get_value(ctx, "env") {
            LuaValue::Nil => Default::default(),
            LuaValue::Table(env) => table_string_map(ctx, env, 256).map_err(HostError)?,
            _ => return Err(HostError("Terminal env must be a table".into()).into()),
        };
        let working_directory = match properties.get_value(ctx, "cwd") {
            LuaValue::Nil => None,
            LuaValue::String(cwd) => {
                Some(std::path::PathBuf::from(cwd.display_lossy().to_string()))
            }
            _ => return Err(HostError("Terminal cwd must be a string".into()).into()),
        };
        let scrollback = match properties.get_value(ctx, "scrollback") {
            LuaValue::Nil => crate::terminals::DEFAULT_SCROLLBACK,
            LuaValue::Integer(lines) if lines >= 0 => lines as usize,
            LuaValue::Number(lines) if lines.is_finite() && lines >= 0.0 => lines as usize,
            _ => {
                return Err(
                    HostError("Terminal scrollback must be a number of lines".into()).into(),
                );
            }
        }
        .min(morf_terminal::MAX_SCROLLBACK);
        let callbacks = crate::terminals::TerminalCallbacks {
            on_exit: optional_closure(ctx, properties, "on_exit").map_err(HostError)?,
            on_title: optional_closure(ctx, properties, "on_title").map_err(HostError)?,
            on_bell: optional_closure(ctx, properties, "on_bell").map_err(HostError)?,
            on_clipboard: optional_closure(ctx, properties, "on_clipboard").map_err(HostError)?,
            on_selection: optional_closure(ctx, properties, "on_selection").map_err(HostError)?,
        };
        if state.borrow().terminals.len() >= limits.terminals {
            return Err(HostError(format!(
                "more than {} terminals: each is a program and a screen of its own \
                 (MORF_LIMITS terminals=N)",
                limits.terminals
            ))
            .into());
        }
        let node = create_node(&state, Element::Terminal);
        configure_element(&state, ctx, limits, node, clean).map_err(HostError)?;
        state.borrow_mut().terminals.register(
            node,
            crate::terminals::TerminalSpec {
                command,
                environment,
                working_directory,
                scrollback,
            },
            callbacks,
        );
        stack.replace(ctx, node_userdata(ctx, Rc::clone(&state), node));
        Ok(CallbackReturn::Return)
    })
}

pub(crate) fn view_constructor<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    limits: Limits,
    kind: ViewKind,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let properties: Table = stack.consume(ctx)?;
        let node = construct_view(ctx, &state, limits, kind, properties)?;
        stack.replace(ctx, node);
        Ok(CallbackReturn::Return)
    })
}

/// Builds a view node from its properties table; shared by `ui.Repeater`,
/// `ui.ListView`, `ui.GridView` and `ui.each`.
pub(crate) fn construct_view<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    limits: Limits,
    kind: ViewKind,
    properties: Table<'gc>,
) -> Result<LuaValue<'gc>, luna::Error<'gc>> {
    {
        let virtualized = !matches!(kind, ViewKind::Repeater);
        let model = match properties.get_value(ctx, "model") {
            LuaValue::UserData(model) => model
                .downcast_static::<ListModelToken>()
                .map_err(|_| HostError("view model must be a morf list model".to_owned()))?,
            _ => return Err(HostError("view model must be a morf list model".to_owned()).into()),
        };
        let delegate = match properties.get_value(ctx, "delegate") {
            LuaValue::Function(Function::Closure(delegate)) => ctx.stash(delegate),
            _ => return Err(HostError("view delegate must be a function".to_owned()).into()),
        };
        // A Repeater lays its delegates out as whatever it is asked to be:
        // `as = "column"` makes it a Column of them, `as = "grid"` a Grid
        // with the `columns` and spacings a Grid takes. Without it the
        // delegates keep their own positions, as before.
        let element = match kind {
            ViewKind::Repeater => match properties.get_value(ctx, "as") {
                LuaValue::Nil => Element::Item,
                LuaValue::String(name) => match name.display_lossy().to_string().as_str() {
                    "item" => Element::Item,
                    "row" => Element::Row,
                    "column" => Element::Column,
                    "grid" => Element::Grid,
                    "flex" => Element::Flex,
                    other => {
                        return Err(HostError(format!(
                            "Repeater `as` must be item, row, column, grid or flex, not `{other}`"
                        ))
                        .into());
                    }
                },
                _ => return Err(HostError("Repeater `as` must be a string".to_owned()).into()),
            },
            _ => Element::Item,
        };
        let repeater_keeps_columns = element == Element::Grid;
        let clean = Table::new(&ctx);
        for (key, value) in properties.iter(ctx) {
            let special = matches!(
                key,
                LuaValue::String(name)
                    if matches!(
                        name.display_lossy().to_string().as_str(),
                        "model"
                            | "delegate"
                            | "as"
                            | "item_extent"
                            | "overscan"
                            | "content_y"
                            | "cell_width"
                            | "cell_height"
                    ) || (name.display_lossy().to_string() == "columns" && !repeater_keeps_columns)
            );
            if !special {
                clean
                    .set(ctx, key, value)
                    .map_err(|error| HostError(error.to_string()))?;
            }
        }
        if virtualized {
            clean.set_field(ctx, "clip", true);
        }
        let node = create_node(state, element);
        configure_element(state, ctx, limits, node, clean).map_err(HostError)?;
        let model_handle = Rc::clone(&model.model);
        let model = model_handle.borrow();
        let configured_view;
        let (range, item_extent, offset, columns, column_extent) = match kind {
            ViewKind::Repeater => {
                configured_view = Some(VirtualList::new_unbounded());
                (0..model.len(), 0.0, 0.0, 1, 0.0)
            }
            ViewKind::List => {
                let item_extent =
                    table_number(ctx, properties, "item_extent", 1.0).map_err(HostError)?;
                let height = table_number(ctx, properties, "height", 0.0).map_err(HostError)?;
                let offset = table_number(ctx, properties, "content_y", 0.0).map_err(HostError)?;
                let overscan = table_number(ctx, properties, "overscan", 1.0).map_err(HostError)?;
                if item_extent <= 0.0 || height < 0.0 || offset < 0.0 || overscan < 0.0 {
                    return Err(HostError("invalid ListView dimensions".to_owned()).into());
                }
                let mut view = VirtualList::new(item_extent, height, overscan as usize)
                    .ok_or_else(|| HostError("invalid ListView dimensions".to_owned()))?;
                view.set_offset(offset);
                let range = view.visible_range(model.len());
                configured_view = Some(view);
                (range, item_extent, offset, 1, 0.0)
            }
            ViewKind::Grid => {
                let cell_width =
                    table_number(ctx, properties, "cell_width", 1.0).map_err(HostError)?;
                let cell_height =
                    table_number(ctx, properties, "cell_height", 1.0).map_err(HostError)?;
                let width = table_number(ctx, properties, "width", 0.0).map_err(HostError)?;
                let height = table_number(ctx, properties, "height", 0.0).map_err(HostError)?;
                let offset = table_number(ctx, properties, "content_y", 0.0).map_err(HostError)?;
                let overscan = table_number(ctx, properties, "overscan", 1.0).map_err(HostError)?;
                let default_columns = (width / cell_width).floor().max(1.0);
                let columns =
                    table_number(ctx, properties, "columns", default_columns).map_err(HostError)?;
                if cell_width <= 0.0
                    || cell_height <= 0.0
                    || width < 0.0
                    || height < 0.0
                    || offset < 0.0
                    || overscan < 0.0
                    || columns < 1.0
                    || columns.fract() != 0.0
                {
                    return Err(HostError("invalid GridView dimensions".to_owned()).into());
                }
                let columns = columns as usize;
                let mut view =
                    VirtualList::new_grid(cell_height, height, overscan as usize, columns)
                        .ok_or_else(|| HostError("invalid GridView dimensions".to_owned()))?;
                view.set_offset(offset);
                let range = view.visible_range(model.len());
                configured_view = Some(view);
                (range, cell_height, offset, columns, cell_width)
            }
        };
        let reuse_limit = range.len().max(1) * 2;
        let mut active = HashMap::new();
        for index in range {
            let (id, item) = model
                .get(index)
                .expect("view range contains live model indexes");
            let child = execute_delegate(ctx, &delegate, item, index, limits).map_err(HostError)?;
            if virtualized {
                position_view_child(
                    &mut state.borrow_mut().scene,
                    child.node,
                    index,
                    item_extent,
                    offset,
                    columns,
                    column_extent,
                )
                .map_err(HostError)?;
            }
            state
                .borrow_mut()
                .scene
                .reparent(child.node, Some(node))
                .map_err(|error| HostError(error.to_string()))?;
            active.insert(id, child);
        }
        drop(model);
        if let Some(mut view) = configured_view {
            let _ = view.sync(&model_handle.borrow(), &[]);
            state.borrow_mut().views.insert(
                node,
                LuaVirtualView {
                    model: model_handle,
                    view,
                    delegate,
                    active,
                    reusable: HashMap::new(),
                    reuse_order: VecDeque::new(),
                    reuse_limit,
                    pool_root: None,
                    exiting: Vec::new(),
                    column_extent,
                    positioned: virtualized,
                },
            );
        }
        Ok(LuaValue::UserData(node_userdata(
            ctx,
            Rc::clone(state),
            node,
        )))
    }
}
