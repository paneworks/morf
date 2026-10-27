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
        self.policy_retirement = previous.policy_retirement.clone();
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
    pub fn set_native_policy_retirement(&mut self, retire: PolicyRetirement) {
        self.policy_retirement = Some(retire);
    }
    pub fn retire_native_policy(&self) {
        if let Some(retire) = &self.policy_retirement {
            retire();
        }
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

    /// Calls a handler and reports whether visible scene or primary surface
    /// settings changed. Reading state does not owe a frame. Errors can leave
    /// partial visible changes, which still need painting.
    pub fn call_ipc_with_repaint(
        &mut self,
        verb: &str,
        args: &[IpcValue],
    ) -> (Result<Vec<IpcValue>, Error>, bool) {
        let before = self.reactive.borrow().revisions().mark();
        let surface = self.layer_surface_config();
        let result = self.call_ipc(verb, args);
        let repaint = self.reactive.borrow().revisions().shown_since(before)
            || self.layer_surface_config() != surface;
        (result, repaint)
    }
}

#[cfg(test)]
mod repaint_tests {
    use super::*;

    fn runtime() -> Runtime {
        let mut runtime = Runtime::default();
        runtime
            .execute(
                "ipc-repaint.lua",
                br##"
            local ui = require("morf.ui")
            local shown = ui.Rect { width=10, height=10, color="#123456" }
            local hidden = ui.Rect { width=10, height=10, visible=false }
            morf.ipc.status = function() return "ready" end
            morf.ipc.resize = function(width) shown.width=width return width end
            morf.ipc.hidden = function() hidden.width=20 end
            morf.ipc.partial = function() shown.width=30 error("after visible write") end
            morf.ipc.surface = function(width) morf.surface.width=width end
        "##,
            )
            .unwrap();
        runtime
    }

    #[test]
    fn queries_and_unknown_verbs_do_not_request_a_frame() {
        let mut runtime = runtime();
        let (reply, repaint) = runtime.call_ipc_with_repaint("status", &[]);
        assert_eq!(reply.unwrap(), vec![IpcValue::String("ready".into())]);
        assert!(!repaint);
        let (reply, repaint) = runtime.call_ipc_with_repaint("missing", &[]);
        assert!(reply.is_err());
        assert!(!repaint);
    }

    #[test]
    fn only_actual_visible_changes_request_a_frame() {
        let mut runtime = runtime();
        let (reply, repaint) = runtime.call_ipc_with_repaint("resize", &[IpcValue::Integer(20)]);
        assert!(reply.is_ok() && repaint);
        let (reply, repaint) = runtime.call_ipc_with_repaint("resize", &[IpcValue::Integer(20)]);
        assert!(reply.is_ok());
        assert!(!repaint);
        let (reply, repaint) = runtime.call_ipc_with_repaint("hidden", &[]);
        assert!(reply.is_ok());
        assert!(!repaint);
    }

    #[test]
    fn a_failed_handler_still_paints_its_partial_visible_changes() {
        let mut runtime = runtime();
        let (reply, repaint) = runtime.call_ipc_with_repaint("partial", &[]);
        assert!(
            reply
                .unwrap_err()
                .to_string()
                .contains("after visible write")
        );
        assert!(repaint);
    }

    #[test]
    fn surface_settings_do_not_depend_on_a_scene_node_change() {
        let mut runtime = runtime();
        let (reply, repaint) = runtime.call_ipc_with_repaint("surface", &[IpcValue::Integer(200)]);
        assert!(reply.is_ok() && repaint);
        assert_eq!(runtime.layer_surface_config().width, 200);
        let (reply, repaint) = runtime.call_ipc_with_repaint("surface", &[IpcValue::Integer(200)]);
        assert!(reply.is_ok());
        assert!(!repaint);
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
