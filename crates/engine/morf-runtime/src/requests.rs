//! What handlers asked of the platform -- a key typed for them, an input
//! method's edits, a capture, the clipboard, a drag -- queued for the host
//! to carry out, and what the host hands back.

/// Deferred virtual keyboard request produced by Lua.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VirtualKeyboardRequest {
    /// One evdev keycode state change.
    Key { keycode: u32, pressed: bool },
    /// XKB modifier masks and layout group.
    Modifiers {
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    },
}

/// Deferred input-method-v2 request produced by Lua.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputMethodRequest {
    /// Inserts committed UTF-8 text.
    Commit(String),
    /// Replaces the preedit string and cursor range.
    Preedit { text: String, begin: i32, end: i32 },
    /// Deletes byte ranges around the cursor.
    Delete { before: u32, after: u32 },
}

/// Deferred text-input-v3 state request produced by Lua.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextInputRequest {
    Disable,
    Surrounding {
        text: String,
        cursor: i32,
        anchor: i32,
    },
    ContentType {
        hints: u32,
        purpose: u32,
    },
    CursorRect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
}

/// One compositor output capture delivered to Lua.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Screencopy {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Bytes between adjacent rows.
    pub stride: u32,
    /// Shared-memory pixel format name.
    pub format: String,
    /// Whether rows are ordered bottom-to-top.
    pub y_invert: bool,
    /// Whether the picture is on the GPU, with `pixels` empty.
    pub gpu: bool,
    /// A source string `ui.Image` resolves, holding this capture's pixels.
    ///
    /// The point of it: a configuration that wants the picture on screen sets
    /// `ui.Image { source = frame.source }` and is done. Without this the
    /// capture protocols hand over bytes with nowhere to go — `ui.Image`
    /// resolves paths, so showing one meant encoding a file and reading it
    /// back, megabytes per thumbnail per refresh to move pixels already in
    /// memory.
    ///
    /// Named after the request, so a thumbnail that refreshes replaces itself.
    /// `pixels` is still there for a configuration that wants the bytes.
    pub source: String,
    /// Captured bytes including stride padding.
    pub pixels: Vec<u8>,
}

/// Correlated capture request queued by Lua.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreencopyRequest {
    /// Runtime-local request identifier.
    pub id: u64,
    /// Whether the compositor should include the cursor image.
    pub include_cursor: bool,
    /// A window to capture instead of the output, by the identifier
    /// `morf.windows` reported.
    ///
    /// By identifier rather than index or title: an index means something else
    /// the moment a window opens, and two windows of one application share a
    /// title as readily as an app id.
    pub window: Option<String>,
    /// Whether the picture should stay on the GPU.
    ///
    /// The compositor then draws into memory the renderer exported, and the
    /// frame's `source` is a texture rather than pixels: nothing is copied
    /// out and nothing uploaded back. Honoured where the compositor and the
    /// GPU allow it, and quietly shared memory where they do not.
    pub gpu: bool,
    /// The name the picture is published under, when the caller chose one.
    ///
    /// `frame.source` is then `memory:capture/<name>` or `gpu:capture/<name>`,
    /// and a later capture under the same name replaces the picture rather
    /// than adding one: a thumbnail that refreshes holds one image, not one
    /// per refresh. Without a name the request's own id is used, and the
    /// picture stays until `morf.screencopy.release(source)`.
    pub name: Option<String>,
    /// The output to capture, by the name `morf.screens` reports; `None` is
    /// the shell's own output, or the first there is.
    pub output: Option<String>,
}

/// One selection the configuration asked to own.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardRequest {
    /// The bytes every offered type answers with.
    pub data: Vec<u8>,
    /// The type to offer them as; `None` is text, offered under every name
    /// text goes by.
    pub mime: Option<String>,
    /// The primary selection (middle-click paste) rather than the clipboard.
    pub primary: bool,
}

/// One read of an offer the configuration asked for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferReadRequest {
    /// Runtime-local request identifier the answer comes back under.
    pub id: u64,
    /// The offer, by the identifier the host gave it.
    pub offer: u64,
    /// The type, or a shorthand the host resolves: `text`, `image`, `uris`.
    pub mime: String,
}

/// A drag out of the shell the configuration asked to start.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DragRequest {
    /// Offered as text.
    pub text: Option<String>,
    /// Offered as `text/uri-list`.
    pub uris: Vec<String>,
    /// Local paths, offered as `file://` URIs in the same list.
    pub paths: Vec<String>,
    /// Any other type, with its bytes.
    pub data: Vec<(String, Vec<u8>)>,
}

/// Something on offer -- a selection or a drag -- as the host describes it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OfferDescription {
    /// The host's identifier, which a read names the offer by.
    pub id: u64,
    /// Every type the source can produce, in its own order.
    pub mime_types: Vec<String>,
    /// The type the target accepted, for a drag.
    pub accepted: Option<String>,
    /// Parsed `text/uri-list`, for a drop that offered one.
    pub uris: Vec<String>,
    /// The local paths among those URIs.
    pub paths: Vec<String>,
    /// The text, for a drop that offered some.
    pub text: Option<String>,
}

/// What `morf.gamma.set` or `reset` asked of an output.
#[derive(Clone, Debug, PartialEq)]
pub struct GammaRequest {
    /// The output's name; `None` is the one this shell is on.
    pub output: Option<String>,
    /// `(temperature, brightness, gamma)`, or `None` to reset.
    pub set: Option<(f64, f64, f64)>,
}

/// What a configuration asked to do to a workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRequest {
    Activate(String),
    Remove(String),
    /// Move the workspace to the group on the named output.
    Assign {
        key: String,
        output: String,
    },
}

/// Something a configuration asked to do to another window.
#[derive(Clone, Debug)]
pub struct ToplevelRequest {
    pub identifier: String,
    pub action: String,
    pub value: bool,
    /// For `set_minimize_target`: x, y, width, height on the shell's surface.
    pub rect: Option<(i32, i32, i32, i32)>,
}

mod queue;

pub use queue::Requests;
