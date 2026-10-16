//! Running a configuration only to learn what it imports: every gated API
//! refused, no host, no time passing. `morf approve` reads a configuration
//! this way.

use std::path::Path;

use morf_app::backend::headless::virtual_outputs;
use morf_lua::{Limits, PermissionMode, Runtime};

use crate::process::supervisor::{
    LoadPolicy, execute_config_with, lua_screen, lua_screens, store_outputs,
};

/// The runtime after loading `path` with every gated call refused, and how
/// the load went. The runtime is returned either way: a configuration that
/// stops part way has still named the modules it reached.
pub fn discover(path: &Path, approvals: Option<&Path>) -> (Runtime, Result<(), String>) {
    let screens = virtual_outputs(1, (1920, 1080), 1);
    store_outputs(&screens);
    let (limits, _) = Limits::from_env();
    let mut runtime = Runtime::for_screen(limits, lua_screen(&screens[0]));
    runtime.use_virtual_clock();
    runtime.set_capabilities(&[("headless".to_owned(), "true".to_owned())]);
    runtime.set_permission_mode(PermissionMode::RefuseAll);
    if let Some(approvals) = approvals {
        runtime.set_approvals_path(approvals.to_path_buf());
    }
    let result = std::fs::read(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))
        .and_then(|source| {
            execute_config_with(
                &mut runtime,
                path,
                &source,
                LoadPolicy::default(),
                &lua_screens(&screens),
                &[],
            )
        });
    (runtime, result)
}
