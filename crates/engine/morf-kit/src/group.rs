//! Groups of presses: radio buttons, a segmented choice, a toggle group.
//!
//! Presses that share a `group` name and say `exclusive` keep at most one
//! checked: checking one unchecks the rest, and -- unless the group allows
//! none -- the checked one cannot be unchecked by pressing it again. The
//! arrows move between members, checking as they go, the way radio
//! buttons do. The registry (`module.rs`) holds the members; this is what
//! it asks of them.

use crate::Effects;

/// How a control belongs to a group.
#[derive(Clone, Debug, PartialEq)]
pub struct Membership {
    pub name: String,
    pub exclusive: bool,
    pub allow_none: bool,
}

/// What the registry may do to a member.
pub trait Member {
    fn checked(&self) -> bool;
    /// Checks or unchecks it as the group decides, answering what changed.
    fn set_checked_by_group(&mut self, checked: bool) -> Effects;
    fn enabled(&self) -> bool;
}

/// The step an arrow key takes through a group: -1, 1 or none.
pub fn arrow_step(name: &str, mirrored: bool) -> Option<i64> {
    match name {
        "Up" => Some(-1),
        "Down" => Some(1),
        "Left" => Some(if mirrored { 1 } else { -1 }),
        "Right" => Some(if mirrored { -1 } else { 1 }),
        _ => None,
    }
}
