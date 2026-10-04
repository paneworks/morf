use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use smithay_client_toolkit::shell::xdg::window::Window;
use std::error::Error as StdError;
use std::fmt;
use wayland_client::protocol::{wl_subsurface, wl_surface};
use wayland_client::{Connection, EventQueue};
use wayland_protocols::xdg::shell::client::xdg_toplevel;

use crate::backend::wayland::state_types::*;
// The neutral types, under the paths the backend has always used them by.
pub(crate) use crate::{data::*, event::*, input::*, kind::*, output::*, positioner::*, window::*};

impl Edge {
    pub(crate) fn protocol(self) -> xdg_toplevel::ResizeEdge {
        match self {
            Self::Top => xdg_toplevel::ResizeEdge::Top,
            Self::Bottom => xdg_toplevel::ResizeEdge::Bottom,
            Self::Left => xdg_toplevel::ResizeEdge::Left,
            Self::Right => xdg_toplevel::ResizeEdge::Right,
            Self::TopLeft => xdg_toplevel::ResizeEdge::TopLeft,
            Self::TopRight => xdg_toplevel::ResizeEdge::TopRight,
            Self::BottomLeft => xdg_toplevel::ResizeEdge::BottomLeft,
            Self::BottomRight => xdg_toplevel::ResizeEdge::BottomRight,
        }
    }
}

/// Wayland connection or protocol setup failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaylandError(pub(crate) String);

impl fmt::Display for WaylandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StdError for WaylandError {}

/// Live layer surface and its event queue.
pub struct LayerClient {
    pub(crate) connection: Connection,
    pub(crate) queue: EventQueue<LayerState>,
    pub(crate) state: LayerState,
}

/// The shell role a primary surface is actually wearing.
///
/// morf wants a layer surface: it is the protocol built for shells, and it is
/// what gives an anchored bar, an exclusive zone and keyboard focus that a
/// toplevel cannot ask for. But `wlr-layer-shell` is an optional extension, and
/// kiosk compositors do not carry it — `cage`, which is what greetd runs a
/// greeter inside, offers only `xdg-shell`. Making the bind fatal meant morf
/// refused to start there at all, which is the wrong trade: a greeter that
/// covers the screen is precisely the case where a fullscreen toplevel is
/// indistinguishable from the layer surface it is standing in for, because the
/// compositor gives its single client the whole output regardless.
///
/// So the layer role is preferred and the toplevel is the fallback — for the
/// primary surface only. Every other layer surface (a dock, a menu, an OSD)
/// becomes a `wl_subsurface` of that toplevel, placed inside it by its anchors,
/// margins and size exactly where layer-shell would put it on an output the
/// primary's size (`placement`). Opening each as a toplevel of its own
/// instead stacked unrelated fullscreen windows, and dropped every anchor.
pub(crate) enum ShellSurface {
    /// A `wlr-layer-shell` surface: what a shell wants.
    Layer(LayerSurface),
    /// A fullscreen xdg toplevel, standing in for the primary surface where
    /// there is no layer-shell.
    Window(Box<Window>),
    /// A desynchronised subsurface of that toplevel, standing in for any other
    /// layer surface where there is no layer-shell.
    Subsurface(SubsurfaceShell),
}

/// A layer surface drawn as a subsurface of the fallback toplevel.
pub(crate) struct SubsurfaceShell {
    pub(crate) surface: wl_surface::WlSurface,
    pub(crate) subsurface: wl_subsurface::WlSubsurface,
}

impl Drop for SubsurfaceShell {
    fn drop(&mut self) {
        // The role object first: destroying it unmaps the surface from its
        // parent, and the surface itself goes after, as sctk's own shell
        // surfaces do on drop.
        self.subsurface.destroy();
        self.surface.destroy();
    }
}

impl ShellSurface {
    /// The underlying `wl_surface`, whichever role wraps it.
    pub(crate) fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            Self::Layer(layer) => layer.wl_surface(),
            Self::Window(window) => window.wl_surface(),
            Self::Subsurface(sub) => &sub.surface,
        }
    }

    /// The layer surface, when this really is one.
    ///
    /// Callers use this for the things only layer-shell can do — re-anchoring,
    /// the exclusive zone, parenting a popup — and skip them otherwise, since a
    /// toplevel has no equivalent to skip *to*.
    pub(crate) fn as_layer(&self) -> Option<&LayerSurface> {
        match self {
            Self::Layer(layer) => Some(layer),
            Self::Window(_) | Self::Subsurface(_) => None,
        }
    }

    /// The fallback toplevel, when this is the primary standing in for one.
    pub(crate) fn as_window(&self) -> Option<&Window> {
        match self {
            Self::Window(window) => Some(window),
            Self::Layer(_) | Self::Subsurface(_) => None,
        }
    }

    /// The subsurface, when this is a layer surface standing in as one.
    pub(crate) fn as_subsurface(&self) -> Option<&SubsurfaceShell> {
        match self {
            Self::Subsurface(sub) => Some(sub),
            Self::Layer(_) | Self::Window(_) => None,
        }
    }

    /// Commits pending surface state.
    pub(crate) fn commit(&self) {
        match self {
            Self::Layer(layer) => layer.commit(),
            Self::Window(window) => window.commit(),
            Self::Subsurface(sub) => sub.surface.commit(),
        }
    }
}
