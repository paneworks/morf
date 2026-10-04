//! The compositor's outputs: naming them, keeping the last list, and the
//! screens a runtime is told about.

use morf_lua::Screen;
use morf_app::Output;
use std::collections::BTreeMap;

pub fn named_screens(
    screens: &[Output],
) -> Result<BTreeMap<String, Output>, String> {
    screens
        .iter()
        .map(|screen| {
            screen
                .name
                .clone()
                .map(|name| (name, screen.clone()))
                .ok_or_else(|| format!("output {} has no compositor name", screen.id))
        })
        .collect()
}

/// Every output the compositor currently advertises, in the order it advertised
/// them.
///
/// One morf process drives every output, one worker thread each, so the output
/// topology is a fact about the process rather than per-worker state. The
/// supervisor is the only writer: it seeds this from its probe connection
/// before the first worker starts and refreshes it whenever a worker reports a
/// change. Workers read it when they load a configuration, which is what lets
/// `morf.screens` describe more than the one output a worker draws to.
pub static OUTPUTS: std::sync::Mutex<Vec<Output>> = std::sync::Mutex::new(Vec::new());

/// Records the compositor's output list, reporting whether it changed.
pub fn store_outputs(screens: &[Output]) -> bool {
    let mut outputs = OUTPUTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if outputs.as_slice() == screens {
        return false;
    }
    outputs.clear();
    outputs.extend_from_slice(screens);
    true
}

/// The recorded output list in the shape `morf.screens` is built from.
pub fn known_outputs() -> Vec<Screen> {
    let outputs = OUTPUTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    lua_screens(&outputs)
}

/// Converts compositor output descriptions into the Lua-facing shape, keeping
/// the order the compositor advertised them in.
pub fn lua_screens(screens: &[Output]) -> Vec<Screen> {
    screens.iter().map(lua_screen).collect()
}

/// An output with no compositor name cannot be addressed by a configuration,
/// but it still occupies the desktop, so it is described with an empty name
/// rather than dropped from the list.
pub fn lua_screen(screen: &Output) -> Screen {
    Screen {
        id: screen.id,
        name: screen.name.clone().unwrap_or_default(),
        make: screen.make.clone(),
        model: screen.model.clone(),
        description: screen.description.clone(),
        position: screen.position,
        width: screen.size.map(|size| size.0),
        height: screen.size.map(|size| size.1),
        physical_size: screen.physical_size,
        scale: screen.scale,
        transform: screen.transform.to_owned(),
    }
}
