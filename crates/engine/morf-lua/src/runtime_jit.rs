//! luna's native tier, when morf is built with the `jit` feature.
//!
//! The VM compiles only when asked, and only outside its arena: a runtime
//! queues hot functions while it steps, and the host compiles one between
//! steps (`service`) and a bounded batch after a configuration loads
//! (`prepare`). Nothing here runs inside `Lua::enter`.
//!
//! `MORF_JIT=off` keeps a jit build interpreted, for comparing the two;
//! anything else (or unset) turns the native tier on where the target
//! supports it. Without the feature every function here does nothing.

use crate::types::Runtime;

/// Whether `MORF_JIT` asks for the interpreter alone.
#[cfg(feature = "jit")]
fn wanted() -> bool {
    !std::env::var("MORF_JIT").is_ok_and(|value| value == "off" || value == "0")
}

/// Turns the native tier on for a fresh VM, when built in and wanted.
pub(crate) fn configure(_lua: &mut luna::Lua) {
    #[cfg(feature = "jit")]
    if wanted() && _lua.jit_capabilities().supported_target {
        let config = luna::JitConfig {
            mode: luna::JitMode::Auto,
            ..luna::JitConfig::default()
        };
        if let Err(error) = _lua.set_jit_config(config) {
            eprintln!("morf: the native Lua tier is off: {error}");
        }
    }
}

impl Runtime {
    /// Compiles what a configuration just loaded registered, one bounded
    /// queue of it.
    pub(crate) fn prepare_jit(&mut self) {
        #[cfg(feature = "jit")]
        let _ = self.lua.prepare_jit();
    }

    /// Compiles at most one function the VM found hot since the last call.
    /// Between steps and between turns only: never inside the arena.
    pub(crate) fn service_jit(&mut self) {
        #[cfg(feature = "jit")]
        let _ = self.lua.service_jit();
    }

    /// What the native tier has done so far, for measuring: `None` without
    /// the feature or with it switched off.
    pub fn jit_report(&self) -> Option<String> {
        #[cfg(feature = "jit")]
        {
            if self.lua.jit_config().mode != luna::JitMode::Auto {
                return None;
            }
            let stats = self.lua.jit_stats();
            return Some(format!(
                "native {} / interpreted {} instructions, code {} bytes, compile failures {}",
                stats.native_instructions,
                stats.interpreted_instructions,
                stats.code_bytes,
                stats.compilation_failures
            ));
        }
        #[allow(unreachable_code)]
        None
    }
}
