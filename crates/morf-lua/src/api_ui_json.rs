use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use morf_scene::Element;

use crate::{constructors::*, scene_bindings::*, serialization::*, state::*, types::*};

pub(crate) fn install_ui_json_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: Limits,
) -> (Table<'gc>, Table<'gc>, crate::api_http::JsonKinds) {
    let ui = Table::new(&ctx);
    for (name, element) in [
        ("Item", Element::Item),
        ("Inset", Element::Inset),
        ("Rect", Element::Rect),
        ("ClipRect", Element::ClipRect),
        ("Text", Element::Text),
        ("TextInput", Element::TextInput),
        ("Image", Element::Image),
        ("Icon", Element::Icon),
        ("Sdf", Element::Sdf),
        ("SdfShape", Element::SdfShape),
        ("Path", Element::Path),
        ("MouseArea", Element::MouseArea),
        ("DropArea", Element::DropArea),
        ("Row", Element::Row),
        ("Column", Element::Column),
        ("Grid", Element::Grid),
        ("Flex", Element::Flex),
    ] {
        ui.set_field(
            ctx,
            name,
            element_constructor(ctx, Rc::clone(&state), limits, element),
        );
    }
    // `ui.each(list, delegate, options)`: a Repeater over a list, which is
    // what a `morf.state` array is. `options.as` lays the rows out as a
    // column, row or grid, as for a Repeater.
    let each_state = Rc::clone(&state);
    let each = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (model, delegate, options): (LuaValue, LuaValue, Option<Table>) = stack.consume(ctx)?;
        let properties = Table::new(&ctx);
        if let Some(options) = options {
            for (key, value) in options.iter(ctx) {
                properties.set(ctx, key, value)?;
            }
        }
        properties.set_field(ctx, "model", model);
        properties.set_field(ctx, "delegate", delegate);
        let node = crate::constructors::construct_view(
            ctx,
            &each_state,
            limits,
            ViewKind::Repeater,
            properties,
        )?;
        stack.replace(ctx, node);
        Ok(CallbackReturn::Return)
    });
    ui.set_field(ctx, "each", each);
    // A kind that does not exist is named, not a nil that fails to call.
    let unknown_kind = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (_, key): (Table, LuaValue) = stack.consume(ctx)?;
        let key = match key {
            LuaValue::String(name) => name.display_lossy().to_string(),
            other => format!("{other:?}"),
        };
        Err(HostError(format!(
            "no ui kind `{key}`: the kinds are Item, Inset, Rect, ClipRect, Text, TextInput, Image, Icon, Sdf, SdfShape, Path, MouseArea, DropArea, Row, Column, Grid, Flex, Flickable, Loader, Timer, Terminal, Layout, Repeater, ListView, GridView, each"
        ))
        .into())
    });
    let ui_metatable = Table::new(&ctx);
    ui_metatable.set_field(ctx, "__index", unknown_kind);
    ui.set_metatable(ctx, Some(ui_metatable));
    ui.set_field(
        ctx,
        "Layout",
        crate::constructors_layout::layout_constructor(ctx, Rc::clone(&state), limits),
    );
    ui.set_field(
        ctx,
        "Repeater",
        view_constructor(ctx, Rc::clone(&state), limits, ViewKind::Repeater),
    );
    ui.set_field(
        ctx,
        "ListView",
        view_constructor(ctx, Rc::clone(&state), limits, ViewKind::List),
    );
    ui.set_field(
        ctx,
        "GridView",
        view_constructor(ctx, Rc::clone(&state), limits, ViewKind::Grid),
    );
    ui.set_field(
        ctx,
        "Flickable",
        element_constructor(ctx, Rc::clone(&state), limits, Element::Flickable),
    );
    ui.set_field(
        ctx,
        "Loader",
        loader_constructor(ctx, Rc::clone(&state), limits),
    );
    ui.set_field(
        ctx,
        "Timer",
        timer_constructor(ctx, Rc::clone(&state), limits),
    );
    ui.set_field(
        ctx,
        "Terminal",
        terminal_constructor(ctx, Rc::clone(&state), limits),
    );
    let reparent_state = Rc::clone(&state);
    let reparent = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (child, parent): (UserRef<NodeToken>, LuaValue) = stack.consume(ctx)?;
        let parent = match parent {
            LuaValue::Nil => None,
            LuaValue::UserData(parent) => Some(
                parent
                    .downcast_static::<NodeToken>()
                    .map_err(|_| HostError("parent must be a morf node or nil".into()))?
                    .handle,
            ),
            _ => return Err(HostError("parent must be a morf node or nil".into()).into()),
        };
        let mut state = reparent_state.borrow_mut();
        // Put somewhere while it was leaving: it is wanted after all.
        crate::runtime_helpers::cancel_node_exit(&mut state, child.handle);
        state
            .scene
            .reparent(child.handle, parent)
            .map_err(|error| HostError(error.to_string()))?;
        Ok(CallbackReturn::Return)
    });
    ui.set_field(ctx, "reparent", reparent);
    // Removes a node and everything under it for good: its bindings, its
    // handlers, its animations, and — once nothing is borrowed — its
    // `on_destroyed` hooks, deepest first. A Repeater's delegate is its
    // model's to remove, not this.
    let destroy_state = Rc::clone(&state);
    // A node with an `exit` plays it first; `ui.destroy(node, true)` does not.
    let destroy = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, now): (UserRef<NodeToken>, Option<bool>) = stack.consume(ctx)?;
        if now == Some(true) {
            crate::runtime_helpers::remove_scene_subtree(
                &mut destroy_state.borrow_mut(),
                node.handle,
            );
            crate::reactive_bindings::run_destroyed_hooks(&destroy_state, ctx, limits);
        } else {
            crate::runtime_helpers::let_go_of_node(&destroy_state, ctx, limits, node.handle);
        }
        Ok(CallbackReturn::Return)
    });
    ui.set_field(ctx, "destroy", destroy);
    // `ui.follow(target, property, { node, property, scale, offset, min, max })`:
    // the target's property is the source's, scaled, offset and clamped, on
    // every tick -- the same frame the source moves. Following again
    // replaces it; `ui.follow(target, property, nil)` lets go.
    let follow_state = Rc::clone(&state);
    let follow = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (target, property, spec): (UserRef<NodeToken>, String, Option<Table>) =
            stack.consume(ctx)?;
        let mut state = follow_state.borrow_mut();
        state
            .follows
            .retain(|f| !(f.target == target.handle && f.property == property));
        let Some(spec) = spec else {
            return Ok(CallbackReturn::Return);
        };
        let source: UserRef<NodeToken> = match spec.get_value(ctx, "node") {
            LuaValue::Nil => {
                return Err(HostError("ui.follow needs a `node` to follow".into()).into());
            }
            value => luna::FromValue::from_value(ctx, value)?,
        };
        let number = |key: &str, fallback: f64| -> f64 {
            match spec.get_value(ctx, key) {
                LuaValue::Integer(n) => n as f64,
                LuaValue::Number(n) => n,
                _ => fallback,
            }
        };
        let source_property = match spec.get_value(ctx, "property") {
            LuaValue::String(name) => name.to_str().unwrap_or(&property).to_owned(),
            _ => property.clone(),
        };
        state.follows.push(crate::state::Follow {
            target: target.handle,
            property,
            source: source.handle,
            source_property,
            scale: number("scale", 1.0),
            offset: number("offset", 0.0),
            min: number("min", f64::NEG_INFINITY),
            max: number("max", f64::INFINITY),
        });
        crate::state::apply_follows(&mut state);
        Ok(CallbackReturn::Return)
    });
    ui.set_field(ctx, "follow", follow);
    for kind in ["spring", "smoothed"] {
        ui.set_field(
            ctx,
            kind,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let options: Table = stack.consume(ctx)?;
                // A copy, not the caller's table. Writing `kind` into what was
                // handed over means `local settle = { duration = 200 }` reused
                // for both a spring and a smoothing ends up as whichever was
                // written last — in both places, including the one already
                // built — and nothing says so.
                let tagged = Table::new(&ctx);
                for (key, value) in options.iter(ctx) {
                    tagged.set(ctx, key, value)?;
                }
                tagged.set_field(ctx, "kind", kind);
                stack.replace(ctx, tagged);
                Ok(CallbackReturn::Return)
            }),
        );
    }
    morf.set_field(ctx, "ui", ui);
    let json_array_metatable = Table::new(&ctx);
    json_array_metatable.set_field(ctx, "__json_kind", "array");
    json_array_metatable.set_field(ctx, "__metatable", "morf.io.json");
    let json_object_metatable = Table::new(&ctx);
    json_object_metatable.set_field(ctx, "__json_kind", "object");
    json_object_metatable.set_field(ctx, "__metatable", "morf.io.json");
    let json_null_metatable = Table::new(&ctx);
    json_null_metatable.set_field(ctx, "__metatable", "morf.io.json");
    let json_null = UserData::new_static(&ctx, JsonNullToken);
    json_null.set_metatable(ctx, Some(json_null_metatable));
    let array_metatable = ctx.stash(json_array_metatable);
    let object_metatable = ctx.stash(json_object_metatable);
    let null = ctx.stash(json_null);
    let json_decode = Callback::from_fn(&ctx, {
        let array_metatable = array_metatable.clone();
        let object_metatable = object_metatable.clone();
        let null = null.clone();
        move |ctx, _, mut stack| {
            let source: String = stack.consume(ctx)?;
            if source.len() > 1024 * 1024 {
                return Err(HostError("JSON input exceeds 1 MiB".into()).into());
            }
            let value = serde_json::from_str::<serde_json::Value>(&source)
                .map_err(|error| HostError(error.to_string()))?;
            let mut entries = 0;
            let value = json_to_lua(
                ctx,
                &value,
                ctx.fetch(&array_metatable),
                ctx.fetch(&object_metatable),
                ctx.fetch(&null),
                0,
                &mut entries,
            )
            .map_err(HostError)?;
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let json_encode = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (value, pretty): (LuaValue, LuaValue) = stack.consume(ctx)?;
        let pretty = match pretty {
            LuaValue::Nil => false,
            LuaValue::Boolean(value) => value,
            _ => return Err(HostError("JSON pretty flag must be boolean".into()).into()),
        };
        let mut entries = 0;
        let value = lua_to_json(ctx, value, 0, &mut entries).map_err(HostError)?;
        let encoded = if pretty {
            serde_json::to_string_pretty(&value)
        } else {
            serde_json::to_string(&value)
        }
        .map_err(|error| HostError(error.to_string()))?;
        if encoded.len() > 1024 * 1024 {
            return Err(HostError("JSON output exceeds 1 MiB".into()).into());
        }
        stack.replace(ctx, encoded);
        Ok(CallbackReturn::Return)
    });
    let json_array = Callback::from_fn(&ctx, {
        let array_metatable = array_metatable.clone();
        move |ctx, _, mut stack| {
            let value: Table = stack.consume(ctx)?;
            value.set_metatable(ctx, Some(ctx.fetch(&array_metatable)));
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let json_object = Callback::from_fn(&ctx, {
        let object_metatable = object_metatable.clone();
        move |ctx, _, mut stack| {
            let value: Table = stack.consume(ctx)?;
            value.set_metatable(ctx, Some(ctx.fetch(&object_metatable)));
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let json_file_read = Callback::from_fn(&ctx, {
        let array_metatable = array_metatable.clone();
        let object_metatable = object_metatable.clone();
        let null = null.clone();
        move |ctx, _, mut stack| {
            let file: UserRef<FileDocumentToken> = stack.consume(ctx)?;
            let file = file.file.borrow();
            let data = file
                .data()
                .ok_or_else(|| HostError("JSON file view is not loaded".into()))?;
            let value = serde_json::from_slice::<serde_json::Value>(data)
                .map_err(|error| HostError(error.to_string()))?;
            let mut entries = 0;
            let value = json_to_lua(
                ctx,
                &value,
                ctx.fetch(&array_metatable),
                ctx.fetch(&object_metatable),
                ctx.fetch(&null),
                0,
                &mut entries,
            )
            .map_err(HostError)?;
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let json_file_write = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (file, value, pretty): (UserRef<FileDocumentToken>, LuaValue, LuaValue) =
            stack.consume(ctx)?;
        let pretty = match pretty {
            LuaValue::Nil => true,
            LuaValue::Boolean(value) => value,
            _ => return Err(HostError("JSON pretty flag must be boolean".into()).into()),
        };
        let mut entries = 0;
        let value = lua_to_json(ctx, value, 0, &mut entries).map_err(HostError)?;
        let mut encoded = if pretty {
            serde_json::to_vec_pretty(&value)
        } else {
            serde_json::to_vec(&value)
        }
        .map_err(|error| HostError(error.to_string()))?;
        if pretty {
            encoded.push(b'\n');
        }
        stack.replace(ctx, file.file.borrow_mut().set_data(&encoded));
        Ok(CallbackReturn::Return)
    });
    let json = Table::new(&ctx);
    json.set_field(ctx, "decode", json_decode);
    json.set_field(ctx, "encode", json_encode);
    json.set_field(ctx, "array", json_array);
    json.set_field(ctx, "object", json_object);
    json.set_field(ctx, "null", json_null);
    json.set_field(ctx, "read_file", json_file_read);
    json.set_field(ctx, "write_file", json_file_write);
    morf.set_field(ctx, "json", json);
    let kinds = crate::api_http::JsonKinds {
        array: array_metatable,
        object: object_metatable,
        null,
    };
    (ui, json, kinds)
}
