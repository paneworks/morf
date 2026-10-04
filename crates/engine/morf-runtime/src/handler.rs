//! A handler a configuration registered, named without naming the language
//! it is written in: the runtime holds `Handler`s and the scripting layer
//! keeps what they stand for (PLAN.md phase 5).
//!
//! A `Handler` is shared like the closure it stands for: clones are the
//! same handler, and the last one dropped lets the scripting layer forget
//! it.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

/// The number a handler goes by.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HandlerId(pub u64);

/// Where handlers are kept: told when the last reference to one goes.
pub trait HandlerRegistry: Any {
    fn release(&self, id: HandlerId);
    /// For the scripting layer, to find its own registry again.
    fn as_any(&self) -> &dyn Any;
}

struct Registered {
    id: HandlerId,
    registry: Rc<dyn HandlerRegistry>,
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.registry.release(self.id);
    }
}

/// A registered handler.
#[derive(Clone)]
pub struct Handler(Rc<Registered>);

impl Handler {
    /// Takes ownership of handler `id` in `registry`.
    pub fn new(id: HandlerId, registry: Rc<dyn HandlerRegistry>) -> Self {
        Self(Rc::new(Registered { id, registry }))
    }

    pub fn id(&self) -> HandlerId {
        self.0.id
    }

    pub fn registry(&self) -> &dyn HandlerRegistry {
        self.0.registry.as_ref()
    }
}

/// What runs handlers: the scripting layer. The runtime's subsystems call
/// a handler through this and never see the language it is written in.
pub trait Handlers {
    /// Calls `handler` with `args`; what it returned, or why it failed.
    fn call(
        &mut self,
        handler: &Handler,
        args: &[morf_value::IpcValue],
    ) -> Result<Vec<morf_value::IpcValue>, String>;

    /// Calls `handler` for its effect, whatever it returns: a notification
    /// (an animation finished, say) rather than a question.
    fn notify(&mut self, handler: &Handler, args: &[morf_value::IpcValue]) -> Result<(), String> {
        self.call(handler, args).map(|_| ())
    }
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handler({})", self.0.id.0)
    }
}
