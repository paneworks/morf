//! `morf.kit.native`: the widget archetypes of morf-kit, as a Lua module.

/// Makes `morf.kit.native` available to every runtime made afterwards.
pub fn register() {
    crate::register_extension(install);
}

fn install(runtime: &mut crate::Runtime) {
    runtime.add_native_module("morf.kit.native", morf_kit::native_module());
}
