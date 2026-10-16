//! Claims from the scripting side: the evaluator's host, and one pass of
//! it with its presences mirrored into their signals and whatever read
//! them, or the properties it aimed, flushed.

use luna::Context;
use std::rc::Rc;
use std::time::Duration;

use morf_runtime::animation::claims::{ClaimsFrame, ClaimsHost};
use morf_scene::{Motion, NodeHandle, Scene, Value};

use super::{ReactiveState, StateCell};
use crate::runtime::host_types::IpcValue;
use crate::vm::types::{Limits, LogLevel};

impl ClaimsHost for ReactiveState {
    fn scene(&self) -> &Scene {
        self.engine.scene()
    }

    fn retarget(
        &mut self,
        node: NodeHandle,
        property: &str,
        value: Value,
        motion: Option<Motion>,
    ) -> Result<(), String> {
        crate::api::ui::scene::retarget_scene_property(self, node, property, value, motion)
    }
}

/// Which pass of the evaluator: a tick's, or one node's first values.
pub(crate) enum ClaimsPass {
    Tick(Duration),
    Settle(NodeHandle),
}

/// Runs one pass of the evaluator. Its failures are logged; none stops
/// the pass or the tick it is part of.
pub(crate) fn run_claims(
    state: &Rc<StateCell<ReactiveState>>,
    ctx: Context<'_>,
    limits: Limits,
    pass: ClaimsPass,
) -> ClaimsFrame {
    let frame = {
        let mut state = state.borrow_mut();
        let mut claims = std::mem::take(&mut state.animation_mut().claims);
        let frame = match pass {
            ClaimsPass::Tick(delta) => claims.evaluate(&mut *state, delta),
            ClaimsPass::Settle(node) => claims.settle_node(&mut *state, node),
        };
        state.animation_mut().claims = claims;
        for (signal, presence) in &frame.presences {
            let _ = state
                .reactive_mut()
                .force(*signal, IpcValue::Number(*presence));
        }
        for error in &frame.errors {
            state.log(LogLevel::Warn, error.clone());
        }
        frame
    };
    if (frame.retargeted > 0 || !frame.presences.is_empty())
        && let Err(message) = crate::api::ui::bindings::flush_reactive(state, ctx, limits)
    {
        state
            .borrow_mut()
            .log(LogLevel::Warn, format!("claims: {message}"));
    }
    frame
}
