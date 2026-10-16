//! The service names a manifest lists under `services`: the library's
//! catalog, `lib.services.catalog`, the same file the service registry
//! binds from, read as data the way a manifest is.

use luna::{Context, Value as LuaValue};
use std::path::Path;

use super::manifest::run_data;
use crate::vm::types::Limits;

/// The module the catalog is read from.
pub const SERVICE_CATALOG: &str = "lib.services.catalog";

/// One service `morf.service` can bind: its name and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceEntry {
    pub name: String,
    pub holds: String,
}

/// The catalog at `path`, sorted by name.
pub(crate) fn read_catalog<'gc>(
    ctx: Context<'gc>,
    path: &Path,
    limits: Limits,
) -> Result<Vec<ServiceEntry>, String> {
    let table = run_data(ctx, path, limits, "the service catalog")?;
    let mut entries = Vec::new();
    for (key, value) in table.iter(ctx) {
        let (LuaValue::String(name), LuaValue::Table(entry)) = (key, value) else {
            return Err(format!(
                "{}: each service is a name and a table",
                path.display()
            ));
        };
        let holds = match entry.get_value(ctx, "holds") {
            LuaValue::String(holds) => holds.display_lossy().to_string(),
            _ => String::new(),
        };
        entries.push(ServiceEntry {
            name: name.display_lossy().to_string(),
            holds,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// The error a name outside `catalog` earns, in the registry's words.
pub fn unknown_service(name: &str, catalog: &[ServiceEntry]) -> Option<String> {
    if catalog.iter().any(|entry| entry.name == name) {
        return None;
    }
    let names: Vec<&str> = catalog.iter().map(|entry| entry.name.as_str()).collect();
    Some(format!(
        "unknown service `{name}` (registered: {})",
        names.join(", ")
    ))
}
