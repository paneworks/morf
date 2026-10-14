use crate::{runtime::host_types::*, vm::execute::*, vm::types::*};

impl Runtime {
    /// What the runtime is holding: live scene nodes, the scene's property
    /// signals, the reactive graph's signals and effects, and the Lua
    /// bindings behind them. Every figure should track what is on screen;
    /// one that only grows is a leak.
    pub fn resource_stats(&self) -> ResourceStats {
        let state = self.reactive.borrow();
        let (graph_signals, graph_effects) = state
            .reactive
            .graph()
            .map_or((0, 0), |graph| (graph.signal_count(), graph.effect_count()));
        ResourceStats {
            nodes: state.scene.node_count(),
            scene_signals: state.scene.property_signal_count(),
            graph_signals,
            graph_effects,
            bindings: state.reactive.effect_count(),
            tracked_signals: state.reactive.signal_count(),
            handlers: state.events().len(),
        }
    }

    /// Takes a successful native authentication request to release a session lock.
    pub fn take_session_unlock_request(&mut self) -> bool {
        self.reactive
            .borrow_mut()
            .lifecycle_mut()
            .take_unlock_request()
    }

    /// Returns registered IPC verb names in lexical order.
    pub fn ipc_verbs(&self) -> Vec<String> {
        self.reactive.borrow().ipc().names()
    }

    /// Calls one registered IPC handler with bounded primitive arguments.
    pub fn call_ipc(&mut self, verb: &str, args: &[IpcValue]) -> Result<Vec<IpcValue>, Error> {
        let handler = self
            .reactive
            .borrow()
            .ipc()
            .get(verb)
            .cloned()
            .ok_or_else(|| Error::Runtime(format!("unknown IPC verb `{verb}`")))?;
        let _span = crate::profile::span(|| format!("ipc {verb}"));
        self.run_handler(|ctx, limits| execute_ipc_handler(ctx, &handler, args, limits))
            .map_err(Error::Runtime)
    }
}
