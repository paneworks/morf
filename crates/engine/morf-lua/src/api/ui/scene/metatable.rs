//! The metatable every node handle shares: properties read and written
//! through it, its methods, and the properties the runtime keeps for itself.

use super::*;

pub(crate) fn node_metatable<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
) -> Table<'gc> {
    let read_state = Rc::clone(&state);
    let index = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, key): (UserRef<NodeToken>, String) = stack.consume(ctx)?;
        let element = read_state
            .borrow()
            .scene
            .element(node.handle)
            .map_err(|error| HostError(error.to_string()))?;
        if key == "item" && element == Element::Loader {
            let child = read_state
                .borrow()
                .scene
                .children(node.handle)
                .map_err(|error| HostError(error.to_string()))?
                .first()
                .copied();
            match child {
                Some(child) => {
                    stack.replace(ctx, node_userdata(ctx, Rc::clone(&read_state), child))
                }
                None => stack.replace(ctx, LuaValue::Nil),
            }
            return Ok(CallbackReturn::Return);
        }
        if element == Element::TextInput
            && let Some(method) = text_input_method(ctx, &read_state, &key)
        {
            stack.replace(ctx, method);
            return Ok(CallbackReturn::Return);
        }
        if element == Element::Terminal
            && let Some(method) = terminal_method(ctx, &read_state, &key)
        {
            stack.replace(ctx, method);
            return Ok(CallbackReturn::Return);
        }
        if key == "track" && element == Element::SdfShape {
            let target = read_state.borrow().scene.track(node.handle);
            match target {
                Some(target) => {
                    stack.replace(ctx, node_userdata(ctx, Rc::clone(&read_state), target))
                }
                None => stack.replace(ctx, LuaValue::Nil),
            }
            return Ok(CallbackReturn::Return);
        }
        if key == "mask" {
            let mask = read_state.borrow().scene.mask(node.handle);
            if let Some(mask) = mask {
                stack.replace(ctx, node_userdata(ctx, Rc::clone(&read_state), mask));
                return Ok(CallbackReturn::Return);
            }
        }
        // Whether the pointer is inside the node's box, whatever is drawn
        // over it. The host answers it for the nodes that have been asked
        // about, each time the pointer moves; asking enrols the node.
        if key == CONTAINS_POINTER {
            let mut state = read_state.borrow_mut();
            if let Some(active) = &mut state.active {
                active
                    .property_reads
                    .insert((node.handle, CONTAINS_POINTER.to_owned(), false));
            }
            let value = state.pointer_watch.read(node.handle);
            stack.replace(ctx, LuaValue::Boolean(value));
            return Ok(CallbackReturn::Return);
        }
        if key == "stretch" {
            let stretch = read_state.borrow().scene.stretch(node.handle);
            let value = match stretch {
                Some(stretch) => SceneValue::Map(
                    [
                        ("stiffness", stretch.stiffness),
                        ("damping", stretch.damping),
                        ("scale", stretch.scale),
                        ("max", stretch.max),
                    ]
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), SceneValue::Number(value)))
                    .collect(),
                ),
                None => SceneValue::Nil,
            };
            stack.replace(ctx, scene_to_lua(ctx, &value).map_err(HostError)?);
            return Ok(CallbackReturn::Return);
        }
        let key = if key == "active_async" && element == Element::Loader {
            "active".to_owned()
        } else {
            key
        };
        // The direction the node lays out in, inherited (`layout_direction`
        // is its own setting, "" when it takes its parent's). A binding that
        // reads it re-runs when the node is laid out, so one made before
        // the node had a parent settles once it has.
        if key == "effective_direction" {
            let mut state = read_state.borrow_mut();
            if let Some(active) = &mut state.active {
                active
                    .property_reads
                    .insert((node.handle, LAYOUT_SIZE.to_owned(), false));
            }
            let rtl = state.scene.is_rtl(node.handle);
            stack.replace(ctx, if rtl { "rtl" } else { "ltr" });
            return Ok(CallbackReturn::Return);
        }
        // The laid-out rectangle, as the last frame resolved it. Distinct
        // from `width`, which is what the node asked for and is zero for a
        // node sized by its parent or its children. A binding that reads
        // one of these re-runs when the frame moves it.
        if let Some(axis) = key.strip_prefix("layout_")
            && matches!(axis, "x" | "y" | "width" | "height")
        {
            let mut state = read_state.borrow_mut();
            if let Some(active) = &mut state.active {
                let which = if matches!(axis, "x" | "y") {
                    LAYOUT_POSITION
                } else {
                    LAYOUT_SIZE
                };
                active
                    .property_reads
                    .insert((node.handle, which.to_owned(), false));
            }
            let value =
                state
                    .transform_tracker
                    .geometry(node.handle)
                    .map_or(LuaValue::Nil, |geometry| {
                        LuaValue::Number(match axis {
                            "x" => geometry.x,
                            "y" => geometry.y,
                            "width" => geometry.width,
                            _ => geometry.height,
                        })
                    });
            stack.replace(ctx, value);
            return Ok(CallbackReturn::Return);
        }
        let (property, target) = key
            .strip_suffix("_target")
            .map_or((key.as_str(), false), |property| (property, true));
        let value = {
            let mut state = read_state.borrow_mut();
            if !state
                .scene
                .has_property(node.handle, property)
                .map_err(|error| HostError(error.to_string()))?
            {
                return Err(HostError(format!("unknown node property `{key}`")).into());
            }
            let property_key = (node.handle, property.to_owned(), target);
            let signal = state.property_signals.get(&property_key).copied();
            if let Some(active) = &mut state.active {
                if let Some(signal) = signal {
                    active.reads.insert(signal);
                } else {
                    active.property_reads.insert(property_key);
                }
            }
            if target {
                state.scene.target(node.handle, property)
            } else {
                state.scene.current(node.handle, property)
            }
            .map_err(|error| HostError(error.to_string()))?
            .clone()
        };
        stack.replace(ctx, scene_to_lua(ctx, &value).map_err(HostError)?);
        Ok(CallbackReturn::Return)
    });
    let new_index = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, property, value): (UserRef<NodeToken>, String, LuaValue) = stack.consume(ctx)?;
        // A handler set or taken away after the node was made: `node.on_pressed
        // = fn` hears presses from now on, `= nil` stops.
        if let Some(event) = crate::configure::handler_event(&property) {
            let mut state = state.try_borrow_mut().map_err(|_| {
                HostError("nodes cannot be written to from inside a layout function".to_owned())
            })?;
            match value {
                LuaValue::Function(luna::Function::Closure(closure)) => {
                    state.handlers.insert(
                        (node.handle, event),
                        crate::vm::handler_store::register(ctx.stash(closure)),
                    );
                }
                LuaValue::Nil => {
                    state.handlers.remove(&(node.handle, event));
                }
                _ => return Err(HostError(format!("{property} must be a function or nil")).into()),
            }
            return Ok(CallbackReturn::Return);
        }
        if matches!(
            property.as_str(),
            "stretch" | "track" | "mask" | "shortcuts"
        ) {
            let mut state = state.try_borrow_mut().map_err(|_| {
                HostError("nodes cannot be written to from inside a layout function".to_owned())
            })?;
            crate::configure::assign_engine_relation(
                &mut state,
                ctx,
                node.handle,
                &property,
                value,
            )
            .map_err(HostError)?;
            return Ok(CallbackReturn::Return);
        }
        // A table holding bindings among its fields is bound as a whole.
        let value = match value {
            LuaValue::Table(table) if crate::configure_states::binds_inside(&property) => {
                let limits = state.try_borrow().map(|s| s.limits).map_err(|_| {
                    HostError("nodes cannot be written to from inside a layout function".to_owned())
                })?;
                match crate::configure_states::table_binding(ctx, table, limits)
                    .map_err(HostError)?
                {
                    Some(closure) => LuaValue::Function(luna::Function::Closure(closure)),
                    None => value,
                }
            }
            // A name or description taken away is an empty one.
            LuaValue::Nil
                if matches!(
                    property.as_str(),
                    "accessible_name" | "accessible_description" | "accessible_role"
                ) =>
            {
                LuaValue::String(ctx.intern(b""))
            }
            _ => value,
        };
        // A function is a binding, as in a constructor: it replaces any the
        // property had and runs at once.
        if let LuaValue::Function(luna::Function::Closure(closure)) = value {
            {
                let state = state.try_borrow().map_err(|_| {
                    HostError("nodes cannot be written to from inside a layout function".to_owned())
                })?;
                refuse_runtime_owned(&state, node.handle, &property).map_err(HostError)?;
                if !state
                    .scene
                    .has_property(node.handle, &property)
                    .map_err(|error| HostError(error.to_string()))?
                {
                    let element = state
                        .scene
                        .element(node.handle)
                        .map_err(|error| HostError(error.to_string()))?;
                    return Err(
                        HostError(format!("unknown {element:?} property `{property}`")).into(),
                    );
                }
            }
            crate::reactive_bindings::rebind_property(&state, ctx, node.handle, property, closure);
            return Ok(CallbackReturn::Return);
        }
        let value = lua_to_scene(ctx, value, 0).map_err(HostError)?;
        // A write while the scene is borrowed by a layout pass -- from
        // inside a `ui.Layout` function -- is refused rather than allowed to
        // change the tree the pass is walking.
        let mut state = state.try_borrow_mut().map_err(|_| {
            HostError("nodes cannot be written to from inside a layout function".to_owned())
        })?;
        refuse_runtime_owned(&state, node.handle, &property).map_err(HostError)?;
        assign_scene_property(&mut state, node.handle, &property, value).map_err(HostError)?;
        // A text input takes a write in at once, so the caret a handler
        // reads back after setting `text` is already the one that text has.
        if state.scene.element(node.handle).ok() == Some(Element::TextInput) {
            crate::text_inputs::pull(&mut state, node.handle);
            if property == "focus" {
                crate::text_inputs::reconcile_focus(&mut state);
            }
        }
        Ok(CallbackReturn::Return)
    });
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__index", index);
    metatable.set_field(ctx, "__newindex", new_index);
    metatable
}

/// Refuses a configuration's write to a property the runtime keeps: a
/// `MouseArea`'s `hovered` and `pressed` say what the pointer is doing, and
/// a write would only be undone by the next pointer event.
pub(crate) fn refuse_runtime_owned(
    state: &ReactiveState,
    node: NodeHandle,
    property: &str,
) -> Result<(), String> {
    if matches!(property, "focused" | "visual_focus") {
        return Err(format!(
            "`{property}` is read-only: focus sets it (morf.focus.set moves focus)"
        ));
    }
    if property == CONTAINS_POINTER {
        return Err("`contains_pointer` is read-only: the pointer sets it".to_owned());
    }
    if matches!(property, "hovered" | "pressed")
        && state.scene.element(node).ok() == Some(Element::MouseArea)
    {
        return Err(format!(
            "MouseArea `{property}` is read-only: the pointer sets it"
        ));
    }
    if property == "links" && state.scene.element(node).ok() == Some(Element::Text) {
        return Err("Text `links` is read-only: the layout says where they are".to_owned());
    }
    if matches!(property, "status" | "error" | "frame_count")
        && state.scene.element(node).ok() == Some(Element::Image)
    {
        return Err(format!(
            "Image `{property}` is read-only: the runtime says what became of the source"
        ));
    }
    if matches!(
        property,
        "columns" | "rows" | "title" | "running" | "exit_code"
    ) && state.scene.element(node).ok() == Some(Element::Terminal)
    {
        return Err(format!(
            "Terminal `{property}` is read-only: the terminal and its program set it"
        ));
    }
    Ok(())
}
