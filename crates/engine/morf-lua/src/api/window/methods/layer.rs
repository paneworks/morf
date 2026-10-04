//! A `window.layer` handle's live layer settings: which may change, one
//! applied, and several applied at once by `window:configure`.

use super::*;

/// Layer settings a `window.layer` handle may change after creation.
///
/// Everything wlr-layer-shell lets a mapped surface change, plus `layer`
/// (moved in place where the compositor speaks version 2, rebuilt where it
/// does not) and the engine's own `mask` and `opaque`. The namespace names
/// the surface to the compositor and is fixed; reserve, backdrop and the
/// session lock belong to the shell's own surface.
pub(crate) const RUNTIME_LAYER_SETTINGS: &[&str] = &[
    "width",
    "height",
    "exclusive_zone",
    "margin_top",
    "margin_right",
    "margin_bottom",
    "margin_left",
    "anchors",
    "layer",
    "keyboard_focus",
    "mask",
    "opaque",
];

/// Applies one setting to a live `window.layer` surface, the way an
/// assignment to `morf.surface.<key>` does for the shell's own.
pub(crate) fn set_window_layer_setting<'gc>(
    ctx: Context<'gc>,
    state: &mut ReactiveState,
    id: u64,
    key: &str,
    value: LuaValue<'gc>,
) -> Result<(), HostError> {
    let surface = state
        .window_surfaces
        .get_mut(&id)
        .ok_or_else(|| HostError("window destroyed".into()))?;
    let WindowSurfaceKind::Layer(config) = &mut surface.kind else {
        return Err(HostError(format!(
            "`{key}` can only be set on a layer surface"
        )));
    };
    if !RUNTIME_LAYER_SETTINGS.contains(&key) {
        return Err(HostError(if key == "namespace" {
            "a layer surface's namespace is fixed when it is created".into()
        } else {
            format!("`{key}` cannot be set on a layer surface handle")
        }));
    }
    let changed = apply_layer_setting(ctx, config, key, value).map_err(HostError)?;
    state.window_surfaces_changed |= changed;
    Ok(())
}

/// `window:configure { key = value, ... }`: several layer settings at once,
/// validated before any lands so a bad one leaves the surface untouched.
pub(crate) fn window_configure_method<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (surface, settings): (UserRef<WindowSurfaceToken>, Table) = stack.consume(ctx)?;
        let mut state = state.borrow_mut();
        let mut entries = Vec::new();
        for (key, value) in settings.iter(ctx) {
            let LuaValue::String(key) = key else {
                return Err(HostError("layer settings must have string keys".into()).into());
            };
            entries.push((key.display_lossy().to_string(), value));
        }
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));
        let before = state
            .window_surfaces
            .get(&surface.id)
            .map(|surface| surface.kind.clone())
            .ok_or_else(|| HostError("window destroyed".into()))?;
        let changed_before = state.window_surfaces_changed;
        for (key, value) in entries {
            if let Err(error) = set_window_layer_setting(ctx, &mut state, surface.id, &key, value) {
                if let Some(target) = state.window_surfaces.get_mut(&surface.id) {
                    target.kind = before;
                }
                state.window_surfaces_changed = changed_before;
                return Err(error.into());
            }
        }
        Ok(CallbackReturn::Return)
    })
}
