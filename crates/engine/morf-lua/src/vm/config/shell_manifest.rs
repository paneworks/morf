//! What a shell needs of the engine, from `morf.lua` at its root:
//!
//!     return { name = "eva", version = "0.1.0", morf = ">=0.3.0", tested = "0.3" }
//!
//! Read as data, before any of the shell runs, so an engine too old for the
//! shell still says why it will not run it.

use super::*;
use crate::vm::permissions::run_data;
use crate::vm::requirement::{ENGINE, beyond, satisfies};
use luna::Value as LuaValue;

/// The file a shell keeps it in.
const SHELL_MANIFEST: &str = "morf.lua";

impl Runtime {
    /// Refuses a configuration whose `morf.lua` asks for a newer engine,
    /// and warns when this one is past the release line it was tested on.
    /// A configuration without one runs as it is.
    pub fn check_shell_manifest(&mut self, config: &Path) -> Result<(), String> {
        let Some(path) = find(config) else {
            return Ok(());
        };
        let limits = self.limits;
        let [name, version, wanted, tested] = self.lua.enter(|ctx| {
            let table = run_data(ctx, &path, limits, "a shell manifest")?;
            let text = |key: &str| match table.get_value(ctx, key) {
                LuaValue::Nil => Ok(None),
                LuaValue::String(value) => Ok(Some(value.display_lossy().to_string())),
                _ => Err(format!("{}: `{key}` is a string", path.display())),
            };
            Ok::<_, String>([
                text("name")?,
                text("version")?,
                text("morf")?,
                text("tested")?,
            ])
        })?;
        let mut shell = name.unwrap_or_else(|| "this shell".to_owned());
        if let Some(version) = version {
            shell = format!("{shell} {version}");
        }
        let invalid = |error: String| format!("{}: {error}", path.display());
        if let Some(wanted) = wanted
            && !satisfies(&wanted, ENGINE).map_err(invalid)?
        {
            return Err(format!(
                "{shell} needs morf {wanted}; this is morf {ENGINE}"
            ));
        }
        if let Some(tested) = tested
            && beyond(&tested, ENGINE).map_err(invalid)?
        {
            self.warn(format!(
                "{shell} was tested up to morf {tested}; this is morf {ENGINE}"
            ));
        }
        Ok(())
    }
}

/// `morf.lua` beside the configuration, or one folder up: a shell whose
/// parts each have a folder of their own keeps it at its root.
fn find(config: &Path) -> Option<PathBuf> {
    config
        .parent()?
        .ancestors()
        .take(2)
        .map(|dir| dir.join(SHELL_MANIFEST))
        .find(|path| path.is_file() && path != config)
}
