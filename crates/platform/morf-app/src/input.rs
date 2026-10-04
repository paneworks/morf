//! Input as the windows receive it: modifiers held, text input and input
//! method batches, and the rectangles a window takes the pointer in.

/// Which modifier keys are held.
///
/// The keysym already says what a key means with Shift applied; this says
/// what else is held, which is what tells Ctrl+C from C.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
}

/// Atomically committed text-input-v3 edit batch.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextInputState {
    /// Whether this client's surface has text-input focus.
    pub focused: bool,
    /// Current preedit string when changed in this batch.
    pub preedit: Option<String>,
    /// Preedit cursor start in bytes when supplied.
    pub preedit_begin: i32,
    /// Preedit cursor end in bytes when supplied.
    pub preedit_end: i32,
    /// UTF-8 text committed by the input method.
    pub commit: Option<String>,
    /// Bytes to delete before the cursor.
    pub delete_before: u32,
    /// Bytes to delete after the cursor.
    pub delete_after: u32,
    /// Serial supplied by the compositor done event.
    pub serial: u32,
}

/// Atomically committed input-method context.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InputMethodState {
    /// Whether a focused text input requested this input method.
    pub active: bool,
    /// UTF-8 text around the application cursor when supported.
    pub surrounding_text: Option<String>,
    /// Byte offset of the cursor in surrounding text.
    pub cursor: u32,
    /// Byte offset of the selection anchor in surrounding text.
    pub anchor: u32,
    /// Number of compositor done events received.
    pub serial: u32,
}

/// Integer surface-local rectangle used to construct an input region.
///
/// The same four fields `morf_value::region` already defines, so it is that type
/// rather than a copy of it. Two names for one shape meant a field-by-field
/// rebuild of every rectangle on the way from the region rasteriser to the
/// compositor, allocated fresh on each update.
pub type InputRect = morf_value::region::Rect;
