use crate::vm::types::*;

impl Default for Runtime {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}
