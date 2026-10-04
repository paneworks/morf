//! Delivering offers — the clipboard's and a drag's — to the configuration.

use luna::{Context, Executor, Value as LuaValue, Variadic};
use morf_scene::{NodeHandle, Value as SceneValue};

use crate::api_clipboard::offer_table;
use crate::reactive_execute::drive_executor;
use crate::{events::*, surface_types::*, types::*};
use morf_runtime::Handler;

/// Runs one callback with arguments built inside the Lua context.
fn execute_with<'gc>(
    ctx: Context<'gc>,
    closure: &Handler,
    args: Vec<LuaValue<'gc>>,
    limits: Limits,
) -> Result<(), String> {
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(closure))
            .into(),
        Variadic(args),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

impl Runtime {
    /// Takes the selections the configuration asked to own.
    pub fn take_clipboard_requests(&mut self) -> Vec<ClipboardRequest> {
        std::mem::take(&mut self.reactive.borrow_mut().clipboard_requests)
    }

    /// Whether any configuration code is watching the selection.
    pub fn watches_clipboard(&self) -> bool {
        !self.reactive.borrow().clipboard_watchers.is_empty()
    }

    /// Tells every `morf.clipboard.watch` callback the selection changed.
    ///
    /// A primary-selection change reaches only the watchers that asked for it.
    pub fn dispatch_selection(&mut self, primary: bool, offer: Option<OfferDescription>) -> bool {
        let watchers = self
            .reactive
            .borrow()
            .clipboard_watchers
            .iter()
            .filter(|(_, wants_primary)| !primary || *wants_primary)
            .map(|(callback, _)| callback.clone())
            .collect::<Vec<_>>();
        let state = std::rc::Rc::clone(&self.reactive);
        for callback in &watchers {
            let result = self.run_handler(|ctx, limits| {
                let argument = offer.as_ref().map_or(LuaValue::Nil, |offer| {
                    LuaValue::Table(offer_table(ctx, &state, offer, Some(primary), None))
                });
                execute_with(ctx, callback, vec![argument], limits)
            });
            if let Err(message) = result {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("clipboard watch: {message}"));
            }
        }
        !watchers.is_empty()
    }

    /// Takes the offer reads the configuration asked for.
    pub fn take_offer_reads(&mut self) -> Vec<OfferReadRequest> {
        std::mem::take(&mut self.reactive.borrow_mut().offer_reads)
    }

    /// Hands a finished read to its callback as `(bytes, nil)` or `(nil, error)`.
    pub fn dispatch_offer_read(
        &mut self,
        request_id: u64,
        result: Result<Vec<u8>, String>,
    ) -> bool {
        let Some(callback) = self
            .reactive
            .borrow_mut()
            .offer_read_callbacks
            .remove(&request_id)
        else {
            return false;
        };
        let outcome = self.run_handler(|ctx, limits| {
            let args = match &result {
                Ok(bytes) => vec![LuaValue::String(ctx.intern(bytes)), LuaValue::Nil],
                Err(error) => vec![
                    LuaValue::Nil,
                    LuaValue::String(ctx.intern(error.as_bytes())),
                ],
            };
            execute_with(ctx, &callback, args, limits)
        });
        if let Err(message) = outcome {
            self.reactive
                .borrow_mut()
                .log(LogLevel::Warn, format!("offer read: {message}"));
        }
        true
    }

    /// Takes the drags out the configuration asked to start.
    pub fn take_drag_requests(&mut self) -> Vec<DragRequest> {
        std::mem::take(&mut self.reactive.borrow_mut().drag_requests)
    }

    /// Tells whoever started a drag out how it ended.
    pub fn dispatch_drag_ended(&mut self, dropped: bool) -> bool {
        let callbacks = std::mem::take(&mut self.reactive.borrow_mut().drag_end_callbacks);
        for callback in &callbacks {
            if let Err(message) = self.run_handler(|ctx, limits| {
                execute_with(ctx, callback, vec![LuaValue::Boolean(dropped)], limits)
            }) {
                self.reactive
                    .borrow_mut()
                    .log(LogLevel::Warn, format!("drag finished: {message}"));
            }
        }
        !callbacks.is_empty()
    }

    /// The types a `DropArea` accepts, from its `keys`: a list, one string,
    /// or nothing for anything at all.
    pub fn drop_area_keys(&self, node: NodeHandle) -> Vec<String> {
        let state = self.reactive.borrow();
        match state.scene.current(node, "keys") {
            Ok(SceneValue::List(values)) => values
                .iter()
                .filter_map(|value| match value {
                    SceneValue::String(key) => Some(key.clone()),
                    _ => None,
                })
                .collect(),
            Ok(SceneValue::String(key)) if !key.is_empty() => vec![key.clone()],
            _ => Vec::new(),
        }
    }

    fn dispatch_drop_handler(
        &mut self,
        node: NodeHandle,
        event: UiEvent,
        build: impl for<'gc> FnOnce(Context<'gc>) -> Vec<LuaValue<'gc>>,
    ) -> bool {
        let handler = self.reactive.borrow().handlers.get(&(node, event)).cloned();
        let Some(handler) = handler else {
            return false;
        };
        let result = self.run_handler(|ctx, limits| {
            let args = build(ctx);
            execute_with(ctx, &handler, args, limits)
        });
        if let Err(message) = result {
            self.reactive.borrow_mut().log(
                LogLevel::Warn,
                format!("{:?}.{}: {message}", node, event.property()),
            );
        }
        true
    }

    /// A drag came over a `DropArea`: `on_entered(info)`, where `info`
    /// carries the offered types and, as `accepted`, which one the area took.
    pub fn dispatch_drag_entered(
        &mut self,
        node: NodeHandle,
        point: EventPoint,
        offer: &OfferDescription,
    ) -> bool {
        let state = std::rc::Rc::clone(&self.reactive);
        self.dispatch_drop_handler(node, UiEvent::PointerEntered, |ctx| {
            vec![LuaValue::Table(offer_table(
                ctx,
                &state,
                offer,
                None,
                Some(point),
            ))]
        })
    }

    /// The drag moved over the area: `on_moved(x, y, surface_x, surface_y)`,
    /// local coordinates first.
    pub fn dispatch_drag_moved(&mut self, node: NodeHandle, point: EventPoint) -> bool {
        self.dispatch_drop_handler(node, UiEvent::DropMoved, |_| {
            vec![
                LuaValue::Number(point.local_x),
                LuaValue::Number(point.local_y),
                LuaValue::Number(point.surface_x),
                LuaValue::Number(point.surface_y),
            ]
        })
    }

    /// The drag left the area, or was cancelled over it: `on_exited()`.
    pub fn dispatch_drag_exited(&mut self, node: NodeHandle) -> bool {
        self.dispatch_drop_handler(node, UiEvent::PointerExited, |_| Vec::new())
    }

    /// The drag was let go over the area: `on_dropped(drop)`, with `uris`,
    /// `paths` and `text` already fetched, and `drop:read` for anything else —
    /// only inside the handler, since the drop is finished once it returns.
    pub fn dispatch_dropped(
        &mut self,
        node: NodeHandle,
        point: EventPoint,
        drop: &OfferDescription,
    ) -> bool {
        let state = std::rc::Rc::clone(&self.reactive);
        self.dispatch_drop_handler(node, UiEvent::Dropped, |ctx| {
            vec![LuaValue::Table(offer_table(
                ctx,
                &state,
                drop,
                None,
                Some(point),
            ))]
        })
    }
}
