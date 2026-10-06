//! A lock screen built once per output.
//!
//! `morf.lock_surface(function(screen) return ui.Rect { ... } end)` hands the
//! lock loop a builder instead of one tree. The loop calls it for each output
//! it locks, with that output's description and the size the compositor gave
//! its lock surface, and draws what it returns there. Each output then has a
//! root of its own, laid out against its own size, while everything the
//! trees share — a password, a PAM conversation, `morf.state` — stays in the
//! one runtime.

use luna::{Executor, UserRef, Value as LuaValue, Variadic};
use morf_scene::NodeHandle;

use crate::{
    api_host::screen_entry, reactive_execute::drive_executor,
    runtime_helpers::remove_scene_subtree, state_tokens::NodeToken, types::*,
};

impl Runtime {
    /// Whether the configuration builds its lock per output.
    pub fn has_lock_surface_builder(&self) -> bool {
        self.reactive
            .borrow()
            .session
            .lock_surface_builder
            .is_some()
    }

    /// Builds one output's lock tree. `screen.width` and `screen.height` are
    /// the lock surface's logical size, which is what the tree must cover.
    ///
    /// The node returned is a scene root of its own; the caller owns it and
    /// gives it back with [`Runtime::remove_lock_surface`].
    pub fn build_lock_surface(
        &mut self,
        screen: &Screen,
        index: usize,
    ) -> Result<NodeHandle, Error> {
        let builder = self
            .reactive
            .borrow()
            .session
            .lock_surface_builder
            .clone()
            .ok_or_else(|| Error::Runtime("no lock surface builder is registered".into()))?;
        let density = self.reactive.borrow().density;
        let node = self.run_handler(|ctx, limits| {
            let table = screen_entry(ctx, screen, density);
            table.set_field(ctx, "index", index as i64 + 1);
            let executor = Executor::start(
                ctx,
                ctx.fetch(&crate::vm::handler_store::stashed(&builder))
                    .into(),
                Variadic(vec![LuaValue::Table(table)]),
            );
            drive_executor(ctx, executor, limits, limits.effect_fuel, "lock surface")?;
            match executor.take_result::<UserRef<NodeToken>>(ctx) {
                Ok(Ok(node)) => Ok(node.handle),
                Ok(Err(error)) => Err(format!("lock surface builder must return a node: {error}")),
                Err(error) => Err(error.to_string()),
            }
        });
        let node = node.map_err(Error::Runtime)?;
        if self
            .scene()
            .parent(node)
            .map_err(|error| Error::Runtime(error.to_string()))?
            .is_some()
        {
            return Err(Error::Runtime(
                "lock surface builder must return a new top-level node".into(),
            ));
        }
        Ok(node)
    }

    /// Takes one output's lock tree down, bindings and handlers with it.
    pub fn remove_lock_surface(&mut self, root: NodeHandle) {
        remove_scene_subtree(&mut self.reactive.borrow_mut(), root);
    }
}
