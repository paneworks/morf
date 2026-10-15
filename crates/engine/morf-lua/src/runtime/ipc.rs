use crate::{runtime::host_types::*, vm::execute::*, vm::types::*};
use luna::{Callback, CallbackReturn};

impl Runtime {
    /// Installs the native controller's live authorization check in this VM.
    pub fn set_controller_authorizer(
        &mut self,
        authorize: ControllerAuthorizer,
    ) -> Result<(), Error> {
        self.controller_authorizer = Some(authorize.clone());
        self.lua
            .try_enter(|ctx| {
                let callback = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    let token: luna::String = stack.consume(ctx)?;
                    let token = token.to_str()?;
                    stack.replace(ctx, token.len() <= 128 && authorize(token));
                    Ok(CallbackReturn::Return)
                });
                ctx.globals().set(ctx, "__morf_prek_authorized", callback)?;
                Ok(())
            })
            .map_err(|error| Error::Runtime(error.to_string()))
    }

    /// Native policy must exist before a replacement configuration executes;
    /// reloadable Lua values cannot grant recognizer ownership.
    pub fn inherit_native_policy(&mut self, previous: &Self) -> Result<(), Error> {
        if let Some(authorize) = previous.controller_authorizer.clone() {
            self.set_controller_authorizer(authorize)?;
        }
        if let Some(gate) = previous.recognizer_gate.clone() {
            self.set_recognizer_gate(gate)?;
        }
        if let Some(authorize) = previous.handoff_authorizer.clone() {
            self.set_handoff_authorizer(authorize)?;
        }
        Ok(())
    }

    /// A live native gate for duplicated global recognizers. Local widget
    /// gestures use their existing handlers and are not disabled by this gate.
    pub fn set_recognizer_gate(&mut self, gate: RecognizerGate) -> Result<(), Error> {
        self.recognizer_gate = Some(gate.clone());
        self.lua
            .try_enter(|ctx| {
                let callback = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    stack.replace(ctx, gate());
                    Ok(CallbackReturn::Return)
                });
                ctx.globals()
                    .set(ctx, "__morf_global_recognizers_enabled", callback)?;
                Ok(())
            })
            .map_err(|error| Error::Runtime(error.to_string()))
    }

    pub fn set_handoff_authorizer(&mut self, authorize: HandoffAuthorizer) -> Result<(), Error> {
        self.handoff_authorizer = Some(authorize.clone());
        self.lua
            .try_enter(|ctx| {
                let callback = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                    let (token, enabled): (luna::String, bool) = stack.consume(ctx)?;
                    let token = token.to_str()?;
                    stack.replace(ctx, token.len() == 32 && authorize(token, enabled));
                    Ok(CallbackReturn::Return)
                });
                ctx.globals()
                    .set(ctx, "__morf_prek_handoff_authorized", callback)?;
                Ok(())
            })
            .map_err(|error| Error::Runtime(error.to_string()))
    }

    /// What the runtime is holding: live scene nodes, the scene's property
    /// signals, the reactive graph's signals and effects, and the Lua
    /// bindings behind them. Every figure should track what is on screen;
    /// one that only grows is a leak.
    pub fn resource_stats(&self) -> ResourceStats {
        let state = self.reactive.borrow();
        let (graph_signals, graph_effects) = state
            .reactive()
            .graph()
            .map_or((0, 0), |graph| (graph.signal_count(), graph.effect_count()));
        ResourceStats {
            nodes: state.scene().node_count(),
            scene_signals: state.scene().property_signal_count(),
            graph_signals,
            graph_effects,
            bindings: state.reactive().effect_count(),
            tracked_signals: state.reactive().signal_count(),
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

#[cfg(test)]
mod native_policy_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn replacement_vm_inherits_live_native_policy_before_configuration() {
        let enabled = Arc::new(AtomicBool::new(false));
        let live = enabled.clone();
        let mut previous = Runtime::default();
        previous
            .set_recognizer_gate(Arc::new(move || live.load(Ordering::Acquire)))
            .unwrap();
        previous
            .set_controller_authorizer(Arc::new(|token| token == "native-token"))
            .unwrap();
        previous
            .set_handoff_authorizer(Arc::new(|token, enabled| {
                token == "c".repeat(32) && !enabled
            }))
            .unwrap();
        let mut candidate = Runtime::default();
        candidate.inherit_native_policy(&previous).unwrap();
        candidate
            .execute(
                "replacement.lua",
                br#"
            assert(__morf_global_recognizers_enabled()==false)
            assert(__morf_prek_authorized("native-token"))
            assert(not __morf_prek_authorized("forged"))
            assert(__morf_prek_handoff_authorized(string.rep("c",32),false))
            assert(not __morf_prek_handoff_authorized(string.rep("c",32),true))
            morf.ipc.gate=function() return __morf_global_recognizers_enabled() end
        "#,
            )
            .unwrap();
        enabled.store(true, Ordering::Release);
        assert_eq!(candidate.call_ipc("gate", &[]).unwrap()[0].to_json(), true);
        drop(previous);
        enabled.store(false, Ordering::Release);
        assert_eq!(candidate.call_ipc("gate", &[]).unwrap()[0].to_json(), false);
    }
}
