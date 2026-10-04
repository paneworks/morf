use wayland_protocols::xdg::shell::client::xdg_positioner;

use crate::{PopupAnchor, PopupConstraints, PopupGravity};

/// Pixel encoding returned by a compositor screencopy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreencopyFormat {
    Argb8888,
    Xrgb8888,
}

/// One completed output capture in row-major shared-memory layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreencopyFrame {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Bytes between adjacent rows.
    pub stride: u32,
    /// Pixel channel encoding.
    pub format: ScreencopyFormat,
    /// Whether rows are ordered bottom-to-top.
    pub y_invert: bool,
    /// Captured bytes including stride padding.
    ///
    /// Empty when `dmabuf` is set: the picture is then in the buffer that was
    /// attached for this capture, on the GPU, and was never copied out.
    pub pixels: Vec<u8>,
    /// Whether the compositor drew into the attached dmabuf rather than into
    /// shared memory.
    pub dmabuf: bool,
}

/// A dmabuf to capture into, described the way `zwp_linux_dmabuf_v1` wants it.
///
/// One plane, because that is what the capture protocol carries; the renderer
/// that exported it says where the plane starts and how wide a row is, and
/// which modifier the driver laid it out with -- which the compositor needs
/// to read the memory the same way.
#[derive(Debug)]
pub struct CaptureBuffer<'a> {
    /// The dmabuf's file descriptor, borrowed for the duration of the call.
    pub fd: std::os::fd::BorrowedFd<'a>,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// DRM fourcc of the pixels.
    pub fourcc: u32,
    /// DRM format modifier the memory is laid out with.
    pub modifier: u64,
    /// Byte offset of the plane within the dmabuf.
    pub offset: u32,
    /// Bytes between adjacent rows.
    pub stride: u32,
}

pub(crate) fn popup_anchor(anchor: PopupAnchor) -> xdg_positioner::Anchor {
    match anchor {
        PopupAnchor::None => xdg_positioner::Anchor::None,
        PopupAnchor::Top => xdg_positioner::Anchor::Top,
        PopupAnchor::Bottom => xdg_positioner::Anchor::Bottom,
        PopupAnchor::Left => xdg_positioner::Anchor::Left,
        PopupAnchor::Right => xdg_positioner::Anchor::Right,
        PopupAnchor::TopLeft => xdg_positioner::Anchor::TopLeft,
        PopupAnchor::TopRight => xdg_positioner::Anchor::TopRight,
        PopupAnchor::BottomLeft => xdg_positioner::Anchor::BottomLeft,
        PopupAnchor::BottomRight => xdg_positioner::Anchor::BottomRight,
    }
}

pub(crate) fn popup_gravity(gravity: PopupGravity) -> xdg_positioner::Gravity {
    match gravity {
        PopupGravity::None => xdg_positioner::Gravity::None,
        PopupGravity::Top => xdg_positioner::Gravity::Top,
        PopupGravity::Bottom => xdg_positioner::Gravity::Bottom,
        PopupGravity::Left => xdg_positioner::Gravity::Left,
        PopupGravity::Right => xdg_positioner::Gravity::Right,
        PopupGravity::TopLeft => xdg_positioner::Gravity::TopLeft,
        PopupGravity::TopRight => xdg_positioner::Gravity::TopRight,
        PopupGravity::BottomLeft => xdg_positioner::Gravity::BottomLeft,
        PopupGravity::BottomRight => xdg_positioner::Gravity::BottomRight,
    }
}

pub(crate) fn popup_constraints(
    constraints: PopupConstraints,
) -> xdg_positioner::ConstraintAdjustment {
    let mut value = xdg_positioner::ConstraintAdjustment::empty();
    if constraints.slide_x {
        value |= xdg_positioner::ConstraintAdjustment::SlideX;
    }
    if constraints.slide_y {
        value |= xdg_positioner::ConstraintAdjustment::SlideY;
    }
    if constraints.flip_x {
        value |= xdg_positioner::ConstraintAdjustment::FlipX;
    }
    if constraints.flip_y {
        value |= xdg_positioner::ConstraintAdjustment::FlipY;
    }
    if constraints.resize_x {
        value |= xdg_positioner::ConstraintAdjustment::ResizeX;
    }
    if constraints.resize_y {
        value |= xdg_positioner::ConstraintAdjustment::ResizeY;
    }
    value
}

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

/// One workspace, as `ext-workspace-v1` describes it.
///
/// Compositor-neutral by construction: nothing here is Hyprland's or sway's
/// vocabulary, because the protocol is what both of them speak.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceInfo {
    /// The field to act on, and the one `activate` takes.
    ///
    /// Unique, and lives exactly as long as the workspace does. Not the name --
    /// names are for people, are not unique, and change.
    pub key: String,
    /// The compositor's own cross-session id, when it offers one.
    ///
    /// Optional in the protocol and empty on compositors that send none, so it
    /// is no use as a key. What it is good for is remembering a preference
    /// against a workspace between sessions, which is exactly what the protocol
    /// says it is for.
    pub id: String,
    /// What to show a person, which is often a number.
    pub name: String,
    /// Where it sits in the compositor's arrangement, however many dimensions
    /// that has. What they mean is the compositor's business; what a shell does
    /// with them is sort by them.
    pub coordinates: Vec<u32>,
    /// The output whose group it belongs to, so a per-screen bar can show its
    /// own workspaces rather than all of them.
    pub output: String,
    pub active: bool,
    /// The workspace is asking for attention.
    pub urgent: bool,
    /// The compositor would rather it were not listed.
    pub hidden: bool,
    /// Whether `activate` will do anything. A compositor may list a workspace
    /// it will not switch to, and a bar that offers the click anyway is a bar
    /// with a dead button on it.
    pub activatable: bool,
    /// Whether `remove` will, and whether `assign` will.
    pub removable: bool,
    pub assignable: bool,
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

