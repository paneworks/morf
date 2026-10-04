//! Everybody else's windows: listed through `ext-foreign-toplevel-list`,
//! acted on through `wlr-foreign-toplevel-management`.

mod control;
mod list;

pub(crate) use control::ToplevelControl;

use crate::Desktop;

/// One window on the compositor, as `ext-foreign-toplevel-list-v1` describes it.
///
/// Deliberately thin. The protocol reports what a window *is* — a title, an
/// application, a stable name — and nothing about where it is or what it is
/// doing, because that is the compositor's business and not a client's. An
/// overview or a task switcher wants exactly this list, plus a capture of each,
/// and no more.
/// Something to do to another window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToplevelAction {
    /// Focus it, on this client's seat.
    Activate,
    /// Ask it to close, which is a request and not a kill.
    Close,
    Maximized(bool),
    Minimized(bool),
    Fullscreen(bool),
    /// Where on the shell's own surface the window's task-bar entry is, so a
    /// compositor that animates minimize has somewhere to animate towards.
    MinimizeTarget {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
}


#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToplevelInfo {
    /// Stable for the life of the window, and unique on this compositor.
    ///
    /// The one field to key on. Titles change while you read them and two
    /// windows of the same application share an app id.
    pub identifier: String,
    /// What the window calls itself, which is usually what to show a person.
    pub title: String,
    /// Which application it belongs to, matching a desktop entry's id where the
    /// application sets it — which is how an overview finds an icon.
    pub app_id: String,
    /// Whether this window is the focused one.
    ///
    /// These four come from `wlr-foreign-toplevel-management` rather than from
    /// the enumeration protocol, which reports no state at all. On a compositor
    /// offering only the newer protocol they are all false and
    /// [`Self::controllable`] is false with them, which is how a configuration
    /// tells "not maximized" from "never said".
    pub activated: bool,
    pub maximized: bool,
    pub minimized: bool,
    pub fullscreen: bool,
    /// Whether this window can be acted on — activated, closed, maximized.
    ///
    /// False when the compositor offers no control protocol, and false when it
    /// does but this window could not be matched to a handle in it. A task bar
    /// should draw an entry either way and only offer the click for this.
    pub controllable: bool,
    /// The names of the outputs the window is on, in the order it entered
    /// them; empty when the compositor offers no control protocol or never
    /// said. What a dock on one screen filters its windows by.
    pub outputs: Vec<String>,
    /// The identifier of the window this one belongs to (a dialog's
    /// parent), when the compositor says so (control protocol version 3).
    pub parent: Option<String>,
}

impl Desktop {
    /// What can be done to another window, by identifier.
    ///
    /// One entry point rather than five methods, because every one of them is
    /// the same lookup followed by one request, and the lookup is the part that
    /// can fail. `false` means the window is not controllable — see
    /// [`ToplevelInfo::controllable`].
    pub fn control_toplevel(&mut self, identifier: &str, action: ToplevelAction) -> bool {
        if !self.supports_toplevel_control() {
            return false;
        }
        let Some(handle) = self.toplevel_control_handle(identifier) else {
            return false;
        };
        match action {
            ToplevelAction::Activate => {
                // Activation is scoped to a seat: the protocol wants to know
                // *whose* focus is moving, and a client with no seat has no
                // business moving anybody's.
                let Some(seat) = self.state.seats.seats().next() else {
                    return false;
                };
                handle.activate(&seat);
            }
            ToplevelAction::Close => handle.close(),
            ToplevelAction::Maximized(true) => handle.set_maximized(),
            ToplevelAction::Maximized(false) => handle.unset_maximized(),
            ToplevelAction::Minimized(true) => handle.set_minimized(),
            ToplevelAction::Minimized(false) => handle.unset_minimized(),
            ToplevelAction::Fullscreen(true) => handle.set_fullscreen(None),
            ToplevelAction::Fullscreen(false) => handle.unset_fullscreen(),
            ToplevelAction::MinimizeTarget {
                x,
                y,
                width,
                height,
            } => {
                // Relative to the shell's own primary surface: that is where
                // the task bar is, and a rectangle on any other surface would
                // be a window flying towards the wrong thing.
                let Some(surface) = &self.state.shell_surface else {
                    return false;
                };
                handle.set_rectangle(surface, x, y, width, height);
            }
        }
        true
    }

    /// Whether this compositor lets a client act on other windows at all.
    ///
    /// Separate from a window's own `controllable`, which additionally says
    /// whether *that* window was matched to a handle.
    pub fn supports_toplevel_control(&self) -> bool {
        self.state.toplevel_control_manager.is_some()
    }

    /// Finds the control handle for a window named by the enumeration protocol.
    ///
    /// Matched on application and title, because nothing correlates the two
    /// protocols' handles — see the module note on `toplevel_control`.
    fn toplevel_control_handle(
        &self,
        identifier: &str,
    ) -> Option<&wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1>
    {
        let listed = self
            .state
            .toplevels
            .values()
            .find(|info| info.identifier == identifier)?;
        let key = self
            .state
            .toplevel_controls
            .iter()
            .find(|(_, control)| control.app_id == listed.app_id && control.title == listed.title)
            .map(|(key, _)| key)?;
        self.state.toplevel_control_handles.get(key)
    }

    /// Returns whether shared-memory output capture is available.
    /// Every window the compositor currently knows about.
    ///
    /// Sorted by identifier so the order is the same on two consecutive calls:
    /// the protocol makes no promise about it, and a list that reshuffles under
    /// a person's cursor is worse than one in an arbitrary but stable order.
    ///
    /// Windows still being described are left out. A handle arrives before its
    /// title does, and a task switcher showing a blank row for half a frame is
    /// a worse answer than showing nothing for that frame.
    pub fn toplevels(&self) -> Vec<ToplevelInfo> {
        let mut toplevels: Vec<ToplevelInfo> = self
            .state
            .toplevels
            .values()
            .filter(|toplevel| !toplevel.identifier.is_empty())
            .cloned()
            .collect();
        // The control protocol's view folded onto the enumeration's, matched on
        // application and title. A window with no match keeps its defaults and
        // stays `controllable: false`, which is the honest answer: the state is
        // not false, it is unknown.
        let identifiers: Vec<(String, String, String)> = toplevels
            .iter()
            .map(|toplevel| {
                (
                    toplevel.app_id.clone(),
                    toplevel.title.clone(),
                    toplevel.identifier.clone(),
                )
            })
            .collect();
        for toplevel in &mut toplevels {
            let Some(control) = self.state.toplevel_controls.values().find(|control| {
                control.app_id == toplevel.app_id && control.title == toplevel.title
            }) else {
                continue;
            };
            toplevel.activated = control.activated;
            toplevel.maximized = control.maximized;
            toplevel.minimized = control.minimized;
            toplevel.fullscreen = control.fullscreen;
            toplevel.controllable = true;
            toplevel.outputs = control
                .outputs
                .iter()
                .filter_map(|output| self.state.outputs.info(output).and_then(|info| info.name))
                .collect();
            // The parent is a control handle; its window is found the way this
            // one was, by application and title.
            toplevel.parent = control
                .parent
                .as_ref()
                .and_then(|parent| self.state.toplevel_controls.get(parent))
                .and_then(|parent| {
                    identifiers
                        .iter()
                        .find(|(app_id, title, _)| {
                            *app_id == parent.app_id && *title == parent.title
                        })
                        .map(|(_, _, identifier)| identifier.clone())
                });
        }
        toplevels.sort_by(|a, b| a.identifier.cmp(&b.identifier));
        toplevels
    }

    /// Whether the window list changed since this was last called.
    ///
    /// Taking the flag rather than reading it, so a caller that acts on a
    /// change cannot act on it twice.
    pub fn take_toplevels_changed(&mut self) -> bool {
        std::mem::take(&mut self.state.toplevels_changed)
    }

    /// Whether the compositor reports its windows at all.
    pub fn supports_toplevels(&self) -> bool {
        self.state.toplevel_list.is_some()
    }
}
