//! The `ui.*` constructors for elements, Loaders, Timers, Terminals and
//! views.

use luna::{Callback, CallbackReturn, Context, Function, Table, Value as LuaValue};
use morf_runtime::timers::Timer;
use morf_scene::{Element, VirtualList};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use crate::{configure::*, scene_bindings::*, state::*, table_menu::*, types::*, views::*};

mod views;

pub(crate) use views::{construct_view, view_constructor};

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
        if element == Element::Inset && {
            let state = state.borrow();
            let children = state
                .scene
                .children(node)
                .map_err(|error| HostError(error.to_string()))?;
            // A mask is not the child an Inset insets.
            children
                .iter()
                .filter(|child| !state.scene.is_mask(**child))
                .count()
                > 1
        } {
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
                source = Some(crate::vm::handler_store::register(ctx.stash(factory)));
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
            let source = state
                .borrow()
                .timers
                .source(interval)
                .map_err(|error| HostError(error.to_string()))?;
            let handler = crate::vm::handler_store::register(callback);
            let mut state = state.borrow_mut();
            let id = state.timers.next_id();
            state.timers.add(Timer {
                id,
                source,
                handler: handler.clone(),
                repeat,
                interval,
                node: Some(node),
                origin,
            });
            state.timer_callbacks.insert(node, handler);
        } else if let Some(callback) = callback {
            state
                .borrow_mut()
                .timer_callbacks
                .insert(node, crate::vm::handler_store::register(callback));
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
