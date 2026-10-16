//! The engine-owned reload-error band: the host's guarantee that a
//! failed reload is visible even when the configuration never
//! registered `on_reload_failed`.

use super::*;

impl Runtime {
    /// Whether the engine-owned reload-error band is currently shown.
    pub fn reload_error_shown(&self) -> bool {
        self.reload_error_band
            .borrow()
            .is_some_and(|band| self.scene().contains(band))
    }

    /// The text the reload-error band is showing, while it is shown.
    pub fn reload_error_text(&self) -> Option<String> {
        let band = (*self.reload_error_band.borrow())?;
        let scene = self.scene();
        let label = *scene.children(band).ok()?.first()?;
        match scene.current(label, "text") {
            Ok(morf_scene::Value::String(text)) => Some(text.clone()),
            _ => None,
        }
    }

    /// Shows the engine-owned band across the top of the primary root:
    /// the error, and where it came from. Called by the host only when
    /// the configuration registered no `on_reload_failed` handler for
    /// this failure, so a configuration that reports reload failures
    /// itself never sees both. A successful reload replaces this whole
    /// runtime, which is what dismisses the band then.
    pub fn show_reload_error(&mut self, error: &str, path: &Path) {
        self.clear_reload_error();
        let Some(root) = self.band_parent() else {
            self.reactive.borrow_mut().log(
                LogLevel::Warn,
                format!("reload failed and there is no primary root to show it on: {error}"),
            );
            return;
        };
        let state = &self.reactive;
        let band = crate::api::ui::scene::create_node(state, morf_scene::Element::Rect);
        let label = crate::api::ui::scene::create_node(state, morf_scene::Element::Text);
        let message = format!("reload failed in {}: {error}", path.display());
        let mut state = state.borrow_mut();
        let set = |state: &mut ReactiveState,
                   node: NodeHandle,
                   property: &str,
                   value: morf_scene::Value| {
            if let Err(message) =
                crate::api::ui::scene::assign_scene_property(state, node, property, value)
            {
                state.log(
                    LogLevel::Warn,
                    format!("reload error band {property}: {message}"),
                );
            }
        };
        set(
            &mut state,
            band,
            "id",
            morf_scene::Value::String("morf-reload-error".to_owned()),
        );
        set(
            &mut state,
            band,
            "color",
            morf_scene::Value::Color(morf_scene::Color {
                red: 0.45,
                green: 0.1,
                blue: 0.1,
                alpha: 1.0,
            }),
        );
        set(&mut state, band, "x", morf_scene::Value::Number(0.0));
        set(&mut state, band, "y", morf_scene::Value::Number(0.0));
        set(&mut state, band, "height", morf_scene::Value::Number(30.0));
        set(
            &mut state,
            band,
            "anchors",
            morf_scene::Value::Map(std::collections::BTreeMap::from([
                ("left".to_owned(), morf_scene::Value::Bool(true)),
                ("right".to_owned(), morf_scene::Value::Bool(true)),
                ("top".to_owned(), morf_scene::Value::Bool(true)),
            ])),
        );
        set(&mut state, band, "z", morf_scene::Value::Number(1.0e6));
        set(
            &mut state,
            label,
            "text",
            morf_scene::Value::String(message),
        );
        set(
            &mut state,
            label,
            "color",
            morf_scene::Value::Color(morf_scene::Color {
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0,
            }),
        );
        set(&mut state, label, "x", morf_scene::Value::Number(8.0));
        set(&mut state, label, "y", morf_scene::Value::Number(6.0));
        if let Err(message) = state.scene_mut().reparent(label, Some(band)) {
            state.log(LogLevel::Warn, format!("reload error band: {message}"));
        }
        if let Err(message) = state.scene_mut().reparent(band, Some(root)) {
            state.log(LogLevel::Warn, format!("reload error band: {message}"));
        }
        drop(state);
        *self.reload_error_band.borrow_mut() = Some(band);
    }

    /// Takes the reload-error band down, if it is up.
    pub fn clear_reload_error(&mut self) {
        let band = self.reload_error_band.borrow_mut().take();
        if let Some(band) = band
            && self.scene().contains(band)
        {
            let _ = self.scene_mut().remove(band);
        }
    }

    /// The root the band hangs from: the largest non-window root, the
    /// same choice the host makes for the primary surface.
    fn band_parent(&self) -> Option<NodeHandle> {
        let scene = self.scene();
        let window_roots = self
            .window_surface_configs()
            .iter()
            .map(|surface| surface.root)
            .collect::<std::collections::HashSet<_>>();
        let mut best: Option<(NodeHandle, usize)> = None;
        for root in scene.roots() {
            if window_roots.contains(&root) {
                continue;
            }
            let mut size = 0;
            let mut pending = vec![root];
            while let Some(node) = pending.pop() {
                size += 1;
                if let Ok(children) = scene.children(node) {
                    pending.extend(children.iter().copied());
                }
            }
            if best.is_none_or(|(_, best_size)| size > best_size) {
                best = Some((root, size));
            }
        }
        best.map(|(root, _)| root)
    }
}
