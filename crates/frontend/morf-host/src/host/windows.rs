//! Every live window a configuration declared beside its own surface --
//! popups, toplevels and extra layer surfaces -- in one map, by the window
//! id the backend knows it by.

use std::collections::HashMap;

use morf_app::WindowId;

use crate::surface_layers::{window_layer_id, window_surface_id};
use crate::surfaces::Window;

/// Which kind of declared window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Popup,
    Toplevel,
    Layer,
}

impl Kind {
    /// The backend's id for the window the configuration declared as `id`
    /// (a layer surface goes one past its id: the shell's own is layer 0).
    pub fn window(self, id: u64) -> WindowId {
        match self {
            Self::Popup => WindowId::Popup(id),
            Self::Toplevel => WindowId::Toplevel(id),
            Self::Layer => WindowId::Layer(window_layer_id(id)),
        }
    }
}

/// The declaration id and kind behind a backend window id, when it is one
/// of the declared windows.
fn declared(window: WindowId) -> Option<(Kind, u64)> {
    match window {
        WindowId::Popup(id) => Some((Kind::Popup, id)),
        WindowId::Toplevel(id) => Some((Kind::Toplevel, id)),
        WindowId::Layer(layer) => window_surface_id(layer).map(|id| (Kind::Layer, id)),
        WindowId::Lock(_) => None,
    }
}

/// The declared windows that are live.
#[derive(Default)]
pub struct Windows {
    windows: HashMap<WindowId, Window>,
}

impl Windows {
    pub fn get(&self, kind: Kind, id: u64) -> Option<&Window> {
        self.windows.get(&kind.window(id))
    }

    pub fn get_mut(&mut self, kind: Kind, id: u64) -> Option<&mut Window> {
        self.windows.get_mut(&kind.window(id))
    }

    pub fn insert(&mut self, kind: Kind, id: u64, window: Window) {
        self.windows.insert(kind.window(id), window);
    }

    pub fn remove(&mut self, kind: Kind, id: u64) -> Option<Window> {
        self.windows.remove(&kind.window(id))
    }

    pub fn contains(&self, kind: Kind, id: u64) -> bool {
        self.windows.contains_key(&kind.window(id))
    }

    /// The declaration ids of the live windows of `kind`.
    pub fn ids(&self, kind: Kind) -> Vec<u64> {
        self.windows
            .keys()
            .filter_map(|window| declared(*window))
            .filter(|(of, _)| *of == kind)
            .map(|(_, id)| id)
            .collect()
    }

    /// The live windows of `kind`, with their declaration ids.
    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = (u64, &Window)> {
        self.windows.iter().filter_map(move |(window, live)| {
            declared(*window)
                .filter(|(of, _)| *of == kind)
                .map(|(_, id)| (id, live))
        })
    }

    /// The live windows of `kind`, to change.
    pub fn of_kind_mut(&mut self, kind: Kind) -> impl Iterator<Item = (u64, &mut Window)> {
        self.windows.iter_mut().filter_map(move |(window, live)| {
            declared(*window)
                .filter(|(of, _)| *of == kind)
                .map(|(_, id)| (id, live))
        })
    }

    /// The live window the backend knows as `window`.
    pub fn by_window(&self, window: WindowId) -> Option<&Window> {
        let (kind, id) = declared(window)?;
        self.get(kind, id)
    }

    pub fn values(&self) -> impl Iterator<Item = &Window> {
        self.windows.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Window> {
        self.windows.values_mut()
    }

    pub fn clear(&mut self) {
        self.windows.clear();
    }
}
