//! Views from Lua's side: a delegate built and rebound by running its
//! function, and the scene a view's rows live in. What a view does with
//! them is `morf_runtime::views`'.

use luna::{Context, Executor, Function, UserRef, Value as LuaValue, Variadic};
use std::cell::RefCell;
use std::rc::Rc;

use morf_scene::{Element, NodeHandle, Scene, Value as SceneValue, ViewTransition};

use crate::{
    reactive_bindings::*,
    reactive_execute::*,
    runtime_helpers::{begin_node_exit, cancel_node_exit, remove_scene_subtree},
    scene_bindings::*,
    serialization::*,
    state::*,
    types::*,
};
use morf_runtime::Handler;
use morf_runtime::views::ViewHost;
pub(crate) use morf_runtime::views::{position_view_child, row_extents};

pub(crate) fn execute_delegate(
    ctx: Context<'_>,
    delegate: &Handler,
    item: &SceneValue,
    index: usize,
    limits: Limits,
) -> Result<Delegate, String> {
    let args = Variadic(vec![
        scene_to_lua(ctx, item)?,
        LuaValue::Integer(index as i64 + 1),
    ]);
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(delegate))
            .into(),
        args,
    );
    drive_executor(ctx, executor, limits, limits.delegate_fuel, "delegate")?;
    let values = match executor.take_result::<Variadic<Vec<LuaValue>>>(ctx) {
        Ok(Ok(values)) => values,
        Ok(Err(error)) => return Err(error.to_string()),
        Err(error) => return Err(error.to_string()),
    };
    let Some(LuaValue::UserData(node)) = values.first().copied() else {
        return Err("view delegate must return a morf node".to_owned());
    };
    let node = node
        .downcast_static::<NodeToken>()
        .map_err(|_| "view delegate must return a morf node".to_owned())?;
    let updater = match values.get(1).copied().unwrap_or(LuaValue::Nil) {
        LuaValue::Nil => None,
        LuaValue::Function(Function::Closure(updater)) => Some(ctx.stash(updater)),
        _ => return Err("view delegate updater must be a function".to_owned()),
    };
    Ok(Delegate {
        node: node.handle,
        updater: updater.map(crate::vm::handler_store::register),
        item: item.clone(),
        index,
    })
}

pub(crate) fn execute_delegate_updater(
    ctx: Context<'_>,
    updater: &Handler,
    item: &SceneValue,
    index: usize,
    limits: Limits,
) -> Result<(), String> {
    let args = Variadic(vec![
        scene_to_lua(ctx, item)?,
        LuaValue::Integer(index as i64 + 1),
    ]);
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(updater))
            .into(),
        args,
    );
    drive_executor(
        ctx,
        executor,
        limits,
        limits.effect_fuel,
        "delegate updater",
    )?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn execute_node_factory(
    ctx: Context<'_>,
    factory: &Handler,
    limits: Limits,
) -> Result<NodeHandle, String> {
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(factory))
            .into(),
        (),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "Loader source")?;
    match executor.take_result::<UserRef<NodeToken>>(ctx) {
        Ok(Ok(node)) => Ok(node.handle),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// What a view asks of Lua and the scene, from inside a turn.
struct LuaViewHost<'a, 'gc> {
    state: &'a Rc<RefCell<ReactiveState>>,
    ctx: Context<'gc>,
    limits: Limits,
}

impl ViewHost for LuaViewHost<'_, '_> {
    fn build(
        &mut self,
        delegate: &Handler,
        item: &SceneValue,
        index: usize,
    ) -> Result<Delegate, String> {
        execute_delegate(self.ctx, delegate, item, index, self.limits)
    }

    fn update(&mut self, updater: &Handler, item: &SceneValue, index: usize) -> Result<(), String> {
        execute_delegate_updater(self.ctx, updater, item, index, self.limits)
            .and_then(|()| flush_reactive(self.state, self.ctx, self.limits))
    }

    fn create(&mut self) -> NodeHandle {
        create_node(self.state, Element::Item)
    }

    fn remove(&mut self, node: NodeHandle) {
        remove_scene_subtree(&mut self.state.borrow_mut(), node);
    }

    fn begin_exit(&mut self, node: NodeHandle) -> bool {
        begin_node_exit(&mut self.state.borrow_mut(), node)
    }

    fn cancel_exit(&mut self, node: NodeHandle) -> bool {
        cancel_node_exit(&mut self.state.borrow_mut(), node)
    }

    fn with_scene<R>(&mut self, f: impl FnOnce(&mut Scene) -> R) -> R {
        f(&mut self.state.borrow_mut().scene)
    }
}

/// Brings `view`, filling `parent`, up to its model, scrolled `offset`
/// along, building and rebinding its delegates in Lua.
pub(crate) fn reconcile_lua_view(
    state: &Rc<RefCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    parent: NodeHandle,
    offset: f64,
    view: &mut VirtualView,
) -> Result<Vec<ViewTransition>, String> {
    let mut host = LuaViewHost { state, ctx, limits };
    view.reconcile(&mut host, parent, offset)
}
