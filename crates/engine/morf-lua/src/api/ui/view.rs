use luna::{Callback, CallbackReturn, Context, Table, UserData, UserRef, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use morf_scene::{ListModel, Value as SceneValue};

use crate::{
    api::ui::bindings::*, api::ui::scene::*, runtime::model_revisions::*, runtime::state::*,
    runtime::views::*, vm::loader::*, vm::types::*,
};

pub(crate) fn install_view_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    morf: Table<'gc>,
    limits: Limits,
) {
    // Reads register the model's revision as a dependency of the running
    // binding; changes bump it. See `model_revisions`.
    let model_len = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let model: UserRef<ListModelToken> = stack.consume(ctx)?;
            track_model_read(&mut state.borrow_mut(), &model.model);
            stack.replace(ctx, model.model.borrow().len() as i64);
            Ok(CallbackReturn::Return)
        }
    });
    let model_get = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (model, index): (UserRef<ListModelToken>, i64) = stack.consume(ctx)?;
            track_model_read(&mut state.borrow_mut(), &model.model);
            // Outside the list, as a Lua table's index is: nothing.
            if index < 1 || index as usize > model.model.borrow().len() {
                stack.replace(ctx, LuaValue::Nil);
                return Ok(CallbackReturn::Return);
            }
            let value = model_row(ctx, &model.model, lua_index(index)?)?;
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let model_index_of = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (model, value): (UserRef<ListModelToken>, LuaValue) = stack.consume(ctx)?;
            track_model_read(&mut state.borrow_mut(), &model.model);
            let value = lua_to_scene(ctx, value, 0).map_err(HostError)?;
            let model = model.model.borrow();
            let index = (0..model.len()).find(|index| {
                model
                    .get(*index)
                    .is_some_and(|(_, candidate)| candidate == &value)
            });
            match index {
                Some(index) => stack.replace(ctx, index as i64 + 1),
                None => stack.replace(ctx, LuaValue::Nil),
            }
            Ok(CallbackReturn::Return)
        }
    });
    let model_insert = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (model, index, value): (UserRef<ListModelToken>, i64, LuaValue) =
                stack.consume(ctx)?;
            let index = lua_insert_index(index, model.model.borrow().len())?;
            let value = lua_to_scene(ctx, value, 0).map_err(HostError)?;
            if model.model.borrow_mut().insert(index, value).is_none() {
                return Err(HostError("list-model insert index is out of range".into()).into());
            }
            model_changed(&state, ctx, limits, &model.model).map_err(HostError)?;
            Ok(CallbackReturn::Return)
        }
    });
    let model_remove = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (model, index): (UserRef<ListModelToken>, i64) = stack.consume(ctx)?;
            let index = lua_index(index)?;
            let removed = model.model.borrow_mut().remove(index);
            let value = removed
                .as_ref()
                .map(|value| scene_to_lua(ctx, value))
                .transpose()
                .map_err(HostError)?
                .unwrap_or(LuaValue::Nil);
            if removed.is_some() {
                model_changed(&state, ctx, limits, &model.model).map_err(HostError)?;
            }
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let model_move = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (model, from, to): (UserRef<ListModelToken>, i64, i64) = stack.consume(ctx)?;
            let from = lua_index(from)?;
            let to = lua_index(to)?;
            if !model.model.borrow_mut().move_item(from, to) {
                return Err(HostError("list-model move index is out of range".into()).into());
            }
            if from != to {
                model_changed(&state, ctx, limits, &model.model).map_err(HostError)?;
            }
            Ok(CallbackReturn::Return)
        }
    });
    let model_set = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (model, index, value): (UserRef<ListModelToken>, i64, LuaValue) =
                stack.consume(ctx)?;
            let index = lua_index(index)?;
            let value = lua_to_scene(ctx, value, 0).map_err(HostError)?;
            let unchanged = model
                .model
                .borrow()
                .get(index)
                .is_some_and(|(_, current)| current == &value);
            if !model.model.borrow_mut().set(index, value) {
                return Err(HostError("list-model update index is out of range".into()).into());
            }
            if !unchanged {
                model_changed(&state, ctx, limits, &model.model).map_err(HostError)?;
            }
            Ok(CallbackReturn::Return)
        }
    });
    let model_replace = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (model, items, object_property): (UserRef<ListModelToken>, Table, Option<String>) =
                stack.consume(ctx)?;
            if object_property
                .as_ref()
                .is_some_and(|property| property.is_empty() || property.len() > 128)
            {
                return Err(HostError(
                    "list-model object property must contain 1 to 128 bytes".into(),
                )
                .into());
            }
            let value = lua_to_scene(ctx, LuaValue::Table(items), 0).map_err(HostError)?;
            let SceneValue::List(values) = value else {
                return Err(HostError("list-model replacement needs an array table".into()).into());
            };
            replace_model_rows(
                &state,
                ctx,
                limits,
                &model.model,
                values,
                object_property.as_deref(),
            )
            .map_err(HostError)?;
            Ok(CallbackReturn::Return)
        }
    });
    let model_methods = Table::new(&ctx);
    model_methods.set_field(ctx, "len", model_len);
    model_methods.set_field(ctx, "get", model_get);
    model_methods.set_field(ctx, "index_of", model_index_of);
    model_methods.set_field(ctx, "insert", model_insert);
    model_methods.set_field(ctx, "remove", model_remove);
    model_methods.set_field(ctx, "move", model_move);
    model_methods.set_field(ctx, "set", model_set);
    model_methods.set_field(ctx, "replace", model_replace);
    // `#model` and `model[i]` read -- and are tracked -- like `len` and
    // `get`, so `for i = 1, #model do ... model[i] ... end` and `ipairs`
    // follow the model too. Any other key is a method.
    let model_index = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        let methods = ctx.stash(model_methods);
        move |ctx, _, mut stack| {
            let (model, key): (UserRef<ListModelToken>, LuaValue) = stack.consume(ctx)?;
            let index = match key {
                LuaValue::Integer(index) => Some(index),
                LuaValue::Number(index) if index.fract() == 0.0 => Some(index as i64),
                _ => None,
            };
            let value = match index {
                Some(index) => {
                    track_model_read(&mut state.borrow_mut(), &model.model);
                    match usize::try_from(index - 1) {
                        Ok(index) if index < model.model.borrow().len() => {
                            model_row(ctx, &model.model, index)?
                        }
                        _ => LuaValue::Nil,
                    }
                }
                None => ctx.fetch(&methods).get_value(ctx, key),
            };
            stack.replace(ctx, value);
            Ok(CallbackReturn::Return)
        }
    });
    let model_metatable = Table::new(&ctx);
    model_metatable.set_field(ctx, "__index", model_index);
    model_metatable.set_field(ctx, "__len", model_len);
    let model_metatable = ctx.stash(model_metatable);
    state
        .borrow_mut()
        .metatables_mut()
        .set_model(model_metatable.clone());
    let list_model = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let items: Table = stack.consume(ctx)?;
        let value = lua_to_scene(ctx, LuaValue::Table(items), 0).map_err(HostError)?;
        let SceneValue::List(values) = value else {
            return Err(HostError("list-model needs an array table".into()).into());
        };
        let userdata = UserData::new_static(
            &ctx,
            ListModelToken {
                model: Rc::new(RefCell::new(ListModel::new(values))),
            },
        );
        userdata.set_metatable(ctx, Some(ctx.fetch(&model_metatable)));
        stack.replace(ctx, userdata);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "list_model", list_model);

    let sync_state = Rc::clone(&state);
    let sync_view = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, offset): (UserRef<NodeToken>, f64) = stack.consume(ctx)?;
        let mut view = sync_state
            .borrow_mut()
            .views_mut()
            .remove(&node.handle)
            .ok_or_else(|| HostError("node is not a ListView".to_owned()))?;
        let result = reconcile_lua_view(&sync_state, ctx, limits, node.handle, offset, &mut view);
        sync_state
            .borrow_mut()
            .views_mut()
            .insert(node.handle, view);
        let transitions = result.map_err(HostError)?;
        let values = Table::new(&ctx);
        for (index, transition) in transitions.into_iter().enumerate() {
            values
                .set(
                    ctx,
                    index as i64 + 1,
                    view_transition_to_lua(ctx, transition),
                )
                .expect("view-transition table accepts integer keys");
        }
        stack.replace(ctx, values);
        Ok(CallbackReturn::Return)
    });
    morf.set_field(ctx, "sync_view", sync_view);

    // How long a view's content is, all its rows: what a scroll bar over it
    // measures against.
    let extent_state = Rc::clone(&state);
    let view_extent = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let node: UserRef<NodeToken> = stack.consume(ctx)?;
        let extent = {
            let state = extent_state.borrow();
            let view = state
                .views()
                .get(&node.handle)
                .ok_or_else(|| HostError("node is not a ListView".to_owned()))?;
            let count = view.model.borrow().len();
            view.view.content_extent(count)
        };
        stack.replace(ctx, extent);
        Ok(CallbackReturn::Return)
    });
    crate::api::namespace::table(ctx, morf, "ui").set_field(ctx, "view_extent", view_extent);

    // Where row `index` (from 1) of a view starts along it: what keeping a
    // row in sight scrolls to.
    let start_state = Rc::clone(&state);
    let view_item_start = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (node, index): (UserRef<NodeToken>, i64) = stack.consume(ctx)?;
        let start = {
            let state = start_state.borrow();
            let view = state
                .views()
                .get(&node.handle)
                .ok_or_else(|| HostError("node is not a ListView".to_owned()))?;
            view.view.item_start((index - 1).max(0) as usize)
        };
        stack.replace(ctx, start);
        Ok(CallbackReturn::Return)
    });
    crate::api::namespace::table(ctx, morf, "ui").set_field(
        ctx,
        "view_item_start",
        view_item_start,
    );
}
