//! Where a popup goes relative to its parent, and how the compositor may
//! move it to keep it on screen.

use crate::InputRect;

/// Geometry for a popup anchored to a layer surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PopupConfig {
    /// Parent-surface rectangle used as the popup anchor.
    pub anchor: InputRect,
    /// Requested popup width in logical pixels.
    pub width: u32,
    /// Requested popup height in logical pixels.
    pub height: u32,
    /// Edge or corner of the anchor rectangle used for placement.
    pub anchor_edge: PopupAnchor,
    /// Popup edge or corner pulled toward the anchor.
    pub gravity: PopupGravity,
    /// Horizontal positioner offset in logical pixels.
    pub offset_x: i32,
    /// Vertical positioner offset in logical pixels.
    pub offset_y: i32,
    /// Compositor adjustments allowed when the popup would be constrained.
    pub constraints: PopupConstraints,
    /// Requests an explicit popup grab from the latest input serial.
    pub grab_focus: bool,
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            anchor: InputRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            width: 1,
            height: 1,
            anchor_edge: PopupAnchor::default(),
            gravity: PopupGravity::default(),
            offset_x: 0,
            offset_y: 0,
            constraints: PopupConstraints::default(),
            grab_focus: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PopupAnchor {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    #[default]
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PopupGravity {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PopupConstraints {
    pub slide_x: bool,
    pub slide_y: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

impl Default for PopupConstraints {
    fn default() -> Self {
        Self {
            slide_x: true,
            slide_y: true,
            flip_x: true,
            flip_y: true,
            resize_x: false,
            resize_y: false,
        }
    }
}
