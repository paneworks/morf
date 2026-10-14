//! `morf.shared(name, initial)`: one value every screen's shell sees
//! (morf-runtime's `shared` keeps the values; this is the flush after a sync).

use crate::vm::types::Runtime;

impl Runtime {
    /// [`SharedValues::sync`], then the bindings that read what came in.
    pub(crate) fn poll_shared(&mut self) -> bool {
        // Publishing wakes the other copies that hold what was written.
        let (changed, _) = self.reactive.borrow_mut().sync_shared();
        if !changed {
            return false;
        }
        let limits = self.limits;
        let reactive = std::rc::Rc::clone(&self.reactive);
        self.lua.enter(|ctx| {
            if let Err(message) = crate::api::ui::bindings::flush_reactive(&reactive, ctx, limits) {
                reactive
                    .borrow_mut()
                    .log(crate::LogLevel::Warn, format!("shared value: {message}"));
            }
        });
        true
    }
}
