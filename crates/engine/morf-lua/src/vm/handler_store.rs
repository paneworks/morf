//! The closures behind the runtime's `Handler`s.
//!
//! Kept per thread rather than per runtime: a handler is registered wherever
//! a closure is stashed -- deep in a binding, with nothing but the Lua
//! context at hand -- and a `Handler` never leaves the thread that made it
//! (it is `!Send`), so the thread is exactly the scope it lives in. Ids are
//! unique across every runtime on the thread, so two runtimes' handlers
//! never meet.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use luna::StashedClosure;

use morf_runtime::{Handler, HandlerId, HandlerRegistry};

thread_local! {
    static CLOSURES: RefCell<HashMap<HandlerId, StashedClosure>> = RefCell::new(HashMap::new());
    static NEXT: Cell<u64> = const { Cell::new(0) };
    static REGISTRY: Rc<LuaHandlers> = Rc::new(LuaHandlers);
}

/// The registry every Lua handler on this thread is released to.
struct LuaHandlers;

impl HandlerRegistry for LuaHandlers {
    fn release(&self, id: HandlerId) {
        // Taken out first and dropped after the borrow ends: dropping a
        // closure may drop what it captured, handlers among them.
        let closure = CLOSURES.with(|closures| closures.borrow_mut().remove(&id));
        drop(closure);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Keeps `closure`; it lives as long as the handler returned does.
pub(crate) fn register(closure: StashedClosure) -> Handler {
    let id = NEXT.with(|next| {
        let id = next.get() + 1;
        next.set(id);
        HandlerId(id)
    });
    CLOSURES.with(|closures| closures.borrow_mut().insert(id, closure));
    let registry = REGISTRY.with(Rc::clone);
    Handler::new(id, registry as Rc<dyn HandlerRegistry>)
}

/// The closure behind `handler`.
pub(crate) fn stashed(handler: &Handler) -> StashedClosure {
    debug_assert!(handler.registry().as_any().is::<LuaHandlers>());
    CLOSURES
        .with(|closures| closures.borrow().get(&handler.id()).cloned())
        .expect("a live handler keeps its closure")
}
