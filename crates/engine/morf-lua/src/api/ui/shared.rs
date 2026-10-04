//! `morf.shared(name, initial)`: one value every screen's shell sees
//! (morf-runtime's `shared` keeps the values; this is the flush after a sync).

use crate::types::Runtime;

impl Runtime {
    /// [`SharedValues::sync`], then the bindings that read what came in.
    pub(crate) fn poll_shared(&mut self) -> bool {
        let (changed, published) = {
            let mut state = self.reactive.borrow_mut();
            let state = &mut *state;
            state.engine.shared.sync(&mut state.engine.reactive)
        };
        if published {
            // The other copies sleep until something wakes them.
            morf_io::wake_all();
        }
        if !changed {
            return false;
        }
        let limits = self.limits;
        let reactive = std::rc::Rc::clone(&self.reactive);
        self.lua.enter(|ctx| {
            if let Err(message) = crate::reactive_bindings::flush_reactive(&reactive, ctx, limits) {
                reactive
                    .borrow_mut()
                    .log(crate::LogLevel::Warn, format!("shared value: {message}"));
            }
        });
        true
    }
}
